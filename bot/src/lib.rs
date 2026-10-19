use std::sync::{Arc, atomic::AtomicBool};

use anyhow::Error;
use sea_orm::DatabaseConnection;
use tokio::sync::Notify;

pub mod cleanup;
pub mod commands;
pub mod db;
pub mod event;

pub struct Data {
  pub conn: DatabaseConnection,
  pub thread_started: AtomicBool,
  pub trigger_delete_notify: Arc<Notify>,
}

impl Data {
  pub fn new(conn: DatabaseConnection) -> Self {
    Self {
      conn,
      thread_started: AtomicBool::new(false),
      trigger_delete_notify: Arc::new(Notify::new()),
    }
  }
}

pub type Context<'a> = poise::Context<'a, Data, Error>;
