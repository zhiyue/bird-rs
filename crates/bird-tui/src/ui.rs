//! UI rendering for bird-tui using ratatui.

use crate::app::{App, Focus};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

/// Render the entire TUI frame.
pub fn render(f: &mut Frame, app: &App) {
    // If loading, show loading state
    if app.loading {
        render_loading(f, app);
        return;
    }

    // If help is shown, show help modal
    if app.show_help {
        render_help(f, app);
        return;
    }

    // If error, show error
    if let Some(error) = &app.error {
        render_error(f, app, error);
        return;
    }

    // If no tweets, show empty state
    if app.tweets.is_empty() {
        render_empty(f, app);
        return;
    }

    // Create main layout: left panel (40%) and right panel (60%)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(f.area());

    // Render left panel (tweet list)
    render_left_panel(f, app, chunks[0]);

    // Render right panel (tweet details)
    render_right_panel(f, app, chunks[1]);
}

/// Render the left panel with tweet list.
fn render_left_panel(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .tweets
        .iter()
        .enumerate()
        .map(|(i, tweet)| {
            let emoji = collection_emoji(&tweet.collections);
            let id_display = truncate_id(&tweet.id, 10);
            let headline = truncate_text(&tweet.headline, 30);

            let content = if emoji.is_empty() {
                format!("{} {}", id_display, headline)
            } else {
                format!("{} {} {}", id_display, headline, emoji)
            };

            let style = if i == app.selected_index {
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(format!(
                    " Tweets (Page {}/{}) ",
                    app.current_page + 1,
                    (app.total_count as u32).div_ceil(app.page_size)
                ))
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded),
        )
        .style(if app.focus == Focus::List {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        });

    f.render_widget(list, area);
}

/// Render the right panel with tweet details.
fn render_right_panel(f: &mut Frame, app: &App, area: Rect) {
    if let Some(tweet) = app.selected_tweet() {
        // Create sub-layout for detail panel: title area and content area
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(10)])
            .split(area);

        // Metadata section
        let metadata = vec![
            Line::from(vec![
                Span::styled("Author: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!(
                    "@{} ({})",
                    tweet.author_username, tweet.author_name
                )),
            ]),
            Line::from(vec![
                Span::styled("Created: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(
                    tweet
                        .created_at
                        .as_ref()
                        .cloned()
                        .unwrap_or_else(|| "—".to_string()),
                ),
            ]),
            Line::from(vec![
                Span::styled("Resonance: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!(
                    "{:.1} (♡:{} 📌:{} ↩:{} 🔄:{})",
                    tweet.resonance_score.total,
                    if tweet.resonance_score.liked {
                        "✓"
                    } else {
                        "—"
                    },
                    if tweet.resonance_score.bookmarked {
                        "✓"
                    } else {
                        "—"
                    },
                    tweet.resonance_score.reply_count,
                    tweet.resonance_score.retweet_count,
                )),
            ]),
        ];

        let metadata_block = Paragraph::new(metadata)
            .block(Block::default().borders(Borders::BOTTOM))
            .style(Style::default());

        f.render_widget(metadata_block, chunks[0]);

        // Tweet text content (scrollable)
        let text_content = vec![Line::from(Span::raw(tweet.text.as_str()))];

        let text_block = Paragraph::new(text_content)
            .block(
                Block::default()
                    .title(" Tweet Text ")
                    .borders(Borders::ALL)
                    .border_type(ratatui::widgets::BorderType::Rounded),
            )
            .wrap(Wrap { trim: true })
            .scroll((app.detail_scroll_offset as u16, 0))
            .style(if app.focus == Focus::Detail {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            });

        f.render_widget(text_block, chunks[1]);
    } else {
        let block = Block::default()
            .title(" Tweet Details ")
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded);
        f.render_widget(block, area);
    }
}

/// Render loading state.
fn render_loading(f: &mut Frame, _app: &App) {
    let block = Block::default()
        .title(" Loading... ")
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded);

    let paragraph = Paragraph::new("Loading tweets...").alignment(Alignment::Center);

    f.render_widget(block, f.area());
    f.render_widget(paragraph, f.area());
}

/// Render empty state.
fn render_empty(f: &mut Frame, _app: &App) {
    let block = Block::default()
        .title(" No Tweets ")
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded);

    let paragraph = Paragraph::new(
        "No tweets found. Try syncing with `bird sync likes` or `bird sync bookmarks`.",
    )
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });

    f.render_widget(block, f.area());
    f.render_widget(paragraph, f.area());
}

/// Render error state.
fn render_error(f: &mut Frame, _app: &App, error: &str) {
    let block = Block::default()
        .title(" Error ")
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(Style::default().fg(Color::Red));

    let paragraph = Paragraph::new(error)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(block, f.area());
    f.render_widget(paragraph, f.area());
}

/// Render help modal.
fn render_help(f: &mut Frame, _app: &App) {
    let size = f.area();

    // Create a centered modal
    let modal_width = 60;
    let modal_height = 20;
    let x = (size.width.saturating_sub(modal_width)) / 2;
    let y = (size.height.saturating_sub(modal_height)) / 2;

    let modal_area = Rect {
        x,
        y,
        width: modal_width,
        height: modal_height,
    };

    // Background
    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Double)
            .style(Style::default().bg(Color::Black).fg(Color::White)),
        modal_area,
    );

    // Help text
    let help_text = vec![
        Line::from(Span::styled(
            "Keyboard Shortcuts",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from("↑/↓      Navigate list"),
        Line::from("←/→      Previous/Next page"),
        Line::from("Tab      Switch focus (list ↔ detail)"),
        Line::from("PgUp/PgDn  Scroll detail"),
        Line::from("Ctrl+?   Toggle help"),
        Line::from("q/Esc    Quit"),
        Line::from(""),
        Line::from(Span::styled(
            "Press Ctrl+? or Esc to close",
            Style::default().add_modifier(Modifier::DIM),
        )),
    ];

    let inner = Rect {
        x: modal_area.x + 1,
        y: modal_area.y + 1,
        width: modal_area.width.saturating_sub(2),
        height: modal_area.height.saturating_sub(2),
    };

    let help_widget = Paragraph::new(help_text).style(Style::default());
    f.render_widget(help_widget, inner);
}

/// Get collection emoji representation.
fn collection_emoji(collections: &[String]) -> String {
    let mut result = String::new();
    for collection in collections {
        match collection.as_str() {
            "likes" => result.push('❤'),
            "bookmarks" => result.push('🔖'),
            "user_tweets" => result.push('📝'),
            _ => {}
        }
    }
    result
}

/// Truncate tweet ID to a maximum length.
fn truncate_id(id: &str, max_len: usize) -> String {
    if id.len() <= max_len {
        id.to_string()
    } else {
        format!("{}…", &id[..max_len - 1])
    }
}

/// Truncate text to a maximum length, adding ellipsis if needed.
fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}…", &text[..max_len - 1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collection_emoji() {
        let collections = vec!["likes".to_string()];
        assert_eq!(collection_emoji(&collections), "❤");

        let collections = vec!["bookmarks".to_string(), "likes".to_string()];
        assert_eq!(collection_emoji(&collections), "🔖❤");
    }

    #[test]
    fn test_truncate_id() {
        assert_eq!(truncate_id("12345", 10), "12345");
        assert_eq!(truncate_id("12345678901234567890", 5), "1234…");
    }
}
