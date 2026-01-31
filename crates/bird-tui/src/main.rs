//! bird-tui - A terminal user interface for viewing synced tweets

pub mod app;
pub mod data;
pub mod events;

use anyhow::Result;
use bird_storage::{
    create_storage, default_db_path, StorageConfig, SurrealDbAuth, SurrealDbConfig,
};
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "bird-tui")]
#[command(author, version, about = "A terminal user interface for viewing synced tweets")]
pub struct Args {
    /// Path to the database file (local SurrealDB only).
    #[arg(long, env = "BIRD_DB_PATH")]
    db_path: Option<PathBuf>,

    /// Storage backend to use (surrealdb, memory). Defaults to surrealdb.
    #[arg(long, env = "BIRD_STORAGE")]
    storage: Option<StorageBackend>,

    /// SurrealDB connection endpoint (e.g., ws://localhost:8000).
    #[arg(long, env = "BIRD_DB_URL")]
    db_url: Option<String>,

    /// SurrealDB namespace (default: bird).
    #[arg(long, env = "BIRD_DB_NAMESPACE")]
    db_namespace: Option<String>,

    /// SurrealDB database name (default: main).
    #[arg(long, env = "BIRD_DB_NAME")]
    db_name: Option<String>,

    /// SurrealDB authentication mode (root, namespace, database).
    #[arg(long, env = "BIRD_DB_AUTH")]
    db_auth: Option<DbAuthMode>,

    /// SurrealDB username.
    #[arg(long, env = "BIRD_DB_USER")]
    db_user: Option<String>,

    /// SurrealDB password.
    #[arg(long, env = "BIRD_DB_PASS")]
    db_pass: Option<String>,

    /// Path to config file (defaults to ~/.bird/config.toml).
    #[arg(long, env = "BIRD_CONFIG")]
    config: Option<PathBuf>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum StorageBackend {
    #[value(name = "surrealdb")]
    SurrealDb,
    #[value(name = "memory")]
    Memory,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum DbAuthMode {
    #[value(name = "root")]
    Root,
    #[value(name = "namespace")]
    Namespace,
    #[value(name = "database")]
    Database,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Load config if available (config path for future use)
    let _config_path = args
        .config
        .clone()
        .or_else(|| {
            dirs::home_dir().map(|mut p| {
                p.push(".bird/config.toml");
                p
            })
        });

    // Create storage
    let backend = args.storage.unwrap_or(StorageBackend::SurrealDb);

    let storage_config = match backend {
        StorageBackend::Memory => StorageConfig::Memory,
        StorageBackend::SurrealDb => {
            let db_path = args.db_path.clone().unwrap_or_else(default_db_path);
            let endpoint = args
                .db_url
                .clone()
                .unwrap_or_else(|| format!("rocksdb://{}", db_path.display()));
            let namespace = args
                .db_namespace
                .unwrap_or_else(|| "bird".to_string());
            let database = args.db_name.unwrap_or_else(|| "main".to_string());

            let auth = if let (Some(username), Some(password)) = (args.db_user.clone(), args.db_pass.clone()) {
                let auth_mode = args.db_auth.unwrap_or(DbAuthMode::Root);
                Some(match auth_mode {
                    DbAuthMode::Root => SurrealDbAuth::Root { username, password },
                    DbAuthMode::Namespace => SurrealDbAuth::Namespace { username, password },
                    DbAuthMode::Database => SurrealDbAuth::Database { username, password },
                })
            } else {
                None
            };

            StorageConfig::SurrealDb(SurrealDbConfig {
                endpoint,
                namespace,
                database,
                auth,
            })
        }
    };

    let _storage = create_storage(&storage_config).await?;

    // Placeholder for TUI implementation
    println!("bird-tui initialized successfully");

    Ok(())
}
