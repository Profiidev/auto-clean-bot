use std::{sync::Arc, time::Duration};

use sea_orm::{DatabaseConnection, DbErr};
use serenity::all::Http;
use tokio::{select, sync::Notify, time::sleep};
use tracing::info;

use crate::db::DBTrait;

/// Delete every message whose delay has expired, on Discord and in the database.
/// Discord errors (e.g. message already gone) are logged and the row is dropped anyway.
pub async fn run_cleanup(http: &Http, conn: &DatabaseConnection) -> Result<usize, DbErr> {
  let table = conn.tables().clean_channel();
  let to_delete = table.get_to_delete_messages().await?;

  for (message_id, channel_id) in &to_delete {
    info!(
      "Deleting message {} from channel {}",
      message_id, channel_id
    );
    if let Err(e) = http.delete_message(*channel_id, *message_id, None).await {
      info!("Failed to delete message {}: {}", message_id, e);
    }
  }

  let count = to_delete.len();
  table.delete_messages(to_delete).await?;
  Ok(count)
}

/// Sleep until the next message expires (or `notify` fires), then clean up. Never returns.
pub async fn cleanup_loop(http: Arc<Http>, conn: DatabaseConnection, notify: Arc<Notify>) {
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
        if let Err(e) = run_cleanup(&http, &conn).await {
          info!("Auto-clean failed: {}", e);
        }
        sleep(Duration::from_secs(5)).await;
      },
      _ = notify.notified() => {}
    }
  }
}
