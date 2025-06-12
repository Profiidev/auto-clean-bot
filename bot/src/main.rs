use anyhow::Error;
use migration::MigratorTrait;
use poise::{Framework, FrameworkOptions, PrefixFrameworkOptions, samples::register_globally};
use sea_orm::{Database, DatabaseConnection};
use serenity::all::{ClientBuilder, GatewayIntents};
use tracing_subscriber::{EnvFilter, fmt::layer, layer::SubscriberExt, util::SubscriberInitExt};

use crate::commands::{auto_clean, purge, stop_auto_clean};

mod commands;
mod db;

struct Data {
  conn: DatabaseConnection,
}
type Context<'a> = poise::Context<'a, Data, Error>;

#[tokio::main]
async fn main() {
  #[cfg(debug_assertions)]
  dotenv::dotenv().ok();
  tracing_subscriber::registry()
    .with(layer().compact())
    .with(EnvFilter::from_default_env())
    .init();
  tracing::info!("Starting bot...");

  let db_url = std::env::var("DB_URL").expect("missing DB_URL");
  let conn = Database::connect(&db_url)
    .await
    .expect("Failed to connect to database");
  tracing::info!("Connected to database at {}", db_url);

  migration::Migrator::up(&conn, None)
    .await
    .expect("Failed to run migrations");
  tracing::info!("Database migrations completed");

  let token = std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN");
  let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

  let framework = Framework::builder()
    .options(FrameworkOptions {
      commands: vec![auto_clean(), purge(), stop_auto_clean()],
      prefix_options: PrefixFrameworkOptions {
        prefix: Some("-".into()),
        ..Default::default()
      },
      ..Default::default()
    })
    .setup(|ctx, _ready, framework| {
      Box::pin(async move {
        register_globally(ctx, &framework.options().commands).await?;
        Ok(Data { conn })
      })
    })
    .build();

  let mut client = ClientBuilder::new(token, intents)
    .framework(framework)
    .await
    .expect("Failed to create client");

  tracing::info!("Client created, starting...");
  client.start().await.expect("Failed to start client");
}
