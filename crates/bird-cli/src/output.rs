//! Output formatting for CLI.

use bird_client::{CurrentUser, MediaType, TweetData, TwitterUser};
use colored::Colorize;

/// Options for formatting output.
#[derive(Debug, Clone, Default)]
pub struct FormatOptions {
    /// Whether to show emoji in output.
    pub show_emoji: bool,
    /// Whether to show LLM-generated headlines for long tweets.
    pub show_headline: bool,
}

/// Format a tweet for text output.
pub fn format_tweet(tweet: &TweetData, opts: &FormatOptions) -> String {
    let mut output = String::new();

    // Header: author
    let author_line = format!(
        "{} @{}",
        tweet.author.name.bold(),
        tweet.author.username.cyan()
    );
    output.push_str(&author_line);
    output.push('\n');

    // Show headline if available and requested
    if opts.show_headline {
        if let Some(ref headline) = tweet.headline {
            let icon = if opts.show_emoji { "📝 " } else { "" };
            output.push_str(&format!(
                "{}Headline: {}\n",
                icon,
                headline.italic().yellow()
            ));
        }
    }

    // Tweet text
    output.push_str(&tweet.text);
    output.push('\n');

    // Metadata line
    let mut meta_parts = Vec::new();

    if let Some(created_at) = &tweet.created_at {
        meta_parts.push(created_at.dimmed().to_string());
    }

    if let Some(reply_count) = tweet.reply_count {
        let icon = if opts.show_emoji { "💬 " } else { "" };
        meta_parts.push(
            format!("{}{} replies", icon, reply_count)
                .dimmed()
                .to_string(),
        );
    }

    if let Some(retweet_count) = tweet.retweet_count {
        let icon = if opts.show_emoji { "🔁 " } else { "" };
        meta_parts.push(
            format!("{}{} retweets", icon, retweet_count)
                .dimmed()
                .to_string(),
        );
    }

    if let Some(like_count) = tweet.like_count {
        let icon = if opts.show_emoji { "❤️ " } else { "" };
        meta_parts.push(format!("{}{} likes", icon, like_count).dimmed().to_string());
    }

    if !meta_parts.is_empty() {
        output.push_str(&meta_parts.join(" · "));
        output.push('\n');
    }

    // Media attachments
    if let Some(media) = &tweet.media {
        for m in media {
            let icon = if opts.show_emoji {
                match m.media_type {
                    MediaType::Photo => "📷 ",
                    MediaType::Video => "🎬 ",
                    MediaType::AnimatedGif => "🎞️ ",
                }
            } else {
                ""
            };
            output.push_str(&format!("{}{}\n", icon, m.url.blue()));
        }
    }

    // Article
    if let Some(article) = &tweet.article {
        let icon = if opts.show_emoji { "📰 " } else { "" };
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
        for line in format_tweet(quoted, opts).lines() {
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

/// Format a list of tweets for text output.
pub fn format_tweets(tweets: &[TweetData], opts: &FormatOptions) -> String {
    let mut output = String::new();

    for (i, tweet) in tweets.iter().enumerate() {
        if i > 0 {
            output.push_str(&"─".repeat(40).dimmed().to_string());
            output.push('\n');
        }
        output.push_str(&format_tweet(tweet, opts));
    }

    output
}

/// Format output as JSON.
pub fn format_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string())
}

/// Format a summary line for paginated results.
pub fn format_pagination_summary(
    count: usize,
    total_fetched: usize,
    has_more: bool,
    next_cursor: Option<&str>,
    show_emoji: bool,
) -> String {
    let mut parts = Vec::new();

    let icon = if show_emoji { "📊 " } else { "" };
    parts.push(format!("{}Showing {} tweets", icon, count));

    if total_fetched > count {
        parts.push(format!("({} total fetched)", total_fetched));
    }

    if has_more {
        parts.push("more available".to_string());
    }

    let mut output = parts.join(" · ").dimmed().to_string();

    if let Some(cursor) = next_cursor {
        output.push_str(
            &format!(
                "\n{}Resume with: --cursor \"{}\"",
                if show_emoji { "➡️ " } else { "" },
                cursor
            )
            .dimmed()
            .to_string(),
        );
    }

    output
}
