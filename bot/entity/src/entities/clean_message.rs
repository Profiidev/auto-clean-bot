//! `SeaORM` Entity, @generated by sea-orm-codegen 1.1.8

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "clean_message")]
pub struct Model {
  #[sea_orm(primary_key, auto_increment = false)]
  pub id: Uuid,
  pub channel: Uuid,
  pub message: i64,
  pub created_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
  #[sea_orm(
    belongs_to = "super::clean_channel::Entity",
    from = "Column::Channel",
    to = "super::clean_channel::Column::Id",
    on_update = "Cascade",
    on_delete = "Cascade"
  )]
  CleanChannel,
}

impl Related<super::clean_channel::Entity> for Entity {
  fn to() -> RelationDef {
    Relation::CleanChannel.def()
  }
}

impl ActiveModelBehavior for ActiveModel {}
