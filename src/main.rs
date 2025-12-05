mod config;
mod ui;
mod wall;

use anyhow::{Context, Result};
use clap::Parser;
use config::Config;
use crossterm::{
    execute, cursor,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, enable_raw_mode, disable_raw_mode, Clear, ClearType},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{fs, io::{self, Write}, path::{Path, PathBuf}};
use ui::{AppState, Entry, draw, next_tick, read_event};
use wall::{set_wallpaper, set_sddm_wallpaper};

// Layout constants
const PREVIEW_PANE_WIDTH_PERCENT: u16 = 60;
const FILE_LIST_WIDTH_PERCENT: u16 = 40;
const STATUS_BAR_HEIGHT: u16 = 3;
const PREVIEW_BORDER_PADDING: u16 = 4;
const PREVIEW_TEXT_PADDING: u16 = 6;

#[derive(Parser, Debug)]
struct Args {
    /// Directory to browse (defaults to configured wallpaper directory)
    dir: Option<String>,
    /// Show hidden files
    #[arg(long)]
    hidden: bool,
    /// Also set SDDM wallpaper (requires sudo for theme.conf modification)
    #[arg(long)]
    sddm: bool,
}

/// Expand tilde (~) in path strings to the user's home directory
fn expand_tilde(p: &str) -> PathBuf {
    if p == "~" {
        if let Some(home) = directories::UserDirs::new() {
            return home.home_dir().to_path_buf();
        }
    } else if let Some(stripped) = p.strip_prefix("~/") {
        if let Some(home) = directories::UserDirs::new() {
            return home.home_dir().join(stripped);
        }
    }
    PathBuf::from(p)
}

/// Check if a file path has a supported image extension
fn is_image(p: &Path) -> bool {
    if let Some(ext) = p.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
        matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "tif" | "webp")
    } else {
        false
    }
}

