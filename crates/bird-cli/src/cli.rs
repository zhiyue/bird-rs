//! CLI interface for bird.

use crate::commands::{bookmarks, config, db, insights, likes, list, read, sync, whoami};
use bird_client::cookies::{check_available_sources, resolve_credentials};
use bird_client::{Collection, TwitterClient, TwitterClientOptions};
use bird_storage::{
    create_storage, default_db_path, StorageConfig, SurrealDbAuth, SurrealDbConfig,
};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

/// A fast X/Twitter CLI for reading tweets.
#[derive(Parser)]
#[command(name = "bird")]
#[command(author, version, about, long_about = None)]
#[command(after_help = "Examples:
  bird whoami                    Show the logged-in account
  bird read 1234567890           Read a tweet by ID
  bird 1234567890                Shorthand for read
  bird check                     Show available credential sources
  bird likes                     Fetch your likes
  bird likes --all               Fetch all pages of likes
  bird bookmarks --max-pages 5   Fetch up to 5 pages of bookmarks
  bird sync likes                Sync likes to local database
  bird db backfill-created-at    Backfill created_at_ts for existing tweets
  bird --config ~/.bird/config.toml sync likes
  bird config init               Create a default config file
  bird sync status               Show sync state
  bird insights generate         Analyze tweets for insights
  bird insights generate week    Analyze tweets from the last week")]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Twitter auth_token cookie (overrides browser extraction).
    #[arg(long, global = true, env = "AUTH_TOKEN")]
    auth_token: Option<String>,

    /// Twitter ct0 cookie (overrides browser extraction).
    #[arg(long, global = true, env = "CT0")]
    ct0: Option<String>,

    /// Request timeout in milliseconds.
    #[arg(long, global = true, default_value = "30000")]
    timeout: u64,

    /// Max quoted tweet depth (0 disables).
    #[arg(long, global = true, default_value = "1")]
    quote_depth: u32,

    /// Output as JSON.
    #[arg(long, global = true)]
    json: bool,

    /// Plain output (no emoji, no color).
    #[arg(long, global = true)]
    plain: bool,

    /// Disable emoji output.
    #[arg(long, global = true)]
    no_emoji: bool,

    /// Skip cache and fetch directly from API.
    #[arg(long, global = true)]
    no_cache: bool,

    /// Path to config file (defaults to ~/.bird/config.toml).
    #[arg(long, global = true, env = "BIRD_CONFIG")]
    config: Option<PathBuf>,

    /// Path to the database file (local SurrealDB only).
    #[arg(long, global = true, env = "BIRD_DB_PATH")]
    db_path: Option<PathBuf>,

    /// Storage backend to use (surrealdb, memory). Defaults to surrealdb.
    #[arg(long, global = true, env = "BIRD_STORAGE")]
    storage: Option<StorageBackend>,

    /// SurrealDB connection endpoint (e.g., ws://localhost:8000, https://cloud.surrealdb.com).
    #[arg(long, global = true, env = "BIRD_DB_URL")]
    db_url: Option<String>,

    /// SurrealDB namespace (default: bird).
    #[arg(long, global = true, env = "BIRD_DB_NAMESPACE")]
    db_namespace: Option<String>,

    /// SurrealDB database name (default: main).
    #[arg(long, global = true, env = "BIRD_DB_NAME")]
    db_name: Option<String>,

    /// SurrealDB authentication mode (root, namespace, database).
    #[arg(long, global = true, env = "BIRD_DB_AUTH")]
    db_auth: Option<DbAuthMode>,

    /// SurrealDB username.
    #[arg(long, global = true, env = "BIRD_DB_USER")]
    db_user: Option<String>,

    /// SurrealDB password.
    #[arg(long, global = true, env = "BIRD_DB_PASS")]
    db_pass: Option<String>,

    /// Tweet ID or URL (shorthand for `read`).
    #[arg(value_name = "TWEET_ID_OR_URL")]
    tweet_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum StorageBackend {
    /// SurrealDB-backed storage.
    Surrealdb,
    /// In-memory storage (testing only).
    Memory,
}

