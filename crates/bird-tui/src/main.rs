//! bird-tui - A terminal user interface for viewing synced tweets

pub mod app;
pub mod data;
pub mod events;
pub mod ui;

use anyhow::Result;
use app::App;
use bird_client::cookies::resolve_credentials;
use bird_client::{TwitterClient, TwitterClientOptions};
use bird_storage::{
    create_storage, default_db_path, StorageConfig, SurrealDbAuth, SurrealDbConfig,
};
use clap::{Parser, ValueEnum};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "bird-tui")]
#[command(
    author,
    version,
    about = "A terminal user interface for viewing synced tweets"
)]
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

    /// Twitter auth_token cookie (overrides browser extraction).
    #[arg(long, env = "AUTH_TOKEN")]
    auth_token: Option<String>,

    /// Twitter ct0 cookie (overrides browser extraction).
    #[arg(long, env = "CT0")]
    ct0: Option<String>,

    /// Request timeout in milliseconds.
    #[arg(long, default_value = "30000")]
    timeout: u64,
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
    let _config_path = args.config.clone().or_else(|| {
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
            let namespace = args.db_namespace.unwrap_or_else(|| "bird".to_string());
            let database = args.db_name.unwrap_or_else(|| "main".to_string());

            let auth = if let (Some(username), Some(password)) =
                (args.db_user.clone(), args.db_pass.clone())
            {
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

    let storage = create_storage(&storage_config).await?;

    // Get current user ID
    let user_id = match resolve_credentials(args.auth_token.as_deref(), args.ct0.as_deref(), &[]) {
        Ok(cookies) => {
            // Create Twitter client to fetch current user
            let mut client = TwitterClient::new(TwitterClientOptions {
                cookies,
                timeout_ms: Some(args.timeout),
                quote_depth: Some(1),
            });

            match client.get_current_user().await {
                bird_client::CurrentUserResult::Success(user) => user.id,
                bird_client::CurrentUserResult::Error(e) => {
                    eprintln!("Warning: Failed to get current user: {}. Using placeholder.", e);
                    "unknown_user".to_string()
                }
            }
        }
        Err(e) => {
            eprintln!(
                "Warning: Could not resolve credentials: {}. Using placeholder user ID.",
                e
            );
            eprintln!("Provide --auth-token and/or --ct0 to query Twitter API for current user.");
            "unknown_user".to_string()
        }
    };

    // Create app state
    let mut app = App::new(storage, user_id);

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the app
    let result = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

/// Run the main event loop.
async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        // Render the UI
        terminal.draw(|f| {
            ui::render(f, app);
        })?;

        // Handle events
        match events::handle_events(app).await {
            Ok(true) => break, // User quit
            Ok(false) => continue,
            Err(e) => {
                app.set_error(e);
            }
        }
    }

    Ok(())
}
