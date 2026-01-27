//! Whoami command implementation.

use crate::cli::Cli;
use crate::output::{format_current_user, format_json};
use bird_client::CurrentUserResult;

/// Run the whoami command.
pub async fn run(cli: &Cli, show_emoji: bool) -> anyhow::Result<()> {
    let mut client = cli.create_client()?;

    match client.get_current_user().await {
        CurrentUserResult::Success(user) => {
            if cli.json() {
                println!("{}", format_json(&user));
            } else {
                println!("{}", format_current_user(&user, show_emoji));
            }
        }
        CurrentUserResult::Error(e) => {
            anyhow::bail!("Failed to get current user: {}", e);
        }
    }

    Ok(())
}
