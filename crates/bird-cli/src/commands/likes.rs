//! Likes command implementation.

use crate::cli::Cli;
use crate::output::{format_json, format_pagination_summary, format_tweets};
use bird_client::{CurrentUserResult, PaginationOptions};

/// Run the likes command.
pub async fn run(
    cli: &Cli,
    all: bool,
    max_pages: Option<u32>,
    cursor: Option<String>,
    show_emoji: bool,
) -> anyhow::Result<()> {
    let mut client = cli.create_client()?;

    // Get current user ID
    let user_id = match client.get_current_user().await {
        CurrentUserResult::Success(user) => user.id,
        CurrentUserResult::Error(e) => {
            anyhow::bail!("Failed to get current user: {}", e);
        }
    };

    // Build pagination options
    let mut options = PaginationOptions::new();
    if let Some(c) = cursor {
        options = options.with_cursor(c);
    }
    if all {
        options = options.fetch_all();
    } else if let Some(max) = max_pages {
        options = options.with_max_pages(max);
    } else {
        // Default to 1 page
        options = options.with_max_pages(1);
    }

    // Fetch likes
    let result = if all || max_pages.is_some() {
        client.get_all_likes(&user_id, max_pages).await?
    } else {
        client.get_likes(&user_id, &options).await?
    };

    if cli.json() {
        println!("{}", format_json(&result.items));
    } else if result.items.is_empty() {
        println!("No likes found.");
    } else {
        print!("{}", format_tweets(&result.items, show_emoji));
        println!();
        println!(
            "{}",
            format_pagination_summary(
                result.items.len(),
                result.total_fetched,
                result.has_more,
                result.next_cursor.as_deref(),
                show_emoji
            )
        );
    }

    Ok(())
}
