//! Keyboard event handling for bird-tui.

use crate::app::App;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

/// Handle keyboard events and update app state.
pub async fn handle_events(app: &mut App) -> Result<bool, String> {
    // Check if an event is available (with timeout)
    if event::poll(Duration::from_millis(200))
        .map_err(|e| format!("Failed to poll events: {}", e))?
    {
        match event::read().map_err(|e| format!("Failed to read event: {}", e))? {
            Event::Key(key) => handle_key_event(app, key),
            Event::Resize(_, _) => Ok(false), // Continue running
            _ => Ok(false),                   // Ignore other events
        }
    } else {
        Ok(false) // Continue running
    }
}

/// Handle a key event and return whether to exit the app.
fn handle_key_event(app: &mut App, key: KeyEvent) -> Result<bool, String> {
    // If help is shown, only allow Ctrl+? or Esc to close it
    if app.show_help {
        match key.code {
            KeyCode::Char('?') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.toggle_help();
            }
            KeyCode::Esc => {
                app.toggle_help();
            }
            _ => {}
        }
        return Ok(false);
    }

    // If search is shown, handle search input
    if app.show_search {
        match key.code {
            KeyCode::Esc => {
                app.toggle_search();
                return Ok(false);
            }
            KeyCode::Enter => {
                // Confirm search (keep modal open for more typing)
                return Ok(false);
            }
            KeyCode::Backspace => {
                app.search_query.pop();
                app.mark_search_modified();
                return Ok(false);
            }
            KeyCode::Char(c) => {
                app.search_query.push(c);
                app.mark_search_modified();
                return Ok(false);
            }
            _ => {
                return Ok(false);
            }
        }
    }

    // Handle regular navigation
    match key.code {
        // Quit
        KeyCode::Char('q') | KeyCode::Esc => Ok(true),

        // Help
        KeyCode::Char('?') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_help();
            Ok(false)
        }

        // Theme toggle
        KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.theme.toggle();
            Ok(false)
        }

        // Search toggle
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_search();
            Ok(false)
        }

        // List navigation
        KeyCode::Up => {
            app.select_prev();
            Ok(false)
        }
        KeyCode::Down => {
            app.select_next();
            Ok(false)
        }

        // Pagination
        KeyCode::Left => {
            if app.current_page > 0 {
                app.current_page -= 1;
                app.table_state.select(Some(0));
                app.detail_scroll_offset = 0;
                // TODO: reload tweets for this page
            }
            Ok(false)
        }
        KeyCode::Right => {
            // TODO: check if there are more pages
            app.current_page += 1;
            app.table_state.select(Some(0));
            app.detail_scroll_offset = 0;
            // TODO: reload tweets for this page
            Ok(false)
        }

        // Focus switching
        KeyCode::Tab => {
            app.switch_focus();
            Ok(false)
        }

        // Detail panel scrolling
        KeyCode::PageUp => {
            if app.detail_scroll_offset > 0 {
                app.detail_scroll_offset = app.detail_scroll_offset.saturating_sub(10);
            }
            Ok(false)
        }
        KeyCode::PageDown => {
            app.detail_scroll_offset += 10;
            Ok(false)
        }

        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn create_test_app() -> App {
        use bird_storage::MemoryStorage;
        let storage = Arc::new(MemoryStorage::new());
        App::new(storage, "test_user".to_string())
    }

    #[test]
    fn test_quit_key() {
        let mut app = create_test_app();
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let result = handle_key_event(&mut app, key);
        assert!(result.unwrap()); // Should quit
    }

    #[test]
    fn test_help_toggle() {
        let mut app = create_test_app();
        let key = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::CONTROL);
        let _ = handle_key_event(&mut app, key);
        assert!(app.show_help); // Help should be shown
    }

    #[test]
    fn test_focus_switch() {
        let mut app = create_test_app();
        use crate::app::Focus;
        assert_eq!(app.focus, Focus::List);
        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        let _ = handle_key_event(&mut app, key);
        assert_eq!(app.focus, Focus::Detail);
    }

    #[test]
    fn test_list_navigation() {
        let mut app = create_test_app();
        assert_eq!(app.selected_index(), 0);
        let key_down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let _ = handle_key_event(&mut app, key_down);
        // With empty tweets, index shouldn't change
        assert_eq!(app.selected_index(), 0);
    }
}
