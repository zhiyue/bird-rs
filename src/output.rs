//! Output formatting for CLI.

use crate::types::{CurrentUser, TweetData, TwitterUser};
use colored::Colorize;

/// Format a tweet for text output.
pub fn format_tweet(tweet: &TweetData, show_emoji: bool) -> String {
    let mut output = String::new();

    // Header: author
    let author_line = format!(
        "{} @{}",
        tweet.author.name.bold(),
        tweet.author.username.cyan()
    );
    output.push_str(&author_line);
    output.push('\n');

    // Tweet text
    output.push_str(&tweet.text);
    output.push('\n');

    // Metadata line
    let mut meta_parts = Vec::new();

    if let Some(created_at) = &tweet.created_at {
        meta_parts.push(created_at.dimmed().to_string());
    }

    if let Some(reply_count) = tweet.reply_count {
        let icon = if show_emoji { "💬 " } else { "" };
        meta_parts.push(
            format!("{}{} replies", icon, reply_count)
                .dimmed()
                .to_string(),
        );
    }

    if let Some(retweet_count) = tweet.retweet_count {
        let icon = if show_emoji { "🔁 " } else { "" };
        meta_parts.push(
            format!("{}{} retweets", icon, retweet_count)
                .dimmed()
                .to_string(),
        );
    }

    if let Some(like_count) = tweet.like_count {
        let icon = if show_emoji { "❤️ " } else { "" };
        meta_parts.push(format!("{}{} likes", icon, like_count).dimmed().to_string());
    }

    if !meta_parts.is_empty() {
        output.push_str(&meta_parts.join(" · "));
        output.push('\n');
    }

    // Media attachments
    if let Some(media) = &tweet.media {
        for m in media {
            let icon = if show_emoji {
                match m.media_type {
                    crate::types::MediaType::Photo => "📷 ",
                    crate::types::MediaType::Video => "🎬 ",
                    crate::types::MediaType::AnimatedGif => "🎞️ ",
                }
            } else {
                ""
            };
            output.push_str(&format!("{}{}\n", icon, m.url.blue()));
        }
    }

    // Article
    if let Some(article) = &tweet.article {
        let icon = if show_emoji { "📰 " } else { "" };
        output.push_str(&format!("{}Article: {}\n", icon, article.title.bold()));
        if let Some(preview) = &article.preview_text {
            output.push_str(&format!("  {}\n", preview.dimmed()));
        }
    }

    // Quoted tweet
    if let Some(quoted) = &tweet.quoted_tweet {
        output.push('\n');
        output.push_str(&"┌─ Quoted Tweet ─".dimmed().to_string());
        output.push('\n');
        for line in format_tweet(quoted, show_emoji).lines() {
            output.push_str(&format!("│ {}\n", line));
        }
        output.push_str(&"└────────────────".dimmed().to_string());
        output.push('\n');
    }

    // Tweet URL
    let url = format!(
        "https://x.com/{}/status/{}",
        tweet.author.username, tweet.id
    );
    output.push_str(&url.blue().underline().to_string());
    output.push('\n');

    output
}

/// Format the current user for text output.
pub fn format_current_user(user: &CurrentUser, show_emoji: bool) -> String {
    let icon = if show_emoji { "👤 " } else { "" };
    format!(
        "{}Logged in as {} (@{})\nUser ID: {}",
        icon,
        user.name.bold(),
        user.username.cyan(),
        user.id.dimmed()
    )
}

/// Format a user profile for text output.
pub fn format_user(user: &TwitterUser, show_emoji: bool) -> String {
    let mut output = String::new();

    let verified_badge = if user.is_blue_verified.unwrap_or(false) {
        if show_emoji {
            " ✓"
        } else {
            " [verified]"
        }
    } else {
        ""
    };

    output.push_str(&format!(
        "{}{} @{}\n",
        user.name.bold(),
        verified_badge.blue(),
        user.username.cyan()
    ));

    if let Some(desc) = &user.description {
        output.push_str(desc);
        output.push('\n');
    }

    let mut meta_parts = Vec::new();

    if let Some(followers) = user.followers_count {
        meta_parts.push(format!("{} followers", followers));
    }

    if let Some(following) = user.following_count {
        meta_parts.push(format!("{} following", following));
    }

    if let Some(created_at) = &user.created_at {
        meta_parts.push(format!("Joined {}", created_at));
    }

    if !meta_parts.is_empty() {
        output.push_str(&meta_parts.join(" · ").dimmed().to_string());
        output.push('\n');
    }

    output
}

/// Format output as JSON.
pub fn format_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string())
}