#[derive(Clone, Copy, Debug, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum DbAuthMode {
    Root,
    Namespace,
    Database,
}

#[derive(Debug, Default, Deserialize)]
struct BirdConfig {
    storage: Option<StorageConfigFile>,
}

#[derive(Debug, Default, Deserialize)]
struct StorageConfigFile {
    backend: Option<StorageBackend>,
    db_path: Option<PathBuf>,
    db_url: Option<String>,
    namespace: Option<String>,
    database: Option<String>,
    auth: Option<DbAuthMode>,
    user: Option<String>,
    pass: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show the logged-in account.
    Whoami,

    /// Check available credential sources.
    Check,

    /// Read a tweet by ID or URL (cache-first).
    Read {
        /// Tweet ID or URL.
        tweet_id: String,

        /// Include raw API response in output.
        #[arg(long)]
        json_full: bool,
    },

    /// Fetch your liked tweets.
    Likes {
        /// Fetch all available pages.
        #[arg(long)]
        all: bool,

        /// Maximum number of pages to fetch.
        #[arg(long)]
        max_pages: Option<u32>,

        /// Resume from a specific cursor.
        #[arg(long)]
        cursor: Option<String>,
    },

    /// Fetch your bookmarked tweets.
    Bookmarks {
        /// Fetch all available pages.
        #[arg(long)]
        all: bool,

        /// Maximum number of pages to fetch.
        #[arg(long)]
        max_pages: Option<u32>,

        /// Resume from a specific cursor.
        #[arg(long)]
        cursor: Option<String>,
    },

    /// List tweets from the local database.
    List {
        /// Collection to list (likes, bookmarks, user_tweets).
        #[arg(default_value = "likes")]
        collection: String,

        /// Page number (1-indexed).
        #[arg(long, default_value = "1")]
        page: u32,

        /// Number of tweets per page.
        #[arg(long)]
        page_size: Option<u32>,
    },

    /// Sync tweets to local database.
    Sync {
        #[command(subcommand)]
        action: SyncAction,
    },

    /// Database utilities.
    Db {
        #[command(subcommand)]
        action: DbAction,
    },

    /// Config file utilities.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Generate insights from synced tweets using LLM analysis.
    Insights {
        #[command(subcommand)]
        action: InsightsAction,
    },
}

#[derive(Subcommand)]
enum SyncAction {
    /// Sync liked tweets to database.
    Likes {
        /// Full re-sync (ignore previous sync state).
        #[arg(long)]
        full: bool,

        /// Maximum number of pages to fetch (default: 10).
        #[arg(long)]
        max_pages: Option<u32>,

        /// Delay between page requests in milliseconds (default: 1000).
        #[arg(long)]
        delay: Option<u64>,

        /// Skip backfill, only fetch new items.
        #[arg(long)]
        no_backfill: bool,

        /// Maximum storage size before stopping sync (e.g., "5GB", "500MB").
        #[arg(long)]
        max_storage: Option<String>,
    },

    /// Sync bookmarked tweets to database.
    Bookmarks {
        /// Full re-sync (ignore previous sync state).
        #[arg(long)]
        full: bool,

        /// Maximum number of pages to fetch (default: 10).
        #[arg(long)]
        max_pages: Option<u32>,

        /// Delay between page requests in milliseconds (default: 1000).
        #[arg(long)]
        delay: Option<u64>,

        /// Skip backfill, only fetch new items.
        #[arg(long)]
        no_backfill: bool,

        /// Maximum storage size before stopping sync (e.g., "5GB", "500MB").
        #[arg(long)]
        max_storage: Option<String>,
    },

