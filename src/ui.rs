use crate::config::Config;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use std::time::Duration;
use std::path::PathBuf;

// Layout constants
const FILE_LIST_WIDTH_PERCENT: u16 = 40;
const PREVIEW_WIDTH_PERCENT: u16 = 60;
const STATUS_BAR_HEIGHT: u16 = 3;
const EVENT_POLL_MILLIS: u64 = 16;

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub is_dir: bool,
}

pub struct AppState {
    pub entries: Vec<Entry>,
    pub cursor: usize,
    pub cwd: PathBuf,
    pub message: String,
    pub preview_cache: Option<(PathBuf, String)>, // (path, rendered preview)
    pub preview_image: Option<image::DynamicImage>, // cached decoded image for color rendering
    pub last_terminal_size: Option<(u16, u16)>, // (width, height) to detect resizes
    pub sddm_mode: bool, // whether to also set SDDM wallpaper
    pub config: Config, // application configuration
}

impl AppState {
    pub fn selected(&self) -> Option<&Entry> {
        self.entries.get(self.cursor)
    }

    pub fn selected_file(&self) -> Option<&PathBuf> {
        self.selected().and_then(|e| if !e.is_dir { Some(&e.path) } else { None })
    }
}

pub fn draw(f: &mut Frame, app: &AppState, preview_text: &str) {
    // First split: main area and status bar at bottom
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(STATUS_BAR_HEIGHT)])
        .split(f.size());

    // Second split: left panel (file list) and right panel (preview)
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(FILE_LIST_WIDTH_PERCENT), Constraint::Percentage(PREVIEW_WIDTH_PERCENT)])
        .split(main_layout[0]);

    // left: list
    let items: Vec<_> = app.entries.iter().enumerate().map(|(i, entry)| {
        let name = entry.path.file_name().unwrap_or_default().to_string_lossy();
        let display_name = if entry.is_dir {
            // Check if this is the parent directory entry
            if app.cwd.parent() == Some(&entry.path) {
                "../".to_string()
            } else {
                format!("{}/", name)
            }
        } else {
            name.to_string()
        };
        let mut line = Line::raw(display_name);
        if i == app.cursor { line = line.style(Style::default().add_modifier(Modifier::REVERSED)); }
        ListItem::new(line)
    }).collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(format!(
            " {} ({} items) ", app.cwd.display(), app.entries.len()
        )));

    f.render_widget(list, panels[0]);

    // right: preview
    let preview = Paragraph::new(preview_text.to_string())
        .block(Block::default().borders(Borders::ALL).title(" Preview "));
    f.render_widget(preview, panels[1]);

    // bottom: status message
    let msg = Paragraph::new(app.message.clone())
        .block(Block::default().borders(Borders::TOP).title(" Status "));
    f.render_widget(msg, main_layout[1]);
}

pub fn next_tick() -> bool {
    event::poll(Duration::from_millis(EVENT_POLL_MILLIS)).unwrap_or(false)
}

pub fn read_event() -> Option<KeyEvent> {
    if let Ok(Event::Key(k)) = event::read() {
        return Some(k)
    }
    None
}

pub fn handle_nav(app: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if app.cursor > 0 { app.cursor -= 1; }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.cursor + 1 < app.entries.len() { app.cursor += 1; }
        }
        KeyCode::Char('g') if key.modifiers.is_empty() => app.cursor = 0,
        KeyCode::Char('G') => if !app.entries.is_empty() { app.cursor = app.entries.len()-1; },
        _ => {}
    }
}

