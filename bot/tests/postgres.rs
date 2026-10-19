//! Database behavior on a real postgres (testcontainers) instead of sqlite.
//!
//! Covers what differs between the backends: migrations and the FK cascade on
//! postgres, `u64` snowflakes bound as `bigint`, `timestamp` round trips feeding the
//! wait-time calculation, and the full cleanup flow against a postgres-backed store.
mod common;

use auto_clean_bot::{
  Data,
  cleanup::run_cleanup,
  commands::{AutoClean, remove_auto_clean, set_auto_clean},
  db::DBTrait,
  event::on_message,
};
use chrono::{TimeDelta, Utc};
use common::{API, MockDiscord, postgres_db};
use entity::{clean_message, prelude::*};
use migration::MigratorTrait;
use sea_orm::{ActiveValue::Set, DbErr, prelude::*};
use serenity::all::{ChannelId, MessageId};
use wiremock::{Mock, ResponseTemplate, matchers::method};

#[tokio::test]
async fn migrations_apply_and_roll_back() {
  let (db, _pg) = postgres_db().await;
  let schema = migration::SchemaManager::new(&db);
  assert!(schema.has_table("clean_channel").await.unwrap());
  assert!(schema.has_table("clean_message").await.unwrap());

  // re-running is a no-op
  migration::Migrator::up(&db, None).await.unwrap();

  migration::Migrator::down(&db, None).await.unwrap();
  assert!(!schema.has_table("clean_channel").await.unwrap());
  assert!(!schema.has_table("clean_message").await.unwrap());

  migration::Migrator::up(&db, None).await.unwrap();
  assert!(schema.has_table("clean_channel").await.unwrap());
}

#[tokio::test]
async fn channel_crud_with_large_snowflakes() {
  let (db, _pg) = postgres_db().await;
  let table = db.tables().clean_channel();
  let channel = ChannelId::new(1_382_315_170_595_868_702);

  assert_eq!(
    set_auto_clean(&db, channel, 5).await.unwrap(),
    AutoClean::Added
  );
  let row = table.get_channel(channel).await.unwrap();
  assert_eq!((row.channel as u64, row.delay), (channel.get(), 5));

  assert_eq!(
    set_auto_clean(&db, channel, 1440).await.unwrap(),
    AutoClean::Updated
  );
  assert_eq!(table.get_channel(channel).await.unwrap().delay, 1440);

  assert!(matches!(
    table.update_delay(ChannelId::new(1), 1).await,
    Err(DbErr::RecordNotFound(_))
  ));

  assert!(remove_auto_clean(&db, channel).await.unwrap());
  assert!(!remove_auto_clean(&db, channel).await.unwrap());
}

#[tokio::test]
async fn deleting_channel_cascades_messages() {
  let (db, _pg) = postgres_db().await;
  let table = db.tables().clean_channel();
  table.insert_channel(ChannelId::new(1), 5).await.unwrap();
  table.insert_channel(ChannelId::new(2), 5).await.unwrap();
  table
    .add_message(ChannelId::new(1), MessageId::new(10))
    .await
    .unwrap();
  table
    .add_message(ChannelId::new(2), MessageId::new(20))
    .await
    .unwrap();

  table.delete_channel(ChannelId::new(1)).await.unwrap();

  let rows = CleanMessage::find().all(&db).await.unwrap();
  assert_eq!(rows.len(), 1);
  assert_eq!(rows[0].message, 20);
}

#[tokio::test]
async fn message_for_unknown_channel_violates_foreign_key() {
  let (db, _pg) = postgres_db().await;
  let res = clean_message::ActiveModel {
    id: Set(Uuid::new_v4()),
    channel: Set(Uuid::new_v4()),
    message: Set(1),
    created_at: Set(Utc::now().naive_utc()),
  }
  .insert(&db)
  .await;
  assert!(res.is_err());
}

#[tokio::test]
async fn wait_times_use_stored_timestamps() {
  let (db, _pg) = postgres_db().await;
  let table = db.tables().clean_channel();
  table.insert_channel(ChannelId::new(1), 10).await.unwrap();
  let data = Data::new(db.clone());
  on_message(&data, ChannelId::new(1), MessageId::new(10))
    .await
    .unwrap();

  let wait = table.next_wait_time().await.unwrap().as_secs();
  assert!((9 * 60..=10 * 60).contains(&wait), "wait was {wait}");
  assert!(table.get_to_delete_messages().await.unwrap().is_empty());

  let row = table.get_channel(ChannelId::new(1)).await.unwrap();
  clean_message::ActiveModel {
    id: Set(Uuid::new_v4()),
    channel: Set(row.id),
    message: Set(11),
    created_at: Set(Utc::now().naive_utc() - TimeDelta::minutes(11)),
  }
  .insert(&db)
  .await
  .unwrap();

  assert_eq!(table.next_wait_time().await.unwrap().as_secs(), 0);
  assert_eq!(
    table.get_to_delete_messages().await.unwrap(),
    vec![(MessageId::new(11), ChannelId::new(1))]
  );
}

#[tokio::test]
async fn cleanup_flow_on_postgres() {
  let (db, _pg) = postgres_db().await;
  let discord = MockDiscord::start().await;
  Mock::given(method("DELETE"))
    .respond_with(ResponseTemplate::new(204))
    .mount(&discord.server)
    .await;

  let table = db.tables().clean_channel();
  table.insert_channel(ChannelId::new(1), 1).await.unwrap();
  let row = table.get_channel(ChannelId::new(1)).await.unwrap();
  clean_message::ActiveModel {
    id: Set(Uuid::new_v4()),
    channel: Set(row.id),
    message: Set(10),
    created_at: Set(Utc::now().naive_utc() - TimeDelta::minutes(2)),
  }
  .insert(&db)
  .await
  .unwrap();

  assert_eq!(run_cleanup(&discord.http, &db).await.unwrap(), 1);
  assert_eq!(
    discord.calls().await,
    vec![format!("DELETE {API}/channels/1/messages/10")]
  );
  assert_eq!(CleanMessage::find().count(&db).await.unwrap(), 0);
}
