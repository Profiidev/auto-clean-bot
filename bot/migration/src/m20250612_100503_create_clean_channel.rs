use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
  async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .create_table(
        Table::create()
          .table(CleanChannel::Table)
          .if_not_exists()
          .col(pk_uuid(CleanChannel::Id))
          .col(big_unsigned(CleanChannel::Channel))
          .col(big_unsigned(CleanChannel::Delay))
          .to_owned(),
      )
      .await
  }

  async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    manager
      .drop_table(Table::drop().table(CleanChannel::Table).to_owned())
      .await
  }
}

#[derive(DeriveIden)]
pub enum CleanChannel {
  Table,
  Id,
  Channel,
  Delay,
}
