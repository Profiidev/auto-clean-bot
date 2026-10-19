use anyhow::{Result, bail};
use chrono::TimeDelta;
use poise::command;
use sea_orm::DatabaseConnection;
use serenity::all::{Channel, ChannelId, GetMessages, GuildChannel, Http, MessageId, Timestamp};

use crate::{Context, db::DBTrait};

pub const MAX_DELAY: u64 = 60 * 24;
/// Discord fetches at most 100 messages; purge also removes its own reply.
pub const MAX_PURGE: u8 = 99;
/// Discord refuses to bulk-delete messages older than this.
const BULK_DELETE_MAX_AGE: TimeDelta = TimeDelta::days(14);

#[derive(Debug, PartialEq, Eq)]
pub enum AutoClean {
  Added,
  Updated,
}

#[command(slash_command, prefix_command)]
pub async fn auto_clean(
  ctx: Context<'_>,
  #[description = "The channel to clean up"] channel: Channel,
  #[description = "The delay in minutes before cleaning up messages"] delay: u64,
) -> Result<()> {
  let reply = match set_auto_clean(&ctx.data().conn, channel.id(), delay).await? {
    AutoClean::Added => {
      format!("Now cleaning messages in channel: {channel}, after a delay of {delay} minute.")
    }
    AutoClean::Updated => format!("Updated delay for channel: {channel}, to {delay} minute."),
  };
  ctx.say(reply).await?;

  ctx.data().trigger_delete_notify.notify_waiters();

  Ok(())
}

#[command(slash_command, prefix_command)]
pub async fn stop_auto_clean(
  ctx: Context<'_>,
  #[description = "The channel to stop cleaning up"] channel: Channel,
) -> Result<()> {
  let reply = if remove_auto_clean(&ctx.data().conn, channel.id()).await? {
    format!("Stopped cleaning messages in channel: {channel}")
  } else {
    format!("No auto-clean setup for channel: {channel}")
  };
  ctx.say(reply).await?;

  Ok(())
}

#[command(slash_command, prefix_command)]
pub async fn purge(
  ctx: Context<'_>,
  #[description = "The channel to purge"] channel: GuildChannel,
  #[description = "The number of messages to purge"] count: u8,
) -> Result<()> {
  validate_purge_count(count)?;

  ctx
    .say(format!("Purging {count} messages in channel: {channel}"))
    .await?;

  purge_channel(ctx.http(), channel.id, count, Timestamp::now()).await?;

  Ok(())
}

pub fn validate_delay(delay: u64) -> Result<()> {
  if !(1..=MAX_DELAY).contains(&delay) {
    bail!("Delay must be between 1 and {MAX_DELAY} minutes (24 hours).");
  }
  Ok(())
}

pub fn validate_purge_count(count: u8) -> Result<()> {
  if !(1..=MAX_PURGE).contains(&count) {
    bail!("Count must be between 1 and {MAX_PURGE}.");
  }
  Ok(())
}

/// Enable auto-clean for `channel`, or change its delay when already enabled.
pub async fn set_auto_clean(
  conn: &DatabaseConnection,
  channel: ChannelId,
  delay: u64,
) -> Result<AutoClean> {
  validate_delay(delay)?;
  let table = conn.tables().clean_channel();

  if table.channel_exists(channel).await {
    table.update_delay(channel, delay).await?;
    Ok(AutoClean::Updated)
  } else {
    table.insert_channel(channel, delay).await?;
    Ok(AutoClean::Added)
  }
}

/// Disable auto-clean for `channel`. Returns whether it was enabled.
pub async fn remove_auto_clean(conn: &DatabaseConnection, channel: ChannelId) -> Result<bool> {
  let table = conn.tables().clean_channel();
  if !table.channel_exists(channel).await {
    return Ok(false);
  }
  table.delete_channel(channel).await?;
  Ok(true)
}

