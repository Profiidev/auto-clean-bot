use std::time::Duration;

use chrono::{TimeDelta, Utc};
use entity::{clean_channel, clean_message, prelude::*};
use sea_orm::{ActiveValue::Set, FromQueryResult, JoinType, QuerySelect, prelude::*};
use serenity::all::{ChannelId, MessageId};

#[derive(FromQueryResult)]
struct NextFindResult {
  created_at: chrono::NaiveDateTime,
  delay: i64,
  message: i64,
  channel: i64,
}

pub struct CleanChannelTable<'db> {
  db: &'db DatabaseConnection,
}

impl<'db> CleanChannelTable<'db> {
  pub fn new(db: &'db DatabaseConnection) -> Self {
    Self { db }
  }

  pub async fn get_channel(&self, id: ChannelId) -> Result<clean_channel::Model, DbErr> {
    let res = CleanChannel::find()
      .filter(clean_channel::Column::Channel.eq(id.get()))
      .one(self.db)
      .await?;

    res.ok_or(DbErr::RecordNotFound("Not Found".into()))
  }

  pub async fn channel_exists(&self, id: ChannelId) -> bool {
    self.get_channel(id).await.is_ok()
  }

  pub async fn insert_channel(&self, id: ChannelId, delay: u64) -> Result<(), DbErr> {
    let channel = clean_channel::ActiveModel {
      id: Set(Uuid::new_v4()),
      channel: Set(id.get() as i64),
      delay: Set(delay as i64),
    };

    channel.insert(self.db).await?;
    Ok(())
  }

  pub async fn delete_channel(&self, id: ChannelId) -> Result<(), DbErr> {
    CleanChannel::delete_many()
      .filter(clean_channel::Column::Channel.eq(id.get()))
      .exec(self.db)
      .await?;

    Ok(())
  }

  pub async fn update_delay(&self, id: ChannelId, delay: u64) -> Result<(), DbErr> {
    let mut channel: clean_channel::ActiveModel = self.get_channel(id).await?.into();

    channel.delay = Set(delay as i64);

    channel.update(self.db).await?;
    Ok(())
  }

  pub async fn add_message(&self, id: ChannelId, message_id: MessageId) -> Result<(), DbErr> {
    let Ok(channel) = self.get_channel(id).await else {
      return Ok(());
    };

    let message = clean_message::ActiveModel {
      id: Set(Uuid::new_v4()),
      channel: Set(channel.id),
      message: Set(message_id.get() as i64),
      created_at: Set(Utc::now().naive_utc()),
    };

    message.insert(self.db).await?;

    Ok(())
  }

  async fn wait_times(&self) -> Result<Vec<(TimeDelta, i64, i64)>, DbErr> {
    Ok(
      CleanMessage::find()
        .join(
          JoinType::InnerJoin,
          clean_message::Relation::CleanChannel.def(),
        )
        .filter(clean_channel::Column::Delay.gt(0))
        .select_only()
        .column(clean_message::Column::CreatedAt)
        .column(clean_message::Column::Message)
        .column(clean_channel::Column::Delay)
        .column(clean_channel::Column::Channel)
        .into_model::<NextFindResult>()
        .all(self.db)
        .await?
        .into_iter()
        .map(
          |NextFindResult {
             created_at,
             delay,
             message,
             channel,
           }| {
            (
              created_at + chrono::Duration::minutes(delay) - Utc::now().naive_utc(),
              message,
              channel,
            )
          },
        )
        .collect(),
    )
  }

  pub async fn next_wait_time(&self) -> Result<Duration, DbErr> {
    let delta = self
      .wait_times()
      .await?
      .into_iter()
      .map(|(delta, _, _)| delta)
      .min()
      .unwrap_or(TimeDelta::MAX);

    if delta <= TimeDelta::zero() {
      Ok(Duration::from_secs(0))
    } else {
      Ok(Duration::from_secs(delta.num_seconds() as u64))
    }
  }

  pub async fn get_to_delete_messages(&self) -> Result<Vec<(MessageId, ChannelId)>, DbErr> {
    Ok(
      self
        .wait_times()
        .await?
        .into_iter()
        .filter(|(delta, _, _)| *delta <= TimeDelta::zero())
        .map(|(_, message_id, channel_id)| {
          (
            MessageId::new(message_id as u64),
            ChannelId::new(channel_id as u64),
          )
        })
        .collect::<Vec<_>>(),
    )
  }

  pub async fn delete_messages(&self, messages: Vec<(MessageId, ChannelId)>) -> Result<(), DbErr> {
    let message_ids = messages
      .iter()
      .map(|(message_id, _)| message_id.get() as i64)
      .collect::<Vec<_>>();

    CleanMessage::delete_many()
      .filter(clean_message::Column::Message.is_in(message_ids))
      .exec(self.db)
      .await?;
    Ok(())
  }
}

