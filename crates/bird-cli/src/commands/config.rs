//! Config file commands.

use crate::cli::Cli;
use crate::output::format_json;
use colored::Colorize;
use std::fs;

const DEFAULT_CONFIG: &str = r#"# bird config
[storage]
backend = "surrealdb"
# db_path = "~/.bird/bird.db"
# db_url = "wss://cloud.surrealdb.com"
namespace = "bird"
database = "main"
# auth = "root"
# user = "your_user"
# pass = "your_pass"
"#;

/// Initialize a config file.
pub async fn run_init(cli: &Cli, force: bool, show_emoji: bool) -> anyhow::Result<()> {
    let path = cli.config_path();
    let existed = path.exists();

    if existed && !force {
        anyhow::bail!(
            "Config file already exists at {} (use --force to overwrite)",
            path.display()
        );
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            anyhow::anyhow!(
                "Failed to create config directory {}: {}",
                parent.display(),
                e
            )
        })?;
    }

    fs::write(&path, DEFAULT_CONFIG)
        .map_err(|e| anyhow::anyhow!("Failed to write config file {}: {}", path.display(), e))?;

    if cli.json() {
        #[derive(serde::Serialize)]
        struct ConfigInitOutput {
            path: String,
            overwritten: bool,
        }

        let output = ConfigInitOutput {
            path: path.display().to_string(),
            overwritten: existed && force,
        };
        println!("{}", format_json(&output));
        return Ok(());
    }

    let icon = if show_emoji { "📝 " } else { "" };
    println!(
        "{}Wrote config to {}",
        icon,
        path.display().to_string().green()
    );

    Ok(())
}
