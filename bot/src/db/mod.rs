use sea_orm::DatabaseConnection;

use crate::db::clean_channel::CleanChannelTable;

mod clean_channel;

pub trait DBTrait {
  fn tables(&self) -> Tables<'_>;
}

impl DBTrait for DatabaseConnection {
  fn tables(&self) -> Tables<'_> {
    Tables::new(self)
  }
}

pub struct Tables<'db> {
  db: &'db DatabaseConnection,
}

impl<'db> Tables<'db> {
  pub fn new(db: &'db DatabaseConnection) -> Self {
    Self { db }
  }

  pub fn clean_channel(&self) -> CleanChannelTable<'db> {
    CleanChannelTable::new(self.db)
  }
}
