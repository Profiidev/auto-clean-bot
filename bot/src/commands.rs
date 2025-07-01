use anyhow::Result;
use chrono::Days;
use poise::command;
use serenity::all::{Channel, GetMessages, GuildChannel, MessageId, Timestamp};

use crate::{Context, db::DBTrait};

#[command(slash_command, prefix_command)]
pub async fn auto_clean(
  ctx: Context<'_>,
  #[description = "The channel to clean up"] channel: Channel,
  #[description = "The delay in minutes before cleaning up messages"] delay: u64,
) -> Result<()> {
  if !(1..60 * 24).contains(&delay) {
    return Err(anyhow::anyhow!(
      "Delay must be between 1 and 1440 minutes (24 hours)."
    ));
  }

  let conn = &ctx.data().conn.tables();

  if !conn.clean_channel().channel_exists(channel.id()).await {
    conn
      .clean_channel()
      .insert_channel(channel.id(), delay)
      .await?;

    ctx
      .say(format!(
        "Now cleaning messages in channel: {channel}, after a delay of {delay} minute."
      ))
      .await?;
  } else {
    conn
      .clean_channel()
      .update_delay(channel.id(), delay)
      .await?;

    ctx
      .say(format!(
        "Updated delay for channel: {channel}, to {delay} minute."
      ))
      .await?;
  }

  ctx.data().trigger_delete_notify.notify_waiters();

  Ok(())
}

#[command(slash_command, prefix_command)]
pub async fn stop_auto_clean(
  ctx: Context<'_>,
  #[description = "The channel to stop cleaning up"] channel: Channel,
) -> Result<()> {
  let conn = &ctx.data().conn.tables();

  if conn.clean_channel().channel_exists(channel.id()).await {
    conn.clean_channel().delete_channel(channel.id()).await?;

    ctx
      .say(format!("Stopped cleaning messages in channel: {channel}"))
      .await?;
  } else {
    ctx
      .say(format!("No auto-clean setup for channel: {channel}"))
      .await?;
  }

  Ok(())
}

#[command(slash_command, prefix_command)]
pub async fn purge(
  ctx: Context<'_>,
  #[description = "The channel to purge"] channel: GuildChannel,
  #[description = "The number of messages to purge"] count: u8,
) -> Result<()> {
  ctx
    .say(format!(
      "Purging {count} messages in channel: {channel}"
    ))
    .await?;

  let messages = channel
    .messages(ctx, GetMessages::new().limit(count + 1))
    .await?;

  let mut ids: Vec<MessageId> = Vec::new();
  for message in messages {
    if message.timestamp.timestamp()
      > Timestamp::now()
        .checked_sub_days(Days::new(14))
        .unwrap()
        .timestamp()
    {
      ids.push(message.id);
    } else {
      message.delete(ctx).await?;
    }
  }

  channel.delete_messages(ctx, ids).await?;

  Ok(())
}
