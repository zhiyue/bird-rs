//! UI rendering for bird-tui using ratatui.

use crate::app::{App, Focus};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, Paragraph, Row, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Table, Wrap,
    },
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

    // If search is shown, show search modal
    if app.show_search {
        render_search_modal(f, app);
        // Still render main content behind the modal
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

    // Create main layout with status bar at bottom
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(1)])
        .split(f.area());

    // Split main area into left (67%) and right (33%) - 2:1 ratio
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(67), Constraint::Percentage(33)])
        .split(main_chunks[0]);

    // Render left panel (tweet list)
    render_left_panel(f, app, content_chunks[0]);

    // Render right panel (tweet details)
    render_right_panel(f, app, content_chunks[1]);

    // Render status bar
    render_status_bar(f, app, main_chunks[1]);
}

/// Render the left panel with tweet list (table with columns and scrollbar).
fn render_left_panel(f: &mut Frame, app: &App, area: Rect) {
    // Split area to make room for scrollbar on the right
    let scrollbar_area = Rect {
        x: area.right().saturating_sub(1),
        y: area.y.saturating_add(1), // Account for border
        width: 1,
        height: area.height.saturating_sub(2), // Account for borders
    };

    let table_area = Rect {
        width: area.width.saturating_sub(1), // Make room for scrollbar
        ..area
    };

    // Define column constraints: ID | Author | Score | Headline | Collections
    let widths = [
        Constraint::Length(8),  // ID
        Constraint::Length(14), // Author
        Constraint::Length(5),  // Score
        Constraint::Length(30), // Headline
        Constraint::Fill(1),    // Collections (takes remaining space)
    ];

    // Get tweets to display (filtered or all)
    let display_tweets = app.display_tweets();
    let selected_index = app.selected_index();

    // Build table rows from filtered tweets
    let rows: Vec<Row> = display_tweets
        .iter()
        .enumerate()
        .map(|(i, tweet)| {
            let id_display = truncate_id(&tweet.id, 8);
            let author_display = truncate_text(&tweet.author_username, 14);
            let score_display = format!("{:.1}", tweet.resonance_score.total);
            let headline = truncate_text(&tweet.headline, 30);
            let emoji = collection_emoji(&tweet.collections);

            let style = if i == selected_index {
                Style::default()
                    .bg(app.theme.highlight)
                    .fg(app.theme.text)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(id_display),
                Cell::from(author_display),
                Cell::from(score_display),
                Cell::from(headline),
                Cell::from(emoji),
            ])
            .style(style)
        })
        .collect();

    // Create table with header
    let table = Table::new(rows, widths)
        .header(
            Row::new(vec!["ID", "Author", "Score", "Headline", "Collections"])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(
            Block::default()
                .title(format!(
                    " Tweets (Page {}/{}) ",
                    app.current_page + 1,
                    (app.total_count as u32).div_ceil(app.page_size)
                ))
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .style(Style::default().fg(app.theme.border)),
        )
        .style(if app.focus == Focus::List {
            Style::default().fg(app.theme.primary)
        } else {
            Style::default()
        });

    f.render_widget(table, table_area);

    // Render scrollbar
    if !display_tweets.is_empty() {
        let mut scrollbar_state =
            ScrollbarState::new(display_tweets.len()).position(selected_index);

        f.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼")),
            scrollbar_area,
            &mut scrollbar_state,
        );
    }
}

