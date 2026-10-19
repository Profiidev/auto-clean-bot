//! Discord integration: the bot's REST calls against a mocked Discord API.
mod common;

use std::{sync::Arc, time::Duration};

use auto_clean_bot::{
  cleanup::{cleanup_loop, run_cleanup},
  commands::{MAX_PURGE, purge_channel},
  db::DBTrait,
};
use chrono::{TimeDelta, Utc};
use common::{API, MockDiscord, message_json, sqlite_db};
use entity::{clean_message, prelude::*};
use sea_orm::{ActiveValue::Set, DatabaseConnection, prelude::*};
use serde_json::{Value, json};
use serenity::all::{ChannelId, MessageId, Timestamp};
use tokio::{sync::Notify, time::sleep};
use wiremock::{
  Mock, ResponseTemplate,
  matchers::{method, path, path_regex, query_param},
};

async fn track(db: &DatabaseConnection, channel: u64, message: u64, minutes_ago: i64) {
  let row = db
    .tables()
    .clean_channel()
    .get_channel(ChannelId::new(channel))
    .await
    .unwrap();
  clean_message::ActiveModel {
    id: Set(Uuid::new_v4()),
    channel: Set(row.id),
    message: Set(message as i64),
    created_at: Set(Utc::now().naive_utc() - TimeDelta::minutes(minutes_ago)),
  }
  .insert(db)
  .await
  .unwrap();
}

async fn tracked_messages(db: &DatabaseConnection) -> Vec<i64> {
  let mut ids: Vec<i64> = CleanMessage::find()
    .all(db)
    .await
    .unwrap()
    .into_iter()
    .map(|m| m.message)
    .collect();
  ids.sort();
  ids
}

async fn mock_delete_any(discord: &MockDiscord, status: u16) {
  Mock::given(method("DELETE"))
    .and(path_regex(r"^/api/v10/channels/\d+/messages/\d+$"))
    .respond_with(ResponseTemplate::new(status))
    .mount(&discord.server)
    .await;
}

// ---- auto-clean ----

#[tokio::test]
async fn run_cleanup_deletes_expired_messages_on_discord_and_db() {
  let discord = MockDiscord::start().await;
  mock_delete_any(&discord, 204).await;
  let db = sqlite_db().await;
  let table = db.tables().clean_channel();
  table.insert_channel(ChannelId::new(1), 2).await.unwrap();
  table.insert_channel(ChannelId::new(2), 2).await.unwrap();
  track(&db, 1, 10, 3).await;
  track(&db, 2, 20, 3).await;
  track(&db, 1, 11, 0).await;

  assert_eq!(run_cleanup(&discord.http, &db).await.unwrap(), 2);

  let mut calls = discord.calls().await;
  calls.sort();
  assert_eq!(
    calls,
    vec![
      format!("DELETE {API}/channels/1/messages/10"),
      format!("DELETE {API}/channels/2/messages/20"),
    ]
  );
  assert_eq!(tracked_messages(&db).await, vec![11]);
}

#[tokio::test]
async fn run_cleanup_sends_bot_token() {
  let discord = MockDiscord::start().await;
  mock_delete_any(&discord, 204).await;
  let db = sqlite_db().await;
  db.tables()
    .clean_channel()
    .insert_channel(ChannelId::new(1), 1)
    .await
    .unwrap();
  track(&db, 1, 10, 5).await;

  run_cleanup(&discord.http, &db).await.unwrap();

  let requests = discord.requests().await;
  assert_eq!(requests.len(), 1);
  assert_eq!(
    requests[0].headers.get("authorization").unwrap(),
    "Bot test-token"
  );
}

#[tokio::test]
async fn run_cleanup_drops_rows_even_when_discord_fails() {
  // message already deleted by a user, or missing permissions
  for status in [404, 403, 500] {
    let discord = MockDiscord::start().await;
    mock_delete_any(&discord, status).await;
    let db = sqlite_db().await;
    db.tables()
      .clean_channel()
      .insert_channel(ChannelId::new(1), 1)
      .await
      .unwrap();
    track(&db, 1, 10, 5).await;

    assert_eq!(run_cleanup(&discord.http, &db).await.unwrap(), 1);
    assert!(tracked_messages(&db).await.is_empty(), "status {status}");
  }
}

