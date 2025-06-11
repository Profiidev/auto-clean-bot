use anyhow::{Error, Result};
use chrono::Days;
use poise::{
  Framework, FrameworkOptions, PrefixFrameworkOptions, command, samples::register_globally,
};
use serenity::all::{
  Channel, ClientBuilder, GatewayIntents, GetMessages, GuildChannel, MessageId, Timestamp,
};

struct Data;
type Context<'a> = poise::Context<'a, Data, Error>;

#[command(slash_command, prefix_command)]
async fn auto_clean(
  ctx: Context<'_>,
  #[description = "The channel to clean up"] channel: Channel,
  #[description = "The delay in seconds before cleaning up messages"] delay: u64,
) -> Result<()> {
  ctx
    .say(format!(
      "Now cleaning messages in channel: {}, after a delay of {} seconds.",
      channel, delay
    ))
    .await?;
  Ok(())
}

#[command(slash_command, prefix_command)]
async fn purge(
  ctx: Context<'_>,
  #[description = "The channel to purge"] channel: GuildChannel,
  #[description = "The number of messages to purge"] count: u8,
) -> Result<()> {
  ctx
    .say(format!(
      "Purging {} messages in channel: {}",
      count, channel
    ))
    .await?;

  let messages = channel
    .messages(ctx, GetMessages::new().limit(count))
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

#[tokio::main]
async fn main() {
  #[cfg(debug_assertions)]
  dotenv::dotenv().ok();

  let token = std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN");
  let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

  let framework = Framework::builder()
    .options(FrameworkOptions {
      commands: vec![auto_clean(), purge()],
      prefix_options: PrefixFrameworkOptions {
        prefix: Some("-".into()),
        ..Default::default()
      },
      ..Default::default()
    })
    .setup(|ctx, _ready, framework| {
      Box::pin(async move {
        register_globally(ctx, &framework.options().commands).await?;
        Ok(Data)
      })
    })
    .build();

  let mut client = ClientBuilder::new(token, intents)
    .framework(framework)
    .await
    .expect("Failed to create client");

  client.start().await.expect("Failed to start client");
}