/// Render the right panel with tweet details.
fn render_right_panel(f: &mut Frame, app: &App, area: Rect) {
    if let Some(tweet) = app.selected_tweet() {
        // Create sub-layout for detail panel: metadata area and content area
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(8), Constraint::Min(10)])
            .split(area);

        // Metadata section - aligned columns with fixed label width
        let mut metadata = vec![];
        let label_width: usize = 15;

        // Author line
        let author_padding = " ".repeat(label_width.saturating_sub("Author:".len()));
        metadata.push(Line::from(vec![
            Span::styled("Author:", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(author_padding),
            Span::raw(format!(
                "@{} ({})",
                tweet.author_username, tweet.author_name
            )),
        ]));

        // Author interaction stats line
        if tweet.author_liked_count > 0 || tweet.author_quoted_count > 0 || tweet.author_retweeted_count > 0 {
            let author_with_padding = " ".repeat(label_width.saturating_sub("Engagement:".len()));
            metadata.push(Line::from(vec![
                Span::styled("Engagement:", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(author_with_padding),
                Span::raw(format!(
                    "❤ {} liked  💬 {} quoted  🔄 {} retweeted",
                    tweet.author_liked_count, tweet.author_quoted_count, tweet.author_retweeted_count
                )),
            ]));
        }

        // Created line
        let created_padding = " ".repeat(label_width.saturating_sub("Created:".len()));
        metadata.push(Line::from(vec![
            Span::styled("Created:", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(created_padding),
            Span::raw(
                tweet
                    .created_at
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| "—".to_string()),
            ),
        ]));

        // Collections line
        if !tweet.collections.is_empty() {
            let collections_str = tweet
                .collections
                .iter()
                .map(|c| match c.as_str() {
                    "likes" => "❤  Liked",
                    "bookmarks" => "🔖  Bookmarked",
                    "user_tweets" => "📝  Your tweet",
                    _ => "•",
                })
                .collect::<Vec<_>>()
                .join("   ");
            let discovered_padding = " ".repeat(label_width.saturating_sub("Discovered:".len()));
            metadata.push(Line::from(vec![
                Span::styled("Discovered:", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(discovered_padding),
                Span::raw(collections_str),
            ]));
        }

        // Interactions line
        let interactions_padding = " ".repeat(label_width.saturating_sub("Interactions:".len()));
        metadata.push(Line::from(vec![
            Span::styled(
                "Interactions:",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(interactions_padding),
            Span::raw(format!(
                "↩ {}  💬 {}  🔄 {}",
                tweet.resonance_score.reply_count,
                tweet.resonance_score.quote_count,
                tweet.resonance_score.retweet_count
            )),
        ]));

        // Resonance line with color
        let resonance_padding = " ".repeat(label_width.saturating_sub("Resonance:".len()));
        let resonance_value = format!("{:.1}", tweet.resonance_score.total);
        metadata.push(Line::from(vec![
            Span::styled("Resonance:", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(resonance_padding),
            Span::styled(
                resonance_value,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        metadata.push(Line::from("")); // Spacing

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

/// Render loading state with animated spinner.
fn render_loading(f: &mut Frame, app: &App) {
    // Animated spinner symbols
    let spinner_symbols = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spinner_frame = (app.frame as usize) % spinner_symbols.len();
    let spinner = spinner_symbols[spinner_frame];

    let block = Block::default()
        .title(" Loading... ")
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded);

    let paragraph =
        Paragraph::new(format!("{} Loading tweets...", spinner)).alignment(Alignment::Center);

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
        Line::from("↑/↓        Navigate list"),
        Line::from("←/→        Previous/Next page"),
        Line::from("Tab        Switch focus (list ↔ detail)"),
        Line::from("PgUp/PgDn  Scroll detail panel"),
        Line::from("o          Open tweet in browser"),
        Line::from("Ctrl+F     Search tweets"),
        Line::from("Ctrl+T     Toggle dark/light theme"),
        Line::from("Ctrl+?     Toggle help"),
        Line::from("q/Esc      Quit"),
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

/// Render search modal.
fn render_search_modal(f: &mut Frame, app: &App) {
    let size = f.area();

    // Create a centered modal
    let modal_width = 60;
    let modal_height = 6;
    let x = (size.width.saturating_sub(modal_width)) / 2;
    let y = (size.height.saturating_sub(modal_height)) / 2;

    let modal_area = Rect {
        x,
        y,
        width: modal_width,
        height: modal_height,
    };

    // Clear background
    f.render_widget(Clear, modal_area);

    // Background block
    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(" Search Tweets (Esc to close) ")
            .style(Style::default().fg(app.theme.text)),
        modal_area,
    );

    // Input area
    let inner = Rect {
        x: modal_area.x + 1,
        y: modal_area.y + 1,
        width: modal_area.width.saturating_sub(2),
        height: modal_area.height.saturating_sub(2),
    };

    // Show search query with cursor
    let cursor = "█";
    let input_text = format!("{}{}", app.search_query, cursor);
    let search_text = Paragraph::new(input_text).style(Style::default().fg(app.theme.primary));

    f.render_widget(search_text, inner);

    // Show search results count below
    let results_count = if app.show_search {
        format!("Results: {} tweet(s)", app.filtered_indices.len())
    } else {
        "".to_string()
    };

    let results_rect = Rect {
        x: modal_area.x + 1,
        y: modal_area.y + 3,
        width: modal_area.width.saturating_sub(2),
        height: 1,
    };

    if !results_count.is_empty() {
        let results_widget = Paragraph::new(results_count).style(Style::default().fg(Color::Gray));
        f.render_widget(results_widget, results_rect);
    }
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

/// Truncate tweet ID to a maximum length, ensuring UTF-8 safety.
fn truncate_id(id: &str, max_len: usize) -> String {
    let mut result = String::new();
    let mut byte_count = 0;
    let limit = max_len.saturating_sub(1);

    for ch in id.chars() {
        let ch_len = ch.len_utf8();
        if byte_count + ch_len > limit {
            result.push('…');
            break;
        }
        result.push(ch);
        byte_count += ch_len;
    }

    if result.is_empty() && !id.is_empty() {
        String::from("…")
    } else {
        result
    }
}

/// Truncate text to a maximum length, adding ellipsis if needed. UTF-8 safe.
fn truncate_text(text: &str, max_len: usize) -> String {
    let mut result = String::new();
    let mut byte_count = 0;
    let limit = max_len.saturating_sub(1);

    for ch in text.chars() {
        let ch_len = ch.len_utf8();
        if byte_count + ch_len > limit {
            result.push('…');
            break;
        }
        result.push(ch);
        byte_count += ch_len;
    }

    if result.is_empty() && !text.is_empty() {
        String::from("…")
    } else {
        result
    }
}

/// Render the status bar at the bottom showing hotkeys.
fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let status = Line::from(vec![
        Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" Navigate  "),
        Span::styled("←→", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" Page  "),
        Span::styled("Ctrl+F", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" Search  "),
        Span::styled("Ctrl+T", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" Theme  "),
        Span::styled("Ctrl+?", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" Help  "),
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" Quit"),
    ]);

    let status_widget = Paragraph::new(status)
        .style(Style::default().bg(app.theme.border).fg(app.theme.text))
        .alignment(Alignment::Left);

    f.render_widget(status_widget, area);
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