/// Delete the last `count` messages plus the bot's own reply. Messages younger than
/// 14 days are bulk-deleted, older ones one by one. Returns the number deleted.
pub async fn purge_channel(
  http: &Http,
  channel: ChannelId,
  count: u8,
  now: Timestamp,
) -> Result<usize> {
  validate_purge_count(count)?;

  let messages = channel
    .messages(http, GetMessages::new().limit(count + 1))
    .await?;

  let cutoff = (*now - BULK_DELETE_MAX_AGE).timestamp();
  let (recent, old): (Vec<_>, Vec<_>) = messages
    .iter()
    .partition(|m| m.timestamp.unix_timestamp() > cutoff);

  for message in &old {
    channel.delete_message(http, message.id).await?;
  }

  let recent: Vec<MessageId> = recent.iter().map(|m| m.id).collect();
  if !recent.is_empty() {
    channel.delete_messages(http, &recent).await?;
  }

  Ok(messages.len())
}

#[cfg(test)]
mod test {
  use super::*;
  use crate::db::test::test_db;

  #[test]
  fn validate_delay_bounds() {
    assert!(validate_delay(0).is_err());
    assert!(validate_delay(1).is_ok());
    assert!(validate_delay(MAX_DELAY).is_ok());
    assert!(validate_delay(MAX_DELAY + 1).is_err());
    assert!(validate_delay(u64::MAX).is_err());
  }

  #[test]
  fn validate_purge_count_bounds() {
    assert!(validate_purge_count(0).is_err());
    assert!(validate_purge_count(1).is_ok());
    assert!(validate_purge_count(MAX_PURGE).is_ok());
    assert!(validate_purge_count(MAX_PURGE + 1).is_err());
    // count + 1 would overflow u8
    assert!(validate_purge_count(u8::MAX).is_err());
  }

  #[tokio::test]
  async fn set_auto_clean_adds_then_updates() {
    let db = test_db().await;
    let channel = ChannelId::new(42);

    assert_eq!(
      set_auto_clean(&db, channel, 5).await.unwrap(),
      AutoClean::Added
    );
    assert_eq!(
      set_auto_clean(&db, channel, 10).await.unwrap(),
      AutoClean::Updated
    );

    let row = db
      .tables()
      .clean_channel()
      .get_channel(channel)
      .await
      .unwrap();
    assert_eq!(row.delay, 10);
  }

  #[tokio::test]
  async fn set_auto_clean_invalid_delay_writes_nothing() {
    let db = test_db().await;
    let channel = ChannelId::new(42);

    assert!(set_auto_clean(&db, channel, 0).await.is_err());
    assert!(!db.tables().clean_channel().channel_exists(channel).await);

    set_auto_clean(&db, channel, 5).await.unwrap();
    assert!(set_auto_clean(&db, channel, MAX_DELAY + 1).await.is_err());
    let row = db
      .tables()
      .clean_channel()
      .get_channel(channel)
      .await
      .unwrap();
    assert_eq!(row.delay, 5);
  }

  #[tokio::test]
  async fn remove_auto_clean_existing_and_missing() {
    let db = test_db().await;
    let channel = ChannelId::new(42);

    assert!(!remove_auto_clean(&db, channel).await.unwrap());

    set_auto_clean(&db, channel, 5).await.unwrap();
    assert!(remove_auto_clean(&db, channel).await.unwrap());
    assert!(!db.tables().clean_channel().channel_exists(channel).await);
    assert!(!remove_auto_clean(&db, channel).await.unwrap());
  }

  #[tokio::test]
  async fn purge_channel_rejects_invalid_count_before_any_request() {
    // unroutable proxy: any HTTP request would error differently
    let http = serenity::all::HttpBuilder::new("token")
      .proxy("http://127.0.0.1:1")
      .ratelimiter_disabled(true)
      .build();
    let err = purge_channel(&http, ChannelId::new(1), 0, Timestamp::now())
      .await
      .unwrap_err();
    assert!(err.to_string().contains("Count must be between"));
  }
}