    /// Sync your own tweets/posts to database.
    Posts {
        /// Full re-sync (ignore previous sync state).
        #[arg(long)]
        full: bool,

        /// Maximum number of pages to fetch (default: 10).
        #[arg(long)]
        max_pages: Option<u32>,

        /// Delay between page requests in milliseconds (default: 1000).
        #[arg(long)]
        delay: Option<u64>,

        /// Skip backfill, only fetch new items.
        #[arg(long)]
        no_backfill: bool,

        /// Maximum storage size before stopping sync (e.g., "5GB", "500MB").
        #[arg(long)]
        max_storage: Option<String>,
    },

    /// Continue backfilling older tweets.
    Backfill {
        /// Collection to backfill (likes, bookmarks).
        collection: String,

        /// Maximum number of pages to fetch (default: 10).
        #[arg(long)]
        max_pages: Option<u32>,

        /// Delay between page requests in milliseconds (default: 1000).
        #[arg(long)]
        delay: Option<u64>,

        /// Maximum storage size before stopping sync (e.g., "5GB", "500MB").
        #[arg(long)]
        max_storage: Option<String>,
    },

    /// Show sync status for all collections.
    Status,

    /// Reset sync state for a collection.
    Reset {
        /// Collection to reset (likes, bookmarks).
        collection: String,
    },
}

#[derive(Subcommand)]
enum DbAction {
    /// Backfill created_at_ts for existing tweets.
    BackfillCreatedAt {
        /// Batch size per query (default: 200).
        #[arg(long)]
        batch_size: Option<u32>,
    },

    /// Show database status and counts.
    Status {
        /// Show debug info including timestamp distribution.
        #[arg(long)]
        debug: bool,
    },

