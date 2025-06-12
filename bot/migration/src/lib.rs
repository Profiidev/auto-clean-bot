pub use sea_orm_migration::prelude::*;

pub struct Migrator;

mod m20250612_100503_create_clean_channel;
mod m20250612_100617_create_clean_message;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
  fn migrations() -> Vec<Box<dyn MigrationTrait>> {
    vec![
      Box::new(m20250612_100503_create_clean_channel::Migration),
      Box::new(m20250612_100617_create_clean_message::Migration),
    ]
  }
}