/// Collect directory entries (directories and image files) sorted and with parent directory
///
/// Returns entries in order: parent (..), subdirectories (sorted), image files (sorted)
fn collect_entries(dir: &Path, include_hidden: bool) -> Result<Vec<Entry>> {
    let mut dirs = Vec::new();
    let mut files = Vec::new();

    for entry in fs::read_dir(dir).with_context(|| format!("read_dir {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !include_hidden && name.starts_with('.') { continue; }

        if path.is_dir() {
            dirs.push(Entry { path, is_dir: true });
        } else if path.is_file() && is_image(&path) {
            files.push(Entry { path, is_dir: false });
        }
    }

    // Sort directories and files separately for predictable ordering
    dirs.sort_by(|a, b| a.path.cmp(&b.path));
    files.sort_by(|a, b| a.path.cmp(&b.path));

    // Build result with parent (..) first, then dirs, then files
    let mut result = Vec::new();
    if let Some(parent) = dir.parent() {
        result.push(Entry {
            path: parent.to_path_buf(),
            is_dir: true,
        });
    }

    result.extend(dirs);
    result.extend(files);
    Ok(result)
}

/// Build text preview (just the header and instructions)
fn build_preview(p: Option<&Path>) -> String {
    let Some(img) = p else { return "Select an image (↑/↓ or j/k), Enter=apply, Shift+S=persist, q/Esc=quit".into(); };

    #[cfg(feature = "ascii")]
    {
        format!("{}\n\nEnter=apply  Shift+S=persist  q/Esc=quit", img.display())
    }

    #[cfg(not(feature = "ascii"))]
    {
        format!(
            "{}\n\n(No preview available)\n\nRebuild with --features ascii for image previews.\n\nEnter=apply  Shift+S=persist  q/Esc=quit",
            img.display()
        )
    }
}

/// Load and scale image for preview
///
/// Decodes the image file and resizes it to fit within the specified dimensions
/// while preserving aspect ratio. Uses Lanczos3 filtering for high-quality downscaling.
#[cfg(feature = "ascii")]
fn load_preview_image(img_path: &Path, max_width: u32, max_height: u32) -> Option<image::DynamicImage> {
    use image::{ImageReader, GenericImageView, DynamicImage};

    let reader = ImageReader::open(img_path).ok()?;
    let decoded_img = reader.decode().ok()?;

    let (w, h) = decoded_img.dimensions();

    // Calculate uniform scale to fit within bounds while preserving aspect ratio
    // Use the smaller of width_scale and height_scale to ensure both dimensions fit
    let scale = f32::min(max_width as f32 / w.max(1) as f32, max_height as f32 / h.max(1) as f32);
    let nw = (w as f32 * scale).max(1.0) as u32;
    let nh = (h as f32 * scale).max(1.0) as u32;

    // Resize using Lanczos3 filter for high-quality downscaling
    let resized = image::imageops::resize(&decoded_img, nw, nh, image::imageops::FilterType::Lanczos3);
    Some(DynamicImage::ImageRgba8(resized))
}

/// Render image directly to terminal using half-block characters with true color
///
/// Uses the half-block character (▀) to display two vertically stacked pixels per terminal cell,
/// effectively doubling vertical resolution. This bypasses ratatui by writing ANSI escape codes
/// directly to stdout, as ratatui's Paragraph widget would escape the color codes.
///
/// # Half-block rendering technique
/// - Each terminal cell shows the ▀ character (upper half block)
/// - Foreground color (top pixel) is set with ANSI code: \x1b[38;2;R;G;Bm
/// - Background color (bottom pixel) is set with ANSI code: \x1b[48;2;R;G;Bm
/// - This allows 2 image pixels per terminal row
#[cfg(feature = "ascii")]
fn render_image_to_terminal(
    img: &image::DynamicImage,
    start_x: u16,
    start_y: u16,
    max_width: u16,
    max_height: u16,
) -> io::Result<()> {
    use image::GenericImageView;

    let (w, h) = img.dimensions();

    // Calculate centering offsets
    let img_rows = (h + 1) / 2; // Two vertical pixels per terminal row
    let pad_top = (max_height.saturating_sub(img_rows as u16)) / 2;
    let pad_left = (max_width.saturating_sub(w as u16)) / 2;

    let mut stdout = io::stdout();

    // Render row by row, processing 2 vertical pixels per iteration
    for y in (0..h).step_by(2) {
        let row = start_y + pad_top + (y / 2) as u16;
        let col = start_x + pad_left;

        // Position cursor at start of this row
        execute!(stdout, cursor::MoveTo(col, row))?;

        for x in 0..w {
            // Top pixel becomes foreground color
            let top_px = img.get_pixel(x, y);
            let top_r = top_px[0];
            let top_g = top_px[1];
            let top_b = top_px[2];

            // Bottom pixel becomes background color (duplicate top if at image bottom)
            let (bot_r, bot_g, bot_b) = if y + 1 < h {
                let bot_px = img.get_pixel(x, y + 1);
                (bot_px[0], bot_px[1], bot_px[2])
            } else {
                (top_r, top_g, top_b)
            };

            // Write half-block with RGB colors: ▀ with foreground=top, background=bottom
            write!(
                stdout,
                "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m▀\x1b[0m",
                top_r, top_g, top_b,
                bot_r, bot_g, bot_b
            )?;
        }
    }

    stdout.flush()?;
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let config = Config::load().context("Failed to load configuration")?;

    // Determine directory to browse: CLI arg takes precedence, then config
    let dir_str = args.dir.as_ref().map(|s| s.as_str()).unwrap_or(&config.default_wallpaper_dir);
    let dir = expand_tilde(dir_str);
    let entries = collect_entries(&dir, args.hidden)?;

    if entries.is_empty() {
        eprintln!("No entries found in {}", dir.display());
        eprintln!("Supported image formats: jpg, jpeg, png, gif, bmp, tiff, tif, webp");
        std::process::exit(1);
    }

    let mut app = AppState {
        entries,
        cursor: 0,
        cwd: dir,
        message: String::new(),
        preview_cache: None,
        preview_image: None,
        last_terminal_size: None,
        sddm_mode: args.sddm,
        config,
    };

    // TUI init
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend)?;
    term.clear()?;

    // Main loop
    let res = (|| -> Result<()> {
        loop {
            // Use cached preview if available, otherwise regenerate
            let area = term.size()?;

            // Detect terminal resize and handle it first
            let terminal_resized = match app.last_terminal_size {
                Some((last_w, last_h)) => last_w != area.width || last_h != area.height,
                None => true,
            };

            if terminal_resized {
                app.last_terminal_size = Some((area.width, area.height));
                // Invalidate image cache on resize
                app.preview_image = None;
                app.preview_cache = None;
                // Clear both the terminal buffer and the actual screen to remove old image artifacts
                term.clear()?;
                let mut stdout = io::stdout();
                execute!(stdout, Clear(ClearType::All))?;
                stdout.flush()?;
            }

            // Now we can borrow from app without conflicts
            let selected_path = app.selected_file().map(|p| p.as_path());

            // Build simple text preview
            let preview_txt = build_preview(selected_path);

            // Calculate preview pane dimensions
            let preview_width = (area.width * PREVIEW_PANE_WIDTH_PERCENT) / 100;
            let preview_height = area.height.saturating_sub(STATUS_BAR_HEIGHT);

            // Load/cache the preview image
            #[cfg(feature = "ascii")]
            if let Some(sel) = selected_path {
                let needs_reload = match &app.preview_cache {
                    Some((cached_path, _)) => sel != cached_path.as_path(),
                    None => true,
                };

                if needs_reload {
                    // Calculate available space for image (account for borders and padding)
                    let max_w = preview_width.saturating_sub(PREVIEW_BORDER_PADDING) as u32;
                    let max_h = (preview_height.saturating_sub(PREVIEW_TEXT_PADDING) as u32).saturating_mul(2); // 2x for half-blocks

                    let new_image = load_preview_image(sel, max_w, max_h);
                    let sel_path = sel.to_path_buf();

                    app.preview_image = new_image;
                    app.preview_cache = Some((sel_path, String::new()));
                }
            }

            term.draw(|f| {
                draw(f, &app, &preview_txt);
            })?;

            // Render the color image directly to terminal after ratatui frame
            #[cfg(feature = "ascii")]
            if let Some(ref img) = app.preview_image {
                // Preview pane starts after the file list pane
                let preview_start_x = (area.width * FILE_LIST_WIDTH_PERCENT) / 100;
                let preview_start_y = 1; // Account for top border

                // Available space inside the preview pane (excluding borders)
                let avail_w = preview_width.saturating_sub(2);
                let avail_h = preview_height.saturating_sub(5); // Account for borders and text

                let _ = render_image_to_terminal(img, preview_start_x + 1, preview_start_y + 3, avail_w, avail_h);
            }

            if next_tick() {
                if let Some(key) = read_event() {
                    use crossterm::event::{KeyCode::*, KeyModifiers};
                    match key.code {
                        Char('q') | Esc => break,
                        Enter => {
                            if let Some(entry) = app.selected() {
                                if entry.is_dir {
                                    // Navigate into directory
                                    match collect_entries(&entry.path, args.hidden) {
                                        Ok(new_entries) => {
                                            app.cwd = entry.path.clone();
                                            app.entries = new_entries;
                                            app.cursor = 0;
                                            app.preview_cache = None;
                                            app.preview_image = None;
                                            app.message = String::new();
                                        }
                                        Err(e) => app.message = format!("Error reading directory: {e:#}"),
                                    }
                                } else {
                                    // Apply wallpaper
                                    let sel = &entry.path;
                                    if app.sddm_mode {
                                        match set_sddm_wallpaper(sel, &app.config) {
                                            Ok(_) => app.message = format!("SDDM wallpaper set to {} - run 'sudo systemctl restart sddm' to apply", sel.display()),
                                            Err(e) => app.message = format!("Error: {e:#}"),
                                        }
                                    } else {
                                        match set_wallpaper(sel, false, &app.config) {
                                            Ok(_) => app.message = format!("Applied {}", sel.display()),
                                            Err(e) => app.message = format!("Error: {e:#}"),
                                        }
                                    }
                                }
                            }
                        }
                        Char('S') if key.modifiers.contains(KeyModifiers::SHIFT) => {
                            // Shift+S for persist (more reliable than Shift+Enter across terminals)
                            if let Some(sel) = app.selected_file() {
                                if app.sddm_mode {
                                    match set_sddm_wallpaper(sel, &app.config) {
                                        Ok(_) => app.message = format!("SDDM wallpaper set to {} - run 'sudo systemctl restart sddm' to apply", sel.display()),
                                        Err(e) => app.message = format!("Error: {e:#}"),
                                    }
                                } else {
                                    match set_wallpaper(sel, true, &app.config) {
                                        Ok(_) => app.message = format!("Applied & persisted {}", sel.display()),
                                        Err(e) => app.message = format!("Error: {e:#}"),
                                    }
                                }
                            }
                        }
                        Char('h') | Backspace => {
                            // Go to parent directory
                            if let Some(parent) = app.cwd.parent() {
                                match collect_entries(parent, args.hidden) {
                                    Ok(new_entries) => {
                                        app.cwd = parent.to_path_buf();
                                        app.entries = new_entries;
                                        app.cursor = 0;
                                        app.preview_cache = None;
                                        app.preview_image = None;
                                        app.message = String::new();
                                    }
                                    Err(e) => app.message = format!("Error reading parent directory: {e:#}"),
                                }
                            } else {
                                app.message = "Already at root directory".to_string();
                            }
                        }
                        _ => {
                            ui::handle_nav(&mut app, key);
                        }
                    }
                }
            }
        }
        Ok(())
    })();

    // teardown
    disable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, LeaveAlternateScreen)?;
    if let Err(e) = res { eprintln!("{e:#}"); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn test_expand_tilde_bare() {
        let expanded = expand_tilde("~");
        assert!(expanded.is_absolute());
        assert!(expanded.to_string_lossy().len() > 1);
    }

    #[test]
    fn test_expand_tilde_with_path() {
        let expanded = expand_tilde("~/.config");
        assert!(expanded.is_absolute());
        assert!(expanded.to_string_lossy().contains(".config"));
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        let path = "/absolute/path";
        let expanded = expand_tilde(path);
        assert_eq!(expanded, PathBuf::from(path));
    }

    #[test]
    fn test_is_image_supported_formats() {
        assert!(is_image(Path::new("test.jpg")));
        assert!(is_image(Path::new("test.jpeg")));
        assert!(is_image(Path::new("test.png")));
        assert!(is_image(Path::new("test.gif")));
        assert!(is_image(Path::new("test.bmp")));
        assert!(is_image(Path::new("test.webp")));
        assert!(is_image(Path::new("test.tiff")));
        assert!(is_image(Path::new("test.tif")));
    }

    #[test]
    fn test_is_image_unsupported_formats() {
        assert!(!is_image(Path::new("test.txt")));
        assert!(!is_image(Path::new("test.mp4")));
        assert!(!is_image(Path::new("test")));
    }

    #[test]
    fn test_is_image_case_insensitive() {
        assert!(is_image(Path::new("test.JPG")));
        assert!(is_image(Path::new("test.PNG")));
        assert!(is_image(Path::new("test.GiF")));
    }

    #[test]
    fn test_config_default_values() {
        let config = Config::default();
        assert!(!config.default_wallpaper_dir.is_empty());
        assert!(!config.omadora_background_path.is_empty());
        assert!(!config.sddm_theme_dir.is_empty());
    }

    #[test]
    fn test_collect_entries_sorts_correctly() {
        // Create a temporary test directory structure
        let temp_dir = std::env::temp_dir().join("swaybg_tui_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Create test files
        fs::write(temp_dir.join("zebra.png"), b"").unwrap();
        fs::write(temp_dir.join("alpha.jpg"), b"").unwrap();
        fs::create_dir(temp_dir.join("dir_beta")).unwrap();
        fs::create_dir(temp_dir.join("dir_alpha")).unwrap();

        let entries = collect_entries(&temp_dir, false).unwrap();

        // Find where directories end and files begin
        let first_file_idx = entries.iter().position(|e| !e.is_dir).unwrap_or(entries.len());

        // Check directories are before files
        assert!(entries[..first_file_idx].iter().all(|e| e.is_dir));
        assert!(entries[first_file_idx..].iter().all(|e| !e.is_dir));

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }
}

