//! Shared harness for the integration tests.
//!
//! - [`sqlite_db`] is the default database: in-memory, migrations applied.
//! - [`postgres_db`] boots a throwaway postgres container (testcontainers, needs Docker)
//!   for postgres-specific behavior.
//! - [`MockDiscord`] runs a local HTTP server that stands in for the Discord REST API.
//!   serenity's `Http` is pointed at it through `HttpBuilder::proxy`, so every API call
//!   the bot makes can be stubbed and asserted on.
#![allow(dead_code)]

use chrono::{DateTime, Utc};
use migration::MigratorTrait;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use serde_json::{Value, json};
use std::sync::Arc;

use serenity::all::{Http, HttpBuilder};
use testcontainers_modules::{
  postgres::Postgres,
  testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner},
};
use wiremock::{MockServer, Request};

pub const API: &str = "/api/v10";

async fn connect(url: &str, max_connections: u32) -> DatabaseConnection {
  let mut opts = ConnectOptions::new(url);
  opts.max_connections(max_connections).sqlx_logging(false);
  let db = Database::connect(opts).await.expect("connect database");
  migration::Migrator::up(&db, None)
    .await
    .expect("run migrations");
  db
}

pub async fn sqlite_db() -> DatabaseConnection {
  // one connection: every new in-memory connection would be a separate database
  connect("sqlite::memory:", 1).await
}

/// Postgres with the same major version as the docker-compose setup.
/// The container lives as long as the returned handle.
pub async fn postgres_db() -> (DatabaseConnection, ContainerAsync<Postgres>) {
  let container = Postgres::default()
    .with_tag("18-alpine")
    .start()
    .await
    .expect("start postgres container");
  let url = format!(
    "postgres://postgres:postgres@{}:{}/postgres",
    container.get_host().await.unwrap(),
    container.get_host_port_ipv4(5432).await.unwrap()
  );
  (connect(&url, 5).await, container)
}

pub struct MockDiscord {
  pub server: MockServer,
  pub http: Arc<Http>,
}

impl MockDiscord {
  pub async fn start() -> Self {
    let server = MockServer::start().await;
    let http = Arc::new(
      HttpBuilder::new("Bot test-token")
        .proxy(server.uri())
        .ratelimiter_disabled(true)
        .build(),
    );
    Self { server, http }
  }

  /// All requests the bot sent, as `"METHOD /path"`.
  pub async fn calls(&self) -> Vec<String> {
    self
      .server
      .received_requests()
      .await
      .unwrap_or_default()
      .iter()
      .map(|r: &Request| format!("{} {}", r.method, r.url.path()))
      .collect()
  }

  pub async fn requests(&self) -> Vec<Request> {
    self.server.received_requests().await.unwrap_or_default()
  }
}

/// Minimal Discord message object as returned by `GET /channels/{id}/messages`.
pub fn message_json(id: u64, channel: u64, timestamp: DateTime<Utc>) -> Value {
  json!({
    "id": id.to_string(),
    "channel_id": channel.to_string(),
    "author": {
      "id": "1",
      "username": "someone",
      "discriminator": "0",
      "global_name": null,
      "avatar": null
    },
    "content": "hello",
    "timestamp": timestamp.to_rfc3339(),
    "edited_timestamp": null,
    "tts": false,
    "mention_everyone": false,
    "mentions": [],
    "mention_roles": [],
    "attachments": [],
    "embeds": [],
    "pinned": false,
    "type": 0
  })
}
