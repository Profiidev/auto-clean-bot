use sea_orm_migration::{prelude::*, schema::*};

use crate::m20250612_100503_create_clean_channel::CleanChannel;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .create_table(
        Table::create()
          .table(CleanMessage::Table)
          .if_not_exists()
          .col(pk_uuid(CleanMessage::Id))
          .col(uuid(CleanMessage::Channel))
          .col(big_unsigned(CleanMessage::Message))
          .col(date_time(CleanMessage::CreatedAt))
          .foreign_key(
            ForeignKey::create()
              .from(CleanMessage::Table, CleanMessage::Channel)
              .to(CleanChannel::Table, CleanChannel::Id)
              .on_delete(ForeignKeyAction::Cascade)
              .on_update(ForeignKeyAction::Cascade),
          )
          .to_owned(),
      )
      .await
  }

  async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .drop_table(Table::drop().table(CleanMessage::Table).to_owned())
      .await
  }
}

#[derive(DeriveIden)]
enum CleanMessage {
  Table,
  Id,
  Channel,
  Message,
  CreatedAt,
}