#[cfg(test)]
mod test {
  use chrono::{NaiveDateTime, TimeDelta, Utc};
  use entity::{clean_message, prelude::*};
  use sea_orm::{ActiveValue::Set, DbErr, prelude::*};
  use serenity::all::{ChannelId, MessageId};

  use crate::db::{DBTrait, test::test_db};

  async fn insert_message_at(
    db: &DatabaseConnection,
    channel: ChannelId,
    message: u64,
    created_at: NaiveDateTime,
  ) {
    let row = db
      .tables()
      .clean_channel()
      .get_channel(channel)
      .await
      .unwrap();
    clean_message::ActiveModel {
      id: Set(Uuid::new_v4()),
      channel: Set(row.id),
      message: Set(message as i64),
      created_at: Set(created_at),
    }
    .insert(db)
    .await
    .unwrap();
  }

  fn minutes_ago(minutes: i64) -> NaiveDateTime {
    Utc::now().naive_utc() - TimeDelta::minutes(minutes)
  }

  // ---- channels ----

  #[tokio::test]
  async fn get_channel_returns_row_and_not_found() {
    let db = test_db().await;
    let table = db.tables().clean_channel();
    table.insert_channel(ChannelId::new(7), 3).await.unwrap();

    let row = table.get_channel(ChannelId::new(7)).await.unwrap();
    assert_eq!((row.channel, row.delay), (7, 3));
    assert!(matches!(
      table.get_channel(ChannelId::new(8)).await,
      Err(DbErr::RecordNotFound(_))
    ));
  }

  #[tokio::test]
  async fn channel_exists_reflects_inserts_and_deletes() {
    let db = test_db().await;
    let table = db.tables().clean_channel();
    let channel = ChannelId::new(7);

    assert!(!table.channel_exists(channel).await);
    table.insert_channel(channel, 3).await.unwrap();
    assert!(table.channel_exists(channel).await);
    table.delete_channel(channel).await.unwrap();
    assert!(!table.channel_exists(channel).await);
  }

  #[tokio::test]
  async fn insert_channel_keeps_large_snowflakes() {
    let db = test_db().await;
    let table = db.tables().clean_channel();
    let channel = ChannelId::new(1_382_315_170_595_868_702);

    table.insert_channel(channel, 1).await.unwrap();
    assert_eq!(
      table.get_channel(channel).await.unwrap().channel as u64,
      channel.get()
    );
  }

  #[tokio::test]
  async fn delete_channel_missing_is_noop() {
    let db = test_db().await;
    let table = db.tables().clean_channel();
    table.insert_channel(ChannelId::new(1), 1).await.unwrap();

    table.delete_channel(ChannelId::new(2)).await.unwrap();
    assert!(table.channel_exists(ChannelId::new(1)).await);
  }

  #[tokio::test]
  async fn delete_channel_cascades_messages() {
    let db = test_db().await;
    let table = db.tables().clean_channel();
    let channel = ChannelId::new(1);
    table.insert_channel(channel, 1).await.unwrap();
    table
      .add_message(channel, MessageId::new(10))
      .await
      .unwrap();

    table.delete_channel(channel).await.unwrap();
    assert_eq!(CleanMessage::find().count(&db).await.unwrap(), 0);
  }

  #[tokio::test]
  async fn update_delay_changes_only_target() {
    let db = test_db().await;
    let table = db.tables().clean_channel();
    table.insert_channel(ChannelId::new(1), 1).await.unwrap();
    table.insert_channel(ChannelId::new(2), 2).await.unwrap();

    table.update_delay(ChannelId::new(1), 30).await.unwrap();
    assert_eq!(
      table.get_channel(ChannelId::new(1)).await.unwrap().delay,
      30
    );
    assert_eq!(table.get_channel(ChannelId::new(2)).await.unwrap().delay, 2);
  }

  #[tokio::test]
  async fn update_delay_missing_is_not_found() {
    let db = test_db().await;
    assert!(matches!(
      db.tables()
        .clean_channel()
        .update_delay(ChannelId::new(1), 5)
        .await,
      Err(DbErr::RecordNotFound(_))
    ));
  }

  // ---- messages ----

  #[tokio::test]
  async fn add_message_untracked_channel_is_noop() {
    let db = test_db().await;
    db.tables()
      .clean_channel()
      .add_message(ChannelId::new(1), MessageId::new(10))
      .await
      .unwrap();
    assert_eq!(CleanMessage::find().count(&db).await.unwrap(), 0);
  }