    /// Ensure database schema and indexes exist.
    Optimize,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Create a default config file.
    Init {
        /// Overwrite existing config file.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum InsightsAction {
    /// Generate insights from synced tweets.
    Generate {
        /// Time period to analyze (day, week, month). Default: week.
        #[arg(default_value = "week")]
        period: Option<String>,

        /// Collection to analyze (all, likes, bookmarks). Default: all.
        #[arg(long)]
        collection: Option<String>,

        /// Maximum number of tweets to analyze.
        #[arg(long)]
        max_tweets: Option<u32>,

        /// LLM provider to use. Default: claude-code (uses MAX subscription).
        /// Options: claude-code, anthropic-api
        #[arg(long, default_value = "claude-code")]
        provider: String,

        /// LLM model to use (overrides BIRD_LLM_MODEL env var).
        #[arg(long)]
        model: Option<String>,

        /// Show verbose output including model info.
        #[arg(long, short)]
        verbose: bool,
    },
}

impl Cli {
    /// Run the CLI.
    pub async fn run(self) -> anyhow::Result<()> {
        let show_emoji = !self.plain && !self.no_emoji;

        // Handle commands
        match &self.command {
            Some(Commands::Check) => self.run_check(),
            Some(Commands::Whoami) => whoami::run(&self, show_emoji).await,
            Some(Commands::Read {
                tweet_id,
                json_full,
            }) => read::run(&self, tweet_id, *json_full, show_emoji).await,
            Some(Commands::Likes {
                all,
                max_pages,
                cursor,
            }) => likes::run(&self, *all, *max_pages, cursor.clone(), show_emoji).await,
            Some(Commands::Bookmarks {
                all,
                max_pages,
                cursor,
            }) => bookmarks::run(&self, *all, *max_pages, cursor.clone(), show_emoji).await,
            Some(Commands::List {
                collection,
                page,
                page_size,
            }) => list::run(&self, collection, *page, *page_size, show_emoji).await,
            Some(Commands::Sync { action }) => match action {
                SyncAction::Likes {
                    full,
                    max_pages,
                    delay,
                    no_backfill,
                    max_storage,
                } => {
                    sync::run_sync_likes(
                        &self,
                        *full,
                        *max_pages,
                        *delay,
                        *no_backfill,
                        max_storage.clone(),
                        show_emoji,
                    )
                    .await
                }
                SyncAction::Bookmarks {
                    full,
                    max_pages,
                    delay,
                    no_backfill,
                    max_storage,
                } => {
                    sync::run_sync_bookmarks(
                        &self,
                        *full,
                        *max_pages,
                        *delay,
                        *no_backfill,
                        max_storage.clone(),
                        show_emoji,
                    )
                    .await
                }
                SyncAction::Posts {
                    full,
                    max_pages,
                    delay,
                    no_backfill,
                    max_storage,
                } => {
                    sync::run_sync_posts(
                        &self,
                        *full,
                        *max_pages,
                        *delay,
                        *no_backfill,
                        max_storage.clone(),
                        show_emoji,
                    )
                    .await
                }
                SyncAction::Backfill {
                    collection,
                    max_pages,
                    delay,
                    max_storage,
                } => {
                    let coll: Collection =
                        collection.parse().map_err(|e: String| anyhow::anyhow!(e))?;
                    sync::run_backfill(
                        &self,
                        coll,
                        *max_pages,
                        *delay,
                        max_storage.clone(),
                        show_emoji,
                    )
                    .await
                }
                SyncAction::Status => sync::run_status(&self, show_emoji).await,
                SyncAction::Reset { collection } => sync::run_reset(&self, collection).await,
            },
            Some(Commands::Db { action }) => match action {
                DbAction::BackfillCreatedAt { batch_size } => {
                    db::run_backfill_created_at(&self, *batch_size, show_emoji).await
                }
                DbAction::Status { debug } => db::run_status(&self, show_emoji, *debug).await,
                DbAction::Optimize => db::run_optimize(&self, show_emoji).await,
            },
            Some(Commands::Config { action }) => match action {
                ConfigAction::Init { force } => config::run_init(&self, *force, show_emoji).await,
            },
            Some(Commands::Insights { action }) => match action {
                InsightsAction::Generate {
                    period,
                    collection,
                    max_tweets,
                    provider,
                    model,
                    verbose,
                } => {
                    insights::run_generate(
                        &self,
                        period.clone(),
                        collection.clone(),
                        *max_tweets,
                        provider.clone(),
                        model.clone(),
                        *verbose,
                        show_emoji,
                    )
                    .await
                }
            },
            None => {
                // Check for shorthand tweet ID
                if let Some(tweet_id) = &self.tweet_id {
                    return read::run(&self, tweet_id, false, show_emoji).await;
                }

                // No command provided, show help
                use clap::CommandFactory;
                let mut cmd = Cli::command();
                cmd.print_help()?;
                Ok(())
            }
        }
    }

    /// Run the check command.
    fn run_check(&self) -> anyhow::Result<()> {
        let sources = check_available_sources();

        if sources.is_empty() {
            println!("No credential sources available.");
            println!();
            println!("To authenticate, either:");
            println!("  1. Log in to x.com in Safari");
            println!("  2. Set AUTH_TOKEN and CT0 environment variables");
            println!("  3. Pass --auth-token and --ct0 flags");
        } else {
            println!("Available credential sources:");
            for source in sources {
                println!("  - {}", source);
            }
        }

        Ok(())
    }

    /// Create a Twitter client from CLI options.
    pub fn create_client(&self) -> anyhow::Result<TwitterClient> {
        let cookies = resolve_credentials(self.auth_token.as_deref(), self.ct0.as_deref(), &[])?;

        Ok(TwitterClient::new(TwitterClientOptions {
            cookies,
            timeout_ms: Some(self.timeout),
            quote_depth: Some(self.quote_depth),
        }))
    }

    pub(crate) fn config_path(&self) -> PathBuf {
        self.config.clone().unwrap_or_else(default_config_path)
    }

    fn load_config(&self) -> anyhow::Result<BirdConfig> {
        let path = self.config_path();
        if !path.exists() {
            if self.config.is_some() {
                anyhow::bail!("Config file not found: {}", path.display());
            }
            return Ok(BirdConfig::default());
        }

        let contents = fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read config file {}: {}", path.display(), e))?;
        toml::from_str(&contents)
            .map_err(|e| anyhow::anyhow!("Failed to parse config file {}: {}", path.display(), e))
    }

