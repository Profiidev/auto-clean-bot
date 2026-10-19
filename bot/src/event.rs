use std::sync::{Arc, atomic::Ordering};

use anyhow::Error;
use poise::FrameworkContext;
use serenity::all::{ChannelId, FullEvent, Http, MessageId};
use tokio::spawn;
use tracing::info;

use crate::{Data, cleanup::cleanup_loop, db::DBTrait};

pub async fn event_handler(
  framework: FrameworkContext<'_, Data, Error>,
  event: &FullEvent,
) -> Result<(), Error> {
  let data = framework.user_data;
  match event {
    FullEvent::Ready { data_about_bot, .. } => {
      info!("Logged in as {}", data_about_bot.user.name);
      start_cleanup(data, framework.serenity_context.http.clone());
    }
    FullEvent::Message { new_message } => {
      on_message(data, new_message.channel_id, new_message.id).await?;
    }
    _ => {}
  }
  Ok(())
}

/// Spawn the cleanup loop once; later `Ready` events (reconnects) are ignored.
/// Returns whether the loop was started by this call.
pub fn start_cleanup(data: &Data, http: Arc<Http>) -> bool {
  if data.thread_started.swap(true, Ordering::SeqCst) {
    return false;
  }
  info!("Starting background tasks...");
  spawn(cleanup_loop(
    http,
    data.conn.clone(),
    data.trigger_delete_notify.clone(),
  ));
  true
}

/// Track a new message (if its channel is auto-cleaned) and wake the cleanup loop.
pub async fn on_message(data: &Data, channel: ChannelId, message: MessageId) -> Result<(), Error> {
  data
    .conn
    .tables()
    .clean_channel()
    .add_message(channel, message)
    .await?;

  data.trigger_delete_notify.notify_waiters();
  Ok(())
}

#[cfg(test)]
mod test {
  use std::{sync::Arc, time::Duration};

  use entity::prelude::*;
  use sea_orm::EntityTrait;
  use serenity::all::{ChannelId, HttpBuilder, MessageId};
  use tokio::time::timeout;

  use super::{on_message, start_cleanup};
  use crate::{Data, db::DBTrait, db::test::test_db};

  #[tokio::test]
  async fn on_message_tracks_and_notifies() {
    let data = Data::new(test_db().await);
    data
      .conn
      .tables()
      .clean_channel()
      .insert_channel(ChannelId::new(1), 5)
      .await
      .unwrap();

    let notified = data.trigger_delete_notify.notified();
    on_message(&data, ChannelId::new(1), MessageId::new(10))
      .await
      .unwrap();
    timeout(Duration::from_secs(1), notified)
      .await
      .expect("cleanup loop must be woken");

    let rows = CleanMessage::find().all(&data.conn).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].message, 10);
  }

  #[tokio::test]
  async fn on_message_untracked_channel_stores_nothing() {
    let data = Data::new(test_db().await);
    on_message(&data, ChannelId::new(1), MessageId::new(10))
      .await
      .unwrap();
    assert!(
      CleanMessage::find()
        .all(&data.conn)
        .await
        .unwrap()
        .is_empty()
    );
  }

  #[tokio::test]
  async fn start_cleanup_runs_only_once() {
    let data = Data::new(test_db().await);
    let http = Arc::new(HttpBuilder::new("token").build());

    assert!(start_cleanup(&data, http.clone()));
    assert!(!start_cleanup(&data, http));
  }
}
