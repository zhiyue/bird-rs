//! Bookmarks command implementation.

use crate::cli::Cli;
use crate::output::{format_json, format_pagination_summary, format_tweets};
use bird_client::PaginationOptions;

/// Run the bookmarks command.
pub async fn run(
    cli: &Cli,
    all: bool,
    max_pages: Option<u32>,
    cursor: Option<String>,
    show_emoji: bool,
) -> anyhow::Result<()> {
    let client = cli.create_client()?;

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

    // Fetch bookmarks
    let result = if all || max_pages.is_some() {
        client.get_all_bookmarks(max_pages).await?
    } else {
        client.get_bookmarks(&options).await?
    };

    if cli.json() {
        println!("{}", format_json(&result.items));
    } else {
        if result.items.is_empty() {
            println!("No bookmarks found.");
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
    }

    Ok(())
}