    pub(crate) fn storage_config(&self) -> anyhow::Result<StorageConfig> {
        let config = self.load_config()?;
        let storage_cfg = config.storage.unwrap_or_default();
        let backend = self
            .storage
            .or(storage_cfg.backend)
            .unwrap_or(StorageBackend::Surrealdb);

        match backend {
            StorageBackend::Memory => Ok(StorageConfig::Memory),
            StorageBackend::Surrealdb => {
                let db_url = self.db_url.clone().or(storage_cfg.db_url);
                let db_path = self
                    .db_path
                    .clone()
                    .or(storage_cfg.db_path)
                    .unwrap_or_else(default_db_path);
                let endpoint = db_url.unwrap_or_else(|| format!("rocksdb://{}", db_path.display()));
                let namespace = self
                    .db_namespace
                    .clone()
                    .or(storage_cfg.namespace)
                    .unwrap_or_else(|| "bird".to_string());
                let database = self
                    .db_name
                    .clone()
                    .or(storage_cfg.database)
                    .unwrap_or_else(|| "main".to_string());
                let auth_mode = self
                    .db_auth
                    .or(storage_cfg.auth)
                    .unwrap_or(DbAuthMode::Root);
                let db_user = self.db_user.clone().or(storage_cfg.user);
                let db_pass = self.db_pass.clone().or(storage_cfg.pass);

                let auth = if db_user.is_some() || db_pass.is_some() {
                    let username = db_user.as_deref().ok_or_else(|| {
                        anyhow::anyhow!("--db-user is required when --db-pass is set")
                    })?;
                    let password = db_pass.as_deref().ok_or_else(|| {
                        anyhow::anyhow!("--db-pass is required when --db-user is set")
                    })?;

                    Some(match auth_mode {
                        DbAuthMode::Root => SurrealDbAuth::Root {
                            username: username.to_string(),
                            password: password.to_string(),
                        },
                        DbAuthMode::Namespace => SurrealDbAuth::Namespace {
                            username: username.to_string(),
                            password: password.to_string(),
                        },
                        DbAuthMode::Database => SurrealDbAuth::Database {
                            username: username.to_string(),
                            password: password.to_string(),
                        },
                    })
                } else {
                    None
                };

                Ok(StorageConfig::SurrealDb(SurrealDbConfig {
                    endpoint,
                    namespace,
                    database,
                    auth,
                }))
            }
        }
    }

    /// Get the database path.
    pub fn db_path(&self) -> PathBuf {
        self.db_path.clone().unwrap_or_else(default_db_path)
    }

    /// Create a storage instance.
    pub async fn create_storage(&self) -> anyhow::Result<Arc<dyn bird_storage::Storage>> {
        let config = self.storage_config()?;

        create_storage(&config)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Check if JSON output is enabled.
    pub fn json(&self) -> bool {
        self.json
    }

    /// Check if cache should be skipped.
    pub fn no_cache(&self) -> bool {
        self.no_cache
    }
}

fn default_config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".bird")
        .join("config.toml")
}

