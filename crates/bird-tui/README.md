# bird-tui

A terminal user interface (TUI) for browsing and exploring your synced tweets from the bird database.

## Features

- **Two-Panel Layout**
  - Left panel: Scrollable list of tweets with ID, headline, and collection indicators
  - Right panel: Detailed view of selected tweet with full text and metadata

- **Efficient Navigation**
  - Arrow keys to navigate tweet list
  - Page Up/Down to scroll tweet content
  - Tab to switch focus between panels
  - Left/Right arrows for pagination between pages

- **Collection Indicators**
  - ❤ for likes
  - 🔖 for bookmarks
  - 📝 for your own tweets

- **Resonance Scores**
  - Displays synergistic resonance metric with breakdown
  - Shows interaction counts (replies, retweets)

- **Interactive Help**
  - Press Ctrl+? to toggle help modal with all keyboard shortcuts

## Installation

```bash
cargo build --release -p bird-tui
```

## Usage

### Basic Usage

```bash
# Start the TUI (connects to default database)
./target/release/bird-tui

# Or with custom database
./target/release/bird-tui --db-path ~/.bird/custom.db
```

### Configuration

bird-tui supports the same configuration options as bird-cli:

```bash
# Use memory storage
./target/release/bird-tui --storage memory

# Connect to remote SurrealDB
./target/release/bird-tui --db-url ws://localhost:8000

# With authentication
./target/release/bird-tui --db-url ws://localhost:8000 \
  --db-auth namespace \
  --db-user user123 \
  --db-pass pass123
```

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate tweet list up/down |
| `←` / `→` | Previous/Next page |
| `Tab` | Switch focus (list ↔ detail panel) |
| `Page Up` / `Page Down` | Scroll tweet text |
| `Ctrl+?` | Toggle help modal |
| `q` / `Esc` | Quit |

## Data Prerequisites

Before using bird-tui, ensure you have synced tweets to your database:

```bash
# Sync your likes
bird sync likes

# Sync your bookmarks
bird sync bookmarks

# Or run bird-tui to see available data
bird-tui
```

If no tweets are found, the TUI will display an empty state with instructions.

## Layout

```
┌─ Tweets (Page 1/5) ─────────────────────┐ ┌─ Tweet Text ──────────────────────────────┐
│                                         │ │ Full tweet content displayed here with   │
│ 1234567... My headline      ❤ 🔖       │ │ automatic wrapping for readability.     │
│ 2345678... Another tweet    ❤          │ │                                          │
│ 3456789... Third tweet                 │ │ Author: @handle (Display Name)           │
│ ...                                     │ │ Created: 2024-01-30 15:45                │
│                                         │ │ Resonance: 4.5 (♡:✓ 📌:✓ ↩:2 🔄:1)      │
│                                         │ │                                          │
└─────────────────────────────────────────┘ └──────────────────────────────────────────┘
```

## Performance

- Resonance scores are computed on-demand for displayed tweets
- Interaction pairs are batch-loaded once at startup
- Pagination loads only the current page of tweets
- Scrolling within detail panel is instant (no database queries)

## Troubleshooting

### "No tweets found"
- Have you synced tweets? Run `bird sync likes` or `bird sync bookmarks` first
- Check that tweets are in the correct collections

### Database connection errors
- Ensure your database URL is correct
- For local SurrealDB: use `rocksdb://path/to/db`
- Check your authentication credentials

### Terminal rendering issues
- Ensure your terminal supports 256 colors
- Try resizing the terminal window
- Check that TERM is set correctly (e.g., `xterm-256color`)

## Architecture

- **app.rs**: Application state management
- **ui.rs**: Rendering logic using ratatui
- **events.rs**: Keyboard event handling
- **data.rs**: Data fetching and resonance computation
- **main.rs**: Terminal setup and event loop

## Dependencies

- **ratatui**: TUI rendering framework
- **crossterm**: Terminal manipulation
- **bird-storage**: Tweet storage and querying
- **tokio**: Async runtime