  #[tokio::test]
  async fn add_message_tracked_channel_stores_row() {
    let db = test_db().await;
    let table = db.tables().clean_channel();
    let channel = ChannelId::new(1);
    table.insert_channel(channel, 1).await.unwrap();

    let before = Utc::now().naive_utc();
    table
      .add_message(channel, MessageId::new(10))
      .await
      .unwrap();

    let rows = CleanMessage::find().all(&db).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].message, 10);
    assert_eq!(
      rows[0].channel,
      table.get_channel(channel).await.unwrap().id
    );
    assert!(rows[0].created_at >= before - TimeDelta::seconds(1));
  }

  // ---- wait times ----

  #[tokio::test]
  async fn next_wait_time_without_messages_is_effectively_forever() {
    let db = test_db().await;
    let wait = db.tables().clean_channel().next_wait_time().await.unwrap();
    assert!(wait.as_secs() > 60 * 60 * 24 * 365);
  }

  #[tokio::test]
  async fn next_wait_time_future_message_waits_for_delay() {
    let db = test_db().await;
    let table = db.tables().clean_channel();
    table.insert_channel(ChannelId::new(1), 10).await.unwrap();
    table
      .add_message(ChannelId::new(1), MessageId::new(10))
      .await
      .unwrap();

    let wait = table.next_wait_time().await.unwrap().as_secs();
    assert!((9 * 60..=10 * 60).contains(&wait), "wait was {wait}");
  }

  #[tokio::test]
  async fn next_wait_time_expired_message_is_zero() {
    let db = test_db().await;
    let table = db.tables().clean_channel();
    table.insert_channel(ChannelId::new(1), 1).await.unwrap();
    insert_message_at(&db, ChannelId::new(1), 10, minutes_ago(5)).await;

    assert_eq!(table.next_wait_time().await.unwrap().as_secs(), 0);
  }

  #[tokio::test]
  async fn next_wait_time_picks_soonest_across_channels() {
    let db = test_db().await;
    let table = db.tables().clean_channel();
    table.insert_channel(ChannelId::new(1), 60).await.unwrap();
    table.insert_channel(ChannelId::new(2), 5).await.unwrap();
    table
      .add_message(ChannelId::new(1), MessageId::new(10))
      .await
      .unwrap();
    table
      .add_message(ChannelId::new(2), MessageId::new(20))
      .await
      .unwrap();

    let wait = table.next_wait_time().await.unwrap().as_secs();
    assert!((4 * 60..=5 * 60).contains(&wait), "wait was {wait}");
  }

  #[tokio::test]
  async fn zero_delay_channels_are_ignored() {
    let db = test_db().await;
    let table = db.tables().clean_channel();
    table.insert_channel(ChannelId::new(1), 0).await.unwrap();
    insert_message_at(&db, ChannelId::new(1), 10, minutes_ago(5)).await;

    assert!(table.get_to_delete_messages().await.unwrap().is_empty());
    assert!(table.next_wait_time().await.unwrap().as_secs() > 60 * 60);
  }

  #[tokio::test]
  async fn get_to_delete_messages_returns_only_expired() {
    let db = test_db().await;
    let table = db.tables().clean_channel();
    table.insert_channel(ChannelId::new(1), 2).await.unwrap();
    table.insert_channel(ChannelId::new(2), 2).await.unwrap();
    insert_message_at(&db, ChannelId::new(1), 10, minutes_ago(3)).await;
    insert_message_at(&db, ChannelId::new(2), 20, minutes_ago(3)).await;
    insert_message_at(&db, ChannelId::new(1), 11, minutes_ago(1)).await;

    let mut expired = table.get_to_delete_messages().await.unwrap();
    expired.sort();
    assert_eq!(
      expired,
      vec![
        (MessageId::new(10), ChannelId::new(1)),
        (MessageId::new(20), ChannelId::new(2)),
      ]
    );
  }

  #[tokio::test]
  async fn delete_messages_removes_only_listed() {
    let db = test_db().await;
    let table = db.tables().clean_channel();
    table.insert_channel(ChannelId::new(1), 1).await.unwrap();
    table
      .add_message(ChannelId::new(1), MessageId::new(10))
      .await
      .unwrap();
    table
      .add_message(ChannelId::new(1), MessageId::new(11))
      .await
      .unwrap();

    table
      .delete_messages(vec![(MessageId::new(10), ChannelId::new(1))])
      .await
      .unwrap();

    let rows = CleanMessage::find().all(&db).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].message, 11);
  }

  #[tokio::test]
  async fn delete_messages_empty_is_noop() {
    let db = test_db().await;
    let table = db.tables().clean_channel();
    table.insert_channel(ChannelId::new(1), 1).await.unwrap();
    table
      .add_message(ChannelId::new(1), MessageId::new(10))
      .await
      .unwrap();

    table.delete_messages(vec![]).await.unwrap();
    assert_eq!(CleanMessage::find().count(&db).await.unwrap(), 1);
  }
}
