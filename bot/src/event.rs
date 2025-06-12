use std::{sync::atomic::Ordering, time::Duration};

use anyhow::Error;
use poise::FrameworkContext;
use serenity::all::{Context, FullEvent};
use tokio::{select, spawn, time::sleep};
use tracing::info;

use crate::{Data, db::DBTrait};

pub async fn event_handler(
  ctx: &Context,
  event: &FullEvent,
  _framework: FrameworkContext<'_, Data, Error>,
  data: &Data,
) -> Result<(), Error> {
  match event {
    FullEvent::Ready { data_about_bot, .. } => {
      info!("Logged in as {}", data_about_bot.user.name);

      if !data.thread_started.load(Ordering::Relaxed) {
        data.thread_started.store(true, Ordering::Relaxed);
        info!("Starting background tasks...");

        let ctx = ctx.clone();
        let conn = data.conn.clone();
        let notify = data.trigger_delete_notify.clone();

        spawn(async move {
          loop {
            let Ok(next_wait) = conn.tables().clean_channel().next_wait_time().await else {
              info!("Failed to get next wait time, retrying in 5 seconds...");
              sleep(Duration::from_secs(5)).await;
              continue;
            };

            info!("Next wait time: {:?}", next_wait);

            select! {
              _ = sleep(next_wait) => {
                info!("Running auto-clean task");
                let to_delete = match conn.tables().clean_channel().get_to_delete_messages().await {
                  Ok(messages) => messages,
                  Err(e) => {
                    info!("Error fetching messages to delete: {}", e);
                    continue;
                  }
                };

                for (message_id, channel_id) in &to_delete {
                  info!("Deleting message {} from channel {}", message_id, channel_id);
                  if let Err(e) = ctx.http.delete_message(*channel_id, *message_id, None).await {
                    info!("Failed to delete message {}: {}", message_id, e);
                  }
                }

                if let Err(e) = conn.tables().clean_channel().delete_messages(to_delete).await {
                  info!("Failed to delete messages from database: {}", e);
                }

                sleep(Duration::from_secs(5)).await;
              },
              _ = notify.notified() => {}
            }
          }
        });
      }
    }
    FullEvent::Message { new_message } => {
      data
        .conn
        .tables()
        .clean_channel()
        .add_message(new_message.channel_id, new_message.id)
        .await?;

      data.trigger_delete_notify.notify_waiters();
    }
    _ => {}
  }
  Ok(())
}
