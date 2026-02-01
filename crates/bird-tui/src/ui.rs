//! UI rendering for bird-tui using ratatui.

use crate::app::{App, CalendarFocus, CalendarRange, Focus};
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

    // If error, show error
    if let Some(error) = &app.error {
        render_error(f, app, error);
        return;
    }

    // If calendar is shown, show calendar view
    if app.show_calendar {
        render_calendar(f, app);
        return;
    }

    // If search is shown, show search modal (renders on top of main content)
    let show_search = app.show_search;
    if show_search {
        render_search_modal(f, app);
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

    // Define column constraints: ID | Author | Score | Headline
    let widths = [
        Constraint::Length(20), // ID (Twitter IDs are 19 digits)
        Constraint::Length(14), // Author
        Constraint::Length(5),  // Score
        Constraint::Fill(1),    // Headline (takes remaining space)
    ];

    // Get tweets to display (filtered or all)
    let display_tweets = app.display_tweets();
    let selected_index = app.selected_index();

    // Build table rows from filtered tweets
    let rows: Vec<Row> = display_tweets
        .iter()
        .enumerate()
        .map(|(i, tweet)| {
            let id_display = truncate_id(&tweet.id, 19);
            let author_display = truncate_text(&tweet.author_username, 14);
            let score_display = format!("{:.1}", tweet.resonance_score.total);
            let headline = truncate_text(&tweet.headline, 100); // Much wider now that it fills space

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
            ])
            .style(style)
        })
        .collect();

    // Create table with header
    let table = Table::new(rows, widths)
        .header(
            Row::new(vec!["ID", "Author", "Score", "Headline"])
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
        if tweet.author_liked_count > 0
            || tweet.author_quoted_count > 0
            || tweet.author_retweeted_count > 0
        {
            let author_with_padding = " ".repeat(label_width.saturating_sub("Engagement:".len()));
            metadata.push(Line::from(vec![
                Span::styled("Engagement:", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(author_with_padding),
                Span::raw(format!(
                    "❤ {} liked  💬 {} quoted  🔄 {} retweeted",
                    tweet.author_liked_count,
                    tweet.author_quoted_count,
                    tweet.author_retweeted_count
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
        let text_content: Vec<Line> = tweet
            .text
            .lines()
            .map(|line| Line::from(Span::raw(line)))
            .collect();

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
    let modal_height = 24;
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
        Line::from("↑/↓        Navigate list (weeks in calendar)"),
        Line::from("←/→        Previous/Next page (days in calendar)"),
        Line::from("Tab        Switch focus (list ↔ detail / calendar ↔ tweets)"),
        Line::from("PgUp/PgDn  Scroll detail panel (months in calendar)"),
        Line::from("c          Toggle calendar (chronological view)"),
        Line::from("Home/g     Jump to today (calendar)"),
        Line::from("m/w/d      Calendar range (month/week/day)"),
        Line::from("Enter      Open selected tweet (calendar list)"),
        Line::from("o          Open tweet in browser"),
        Line::from("Ctrl+F     Search tweets"),
        Line::from("Ctrl+T     Toggle dark/light theme"),
        Line::from("Ctrl+?     Toggle help"),
        Line::from("Esc        Close modals / Quit"),
        Line::from("q          Quit"),
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

/// Render calendar view for chronological navigation.
fn render_calendar(f: &mut Frame, app: &App) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(1)])
        .split(f.area());

    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(main_chunks[0]);

    render_calendar_panel(f, app, content_chunks[0]);
    render_calendar_tweets_panel(f, app, content_chunks[1]);
    render_status_bar(f, app, main_chunks[1]);
}

fn render_calendar_panel(f: &mut Frame, app: &App, area: Rect) {
    let month_names = [
        "",
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let month_str = format!(
        "{} {}",
        month_names[app.calendar_display_month.month() as usize],
        app.calendar_display_month.year()
    );

    let border_style = if app.calendar_focus == CalendarFocus::Calendar {
        Style::default().fg(app.theme.primary)
    } else {
        Style::default().fg(app.theme.border)
    };

    let block = Block::default()
        .title(format!(" Calendar — {} ", month_str))
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(border_style);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let year = app.calendar_display_month.year();
    let month = app.calendar_display_month.month() as u32;
    let today = chrono::Local::now().date_naive();
    let selected_naive = app.calendar_selected_date.and_then(|date| {
        chrono::NaiveDate::from_ymd_opt(date.year(), date.month() as u32, date.day() as u32)
    });

    let mut cal_lines = vec![Line::from("Su  Mo  Tu  We  Th  Fr  Sa")];

    if let Ok(month_enum) = time::Month::try_from(month as u8) {
        if let Ok(first_day) = time::Date::from_calendar_date(year, month_enum, 1) {
            let first_weekday = first_day.weekday();
            let days = days_in_month(year, month);

            let mut current_line = vec![];
            let start_offset = (first_weekday.number_days_from_sunday() as usize) % 7;

            for _ in 0..start_offset {
                current_line.push(Span::raw("    "));
            }

            for day in 1..=days {
                let day_naive = chrono::NaiveDate::from_ymd_opt(year, month, day);
                let is_future = day_naive.map(|d| d > today).unwrap_or(false);
                let is_selected = selected_naive
                    .zip(day_naive)
                    .map(|(s, d)| s == d)
                    .unwrap_or(false);

                let mut style = Style::default();
                if is_future {
                    style = style.fg(Color::DarkGray);
                }
                if is_selected {
                    style = style
                        .bg(app.theme.highlight)
                        .fg(app.theme.text)
                        .add_modifier(Modifier::BOLD);
                }

                current_line.push(Span::styled(format!("{:2}  ", day), style));

                if (start_offset + day as usize).is_multiple_of(7) || day == days {
                    cal_lines.push(Line::from(current_line.clone()));
                    current_line.clear();
                }
            }
        }
    }

    cal_lines.push(Line::from(""));
    cal_lines.push(Line::from(Span::styled(
        format!("Range: {}", format_calendar_range_label(app)),
        Style::default().fg(Color::Gray),
    )));
    cal_lines.push(Line::from(Span::styled(
        format!("Selected: {}", format_calendar_selected_label(app)),
        Style::default().fg(Color::Gray),
    )));

    let calendar_widget = Paragraph::new(cal_lines).style(Style::default());
    f.render_widget(calendar_widget, inner);
}

fn render_calendar_tweets_panel(f: &mut Frame, app: &App, area: Rect) {
    let border_style = if app.calendar_focus == CalendarFocus::Tweets {
        Style::default().fg(app.theme.primary)
    } else {
        Style::default().fg(app.theme.border)
    };

    let range_label = format_calendar_range_label(app);
    let title = format!(" Tweets ({}) — {} ", app.calendar_tweets.len(), range_label);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(border_style);

    if app.calendar_tweets.is_empty() {
        let inner = block.inner(area);
        f.render_widget(block, area);
        let empty = Paragraph::new("No tweets in this range.")
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        f.render_widget(empty, inner);
        return;
    }

    let widths = [
        Constraint::Length(12),
        Constraint::Length(14),
        Constraint::Fill(1),
    ];

    let selected_index = app.calendar_table_state.selected();

    let rows: Vec<Row> = app
        .calendar_tweets
        .iter()
        .enumerate()
        .map(|(i, tweet)| {
            let date_display = format_calendar_list_date(app, tweet);
            let author_display = truncate_text(&tweet.author_username, 14);
            let headline = truncate_text(&tweet.headline, 120);

            let mut style = Style::default();
            if app.calendar_focus == CalendarFocus::Tweets && selected_index == Some(i) {
                style = style
                    .bg(app.theme.highlight)
                    .fg(app.theme.text)
                    .add_modifier(Modifier::BOLD);
            }

            Row::new(vec![
                Cell::from(date_display),
                Cell::from(author_display),
                Cell::from(headline),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(rows, widths)
        .header(
            Row::new(vec!["Date", "Author", "Headline"])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(block);

    f.render_widget(table, area);
}

fn format_calendar_range_label(app: &App) -> String {
    let Some((start, end)) = app.calendar_range_naive() else {
        return "—".to_string();
    };

    match app.calendar_range {
        CalendarRange::Day => start.format("%b %d, %Y").to_string(),
        CalendarRange::Week => format!("{} – {}", start.format("%b %d"), end.format("%b %d")),
        CalendarRange::Month => format!("{} {}", start.format("%B"), start.format("%Y")),
    }
}

fn format_calendar_selected_label(app: &App) -> String {
    let Some(selected) = app.calendar_selected_date else {
        return "—".to_string();
    };
    chrono::NaiveDate::from_ymd_opt(
        selected.year(),
        selected.month() as u32,
        selected.day() as u32,
    )
    .map(|date| date.format("%b %d, %Y").to_string())
    .unwrap_or_else(|| "—".to_string())
}

fn format_calendar_list_date(app: &App, tweet: &crate::app::TweetDisplayData) -> String {
    let Some(dt) = tweet.created_at_local.as_ref() else {
        return "—".to_string();
    };

    match app.calendar_range {
        CalendarRange::Day => dt.format("%b %d %I%p").to_string().to_lowercase(),
        _ => dt.format("%b %d").to_string(),
    }
}

/// Get number of days in a month.
fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Get collection emoji representation.
#[allow(dead_code)]
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
    let status = if app.show_calendar {
        let focus_label = match app.calendar_focus {
            CalendarFocus::Calendar => "Calendar",
            CalendarFocus::Tweets => "Tweets",
        };
        let mut spans = vec![
            Span::styled("Arrows", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Move  "),
            Span::styled("PgUp/PgDn", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Month  "),
            Span::styled("Tab", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!(" Focus({})  ", focus_label)),
            Span::styled("M/W/D", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Range  "),
            Span::styled("Home/g", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Today  "),
        ];

        if app.calendar_focus == CalendarFocus::Tweets {
            spans.push(Span::styled(
                "Enter/o",
                Style::default().add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(" Open  "));
        }

        spans.push(Span::styled(
            "q",
            Style::default().add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" Quit  "));

        spans.push(Span::styled(
            "Esc",
            Style::default().add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" Close"));

        Line::from(spans)
    } else {
        Line::from(vec![
            Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Navigate  "),
            Span::styled("←→", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Page  "),
            Span::styled("c", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Calendar  "),
            Span::styled("Ctrl+F", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Search  "),
            Span::styled("Ctrl+T", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Theme  "),
            Span::styled("Ctrl+?", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Help  "),
            Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Quit"),
        ])
    };

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