/// Extract a tweet ID from a URL or return the ID directly.
pub fn extract_tweet_id(input: &str) -> anyhow::Result<String> {
    // If it looks like a URL, try to parse it
    if input.contains('/') || input.contains("twitter.com") || input.contains("x.com") {
        // Try to extract ID from various URL formats:
        // https://twitter.com/user/status/1234567890
        // https://x.com/user/status/1234567890
        // twitter.com/user/status/1234567890/...

        let url = if input.starts_with("http") {
            input.to_string()
        } else {
            format!("https://{}", input)
        };

        if let Ok(parsed) = url::Url::parse(&url) {
            let segments: Vec<&str> = parsed
                .path_segments()
                .map(|s| s.collect())
                .unwrap_or_default();

            // Look for "status" followed by an ID
            for (i, segment) in segments.iter().enumerate() {
                if *segment == "status" && i + 1 < segments.len() {
                    let id = segments[i + 1];
                    // Validate it looks like an ID
                    if id.chars().all(|c| c.is_ascii_digit()) {
                        return Ok(id.to_string());
                    }
                }
            }
        }

        anyhow::bail!("Could not extract tweet ID from URL: {}", input);
    }

    // Validate the ID is numeric
    if !input.chars().all(|c| c.is_ascii_digit()) {
        anyhow::bail!("Invalid tweet ID: {}", input);
    }

    Ok(input.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn base_cli() -> Cli {
        Cli {
            command: None,
            auth_token: None,
            ct0: None,
            timeout: 30000,
            quote_depth: 1,
            json: false,
            plain: false,
            no_emoji: false,
            no_cache: false,
            config: None,
            db_path: None,
            storage: None,
            db_url: None,
            db_namespace: None,
            db_name: None,
            db_auth: None,
            db_user: None,
            db_pass: None,
            tweet_id: None,
        }
    }

    #[test]
    fn test_extract_tweet_id_bare() {
        assert_eq!(
            extract_tweet_id("1234567890123456789").unwrap(),
            "1234567890123456789"
        );
    }

    #[test]
    fn test_extract_tweet_id_x_url() {
        assert_eq!(
            extract_tweet_id("https://x.com/user/status/1234567890123456789").unwrap(),
            "1234567890123456789"
        );
    }

    #[test]
    fn test_extract_tweet_id_twitter_url() {
        assert_eq!(
            extract_tweet_id("https://twitter.com/user/status/1234567890123456789").unwrap(),
            "1234567890123456789"
        );
    }

    #[test]
    fn test_extract_tweet_id_without_https() {
        assert_eq!(
            extract_tweet_id("x.com/user/status/1234567890123456789").unwrap(),
            "1234567890123456789"
        );
    }

    #[test]
    fn test_extract_tweet_id_invalid() {
        assert!(extract_tweet_id("not-a-valid-id").is_err());
    }

    #[test]
    fn test_storage_config_from_file_surrealdb() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[storage]
backend = "surrealdb"
db_url = "wss://cloud.surrealdb.com"
namespace = "bird"
database = "main"
auth = "root"
user = "alice"
pass = "secret"
"#,
        )
        .unwrap();

        let mut cli = base_cli();
        cli.config = Some(path);

        match cli.storage_config().unwrap() {
            StorageConfig::SurrealDb(cfg) => {
                assert_eq!(cfg.endpoint, "wss://cloud.surrealdb.com");
                assert_eq!(cfg.namespace, "bird");
                assert_eq!(cfg.database, "main");
                let auth = cfg.auth.unwrap();
                match auth {
                    SurrealDbAuth::Root { username, password } => {
                        assert_eq!(username, "alice");
                        assert_eq!(password, "secret");
                    }
                    _ => panic!("unexpected auth mode"),
                }
            }
            StorageConfig::Memory => panic!("expected surrealdb config"),
        }
    }

    #[test]
    fn test_storage_config_cli_overrides_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
[storage]
backend = "surrealdb"
db_url = "wss://cloud.surrealdb.com"
namespace = "bird"
database = "main"
"#,
        )
        .unwrap();

        let mut cli = base_cli();
        cli.config = Some(path);
        cli.db_url = Some("ws://localhost:8000".to_string());
        cli.db_namespace = Some("override".to_string());

        match cli.storage_config().unwrap() {
            StorageConfig::SurrealDb(cfg) => {
                assert_eq!(cfg.endpoint, "ws://localhost:8000");
                assert_eq!(cfg.namespace, "override");
                assert_eq!(cfg.database, "main");
            }
            StorageConfig::Memory => panic!("expected surrealdb config"),
        }
    }
}