#[tokio::test]
async fn run_cleanup_nothing_expired_sends_no_requests() {
  let discord = MockDiscord::start().await;
  let db = sqlite_db().await;
  db.tables()
    .clean_channel()
    .insert_channel(ChannelId::new(1), 10)
    .await
    .unwrap();
  track(&db, 1, 10, 1).await;

  assert_eq!(run_cleanup(&discord.http, &db).await.unwrap(), 0);
  assert!(discord.calls().await.is_empty());
  assert_eq!(tracked_messages(&db).await, vec![10]);
}

#[tokio::test]
async fn cleanup_loop_deletes_after_notify() {
  let discord = MockDiscord::start().await;
  mock_delete_any(&discord, 204).await;
  let db = sqlite_db().await;
  db.tables()
    .clean_channel()
    .insert_channel(ChannelId::new(1), 1)
    .await
    .unwrap();

  let notify = Arc::new(Notify::new());
  let http = discord.http.clone();
  let task = tokio::spawn(cleanup_loop(http, db.clone(), notify.clone()));

  // loop is now sleeping "forever" (nothing tracked); an expired message plus notify wakes it
  sleep(Duration::from_millis(100)).await;
  track(&db, 1, 10, 5).await;
  notify.notify_waiters();

  for _ in 0..50 {
    if tracked_messages(&db).await.is_empty() {
      break;
    }
    sleep(Duration::from_millis(50)).await;
  }
  task.abort();

  assert!(tracked_messages(&db).await.is_empty());
  assert_eq!(
    discord.calls().await,
    vec![format!("DELETE {API}/channels/1/messages/10")]
  );
}

// ---- purge ----

async fn mock_messages(discord: &MockDiscord, channel: u64, limit: u8, messages: Vec<Value>) {
  Mock::given(method("GET"))
    .and(path(format!("{API}/channels/{channel}/messages")))
    .and(query_param("limit", limit.to_string()))
    .respond_with(ResponseTemplate::new(200).set_body_json(messages))
    .mount(&discord.server)
    .await;
}

async fn mock_bulk_delete(discord: &MockDiscord, channel: u64) {
  Mock::given(method("POST"))
    .and(path(format!(
      "{API}/channels/{channel}/messages/bulk-delete"
    )))
    .respond_with(ResponseTemplate::new(204))
    .mount(&discord.server)
    .await;
}

fn bulk_body(requests: &[wiremock::Request]) -> Value {
  let req = requests
    .iter()
    .find(|r| r.url.path().ends_with("/bulk-delete"))
    .expect("bulk delete request");
  serde_json::from_slice(&req.body).unwrap()
}

#[tokio::test]
async fn purge_bulk_deletes_recent_messages_including_reply() {
  let discord = MockDiscord::start().await;
  let now = Utc::now();
  mock_messages(
    &discord,
    5,
    3,
    vec![
      message_json(103, 5, now),
      message_json(102, 5, now - TimeDelta::hours(1)),
      message_json(101, 5, now - TimeDelta::days(13)),
    ],
  )
  .await;
  mock_bulk_delete(&discord, 5).await;

  let deleted = purge_channel(&discord.http, ChannelId::new(5), 2, Timestamp::now())
    .await
    .unwrap();

  assert_eq!(deleted, 3);
  assert_eq!(
    bulk_body(&discord.requests().await),
    json!({ "messages": ["103", "102", "101"] })
  );
}

#[tokio::test]
async fn purge_deletes_old_messages_individually() {
  let discord = MockDiscord::start().await;
  let now = Utc::now();
  mock_messages(
    &discord,
    5,
    4,
    vec![
      message_json(104, 5, now),
      message_json(103, 5, now - TimeDelta::minutes(1)),
      message_json(102, 5, now - TimeDelta::days(15)),
      message_json(101, 5, now - TimeDelta::days(30)),
    ],
  )
  .await;
  mock_bulk_delete(&discord, 5).await;
  mock_delete_any(&discord, 204).await;

  purge_channel(&discord.http, ChannelId::new(5), 3, Timestamp::now())
    .await
    .unwrap();

  let requests = discord.requests().await;
  let calls = discord.calls().await;
  assert!(calls.contains(&format!("DELETE {API}/channels/5/messages/102")));
  assert!(calls.contains(&format!("DELETE {API}/channels/5/messages/101")));
  assert_eq!(bulk_body(&requests), json!({ "messages": ["104", "103"] }));
}

