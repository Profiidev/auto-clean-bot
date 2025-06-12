use entity::{clean_channel, prelude::*};
use sea_orm::{ActiveValue::Set, prelude::*};
use serenity::all::ChannelId;

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
}
