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

#[cfg(test)]
pub mod test {
  use migration::MigratorTrait;
  use sea_orm::{ConnectOptions, Database, DatabaseConnection};

  /// Fresh in-memory SQLite database with all migrations applied.
  pub async fn test_db() -> DatabaseConnection {
    // one connection: every new in-memory connection would be a separate database
    let mut opts = ConnectOptions::new("sqlite::memory:");
    opts.max_connections(1).sqlx_logging(false);
    let db = Database::connect(opts).await.unwrap();
    migration::Migrator::up(&db, None).await.unwrap();
    db
  }
}