#[tokio::test]
async fn purge_single_recent_message_uses_single_delete() {
  // bulk-delete needs at least 2 ids; serenity falls back to a single delete
  let discord = MockDiscord::start().await;
  mock_messages(&discord, 5, 2, vec![message_json(101, 5, Utc::now())]).await;
  mock_delete_any(&discord, 204).await;

  purge_channel(&discord.http, ChannelId::new(5), 1, Timestamp::now())
    .await
    .unwrap();

  assert_eq!(
    discord.calls().await,
    vec![
      format!("GET {API}/channels/5/messages"),
      format!("DELETE {API}/channels/5/messages/101"),
    ]
  );
}

#[tokio::test]
async fn purge_only_old_messages_skips_bulk_delete() {
  let discord = MockDiscord::start().await;
  let old = Utc::now() - TimeDelta::days(20);
  mock_messages(
    &discord,
    5,
    3,
    vec![message_json(102, 5, old), message_json(101, 5, old)],
  )
  .await;
  mock_delete_any(&discord, 204).await;

  assert_eq!(
    purge_channel(&discord.http, ChannelId::new(5), 2, Timestamp::now())
      .await
      .unwrap(),
    2
  );
  assert!(
    !discord
      .calls()
      .await
      .iter()
      .any(|c| c.ends_with("bulk-delete"))
  );
}

#[tokio::test]
async fn purge_empty_channel_is_noop() {
  let discord = MockDiscord::start().await;
  mock_messages(&discord, 5, 6, vec![]).await;

  assert_eq!(
    purge_channel(&discord.http, ChannelId::new(5), 5, Timestamp::now())
      .await
      .unwrap(),
    0
  );
  assert_eq!(
    discord.calls().await,
    vec![format!("GET {API}/channels/5/messages")]
  );
}

#[tokio::test]
async fn purge_max_count_requests_discord_limit() {
  let discord = MockDiscord::start().await;
  mock_messages(&discord, 5, MAX_PURGE + 1, vec![]).await;

  purge_channel(
    &discord.http,
    ChannelId::new(5),
    MAX_PURGE,
    Timestamp::now(),
  )
  .await
  .unwrap();
  assert_eq!(discord.requests().await.len(), 1);
}

#[tokio::test]
async fn purge_fetch_error_is_returned() {
  let discord = MockDiscord::start().await;
  Mock::given(method("GET"))
    .respond_with(ResponseTemplate::new(403).set_body_json(json!({
      "code": 50013,
      "message": "Missing Permissions"
    })))
    .mount(&discord.server)
    .await;

  assert!(
    purge_channel(&discord.http, ChannelId::new(5), 5, Timestamp::now())
      .await
      .is_err()
  );
  assert_eq!(discord.requests().await.len(), 1);
}

#[tokio::test]
async fn purge_bulk_delete_error_is_returned() {
  let discord = MockDiscord::start().await;
  let now = Utc::now();
  mock_messages(
    &discord,
    5,
    2,
    vec![message_json(102, 5, now), message_json(101, 5, now)],
  )
  .await;
  Mock::given(method("POST"))
    .respond_with(ResponseTemplate::new(500))
    .mount(&discord.server)
    .await;

  assert!(
    purge_channel(&discord.http, ChannelId::new(5), 1, Timestamp::now())
      .await
      .is_err()
  );
}

#[tokio::test]
async fn untracked_message_id_is_not_deleted_by_cleanup() {
  // message events for channels without auto-clean never reach Discord deletes
  let discord = MockDiscord::start().await;
  let db = sqlite_db().await;
  db.tables()
    .clean_channel()
    .add_message(ChannelId::new(9), MessageId::new(90))
    .await
    .unwrap();

  assert_eq!(run_cleanup(&discord.http, &db).await.unwrap(), 0);
  assert!(discord.calls().await.is_empty());
}
