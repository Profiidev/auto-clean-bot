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
