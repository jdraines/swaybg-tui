# swaybg-tui

A terminal-based wallpaper selector for Wayland environments using swaybg. Navigate directories and browse images with vim-like keybindings. Features pixelly color image previews rendered directly in the terminal using ANSI escape codes and half-block characters.

This is an admittedly vibe-coded tool that I created because I wanted a background switcher that allowed me to actually view images without switching my bg, but felt lo-fi in keeping with the general hyprland aesthetic.

## Features

- **Interactive file and directory browser** - Navigate directories and image files with vim-like keybindings (j/k, g/G, h)
- **Directory navigation** - Enter subdirectories and navigate back to parent directories (via ".." entry or h/Backspace)
- **True color image previews** - High-quality rendering using half-block characters (▀) with RGB colors
- **Live wallpaper application** - Set wallpapers immediately with Enter key
- **Persistent wallpaper** - Save wallpaper selection with Shift+S
- **Responsive layout** - Automatically handles terminal resizes
- **Configurable** - Optional config file for customizing paths and defaults
- **Portable** - Works on any Linux system with Wayland/swaybg

## Installation

### From Source

```bash
git clone https://github.com/yourusername/swaybg-tui
cd swaybg-tui
cargo build --release
sudo cp target/release/swaybg-tui /usr/local/bin/
```

### Requirements

- Rust toolchain (for building from source)
- Wayland compositor
- swaybg (for wallpaper management)
- Terminal with true color support (alacritty, kitty, wezterm, etc.)

## Usage

```bash
# Browse default wallpaper directory (configurable)
swaybg-tui

# Browse specific directory
swaybg-tui /path/to/images

# Show hidden files
swaybg-tui --hidden

# Set SDDM login screen wallpaper (requires sudo)
swaybg-tui --sddm

# See all options
swaybg-tui --help
```

## Configuration

Create an optional config file at `~/.config/swaybg-tui/config.toml`:

```toml
# Default directory to browse for wallpapers
default_wallpaper_dir = "/home/user/Pictures/Wallpapers"

# Path to omadora/swaybg background symlink
omadora_background_path = "/home/user/.config/omadora/current/background"

# SDDM theme directory (for --sddm flag)
sddm_theme_dir = "/usr/share/sddm/themes/simple_sddm_2"
sddm_theme_conf = "/usr/share/sddm/themes/simple_sddm_2/theme.conf"
```

If no config file exists, sensible defaults are used.

### Keybindings

- `↑`/`k` - Move up
- `↓`/`j` - Move down
- `g` - Jump to first entry
- `G` - Jump to last entry
- `Enter` - Enter directory / Apply wallpaper (temporary)
- `h`/`Backspace` - Go to parent directory (or select ".." entry and press Enter)
- `Shift+S` - Apply and persist wallpaper
- `q`/`Esc` - Quit

**Note:** A ".." entry appears at the top of each directory listing (except at root) for easy upward navigation.

## How It Works

### Architecture

The application consists of four main modules:

**`main.rs`** - Entry point and main loop
- Command-line argument parsing
- Directory and image file discovery and filtering
- ".." entry generation for parent directory navigation
- Terminal initialization and raw mode setup
- Main event loop with resize detection
- Directory navigation logic (Enter to descend, h/Backspace to ascend)
- Image loading, caching, and rendering coordination

**`ui.rs`** - User interface layout
- `Entry` struct to distinguish files from directories
- Split-pane layout (40% file list, 60% preview)
- File list rendering with cursor highlighting (directories shown with "/" suffix, parent shown as "../")
- Preview pane and status bar
- Navigation input handling

**`wall.rs`** - Wallpaper management
- Symlink creation at `~/.config/omadora/current/background`
- swaybg process management (kill and restart)
- Integration with uwsm/omadora system

**`Cargo.toml`** - Dependencies and features
- `ascii` feature flag enables image previews (default on)
- Core dependencies: ratatui, crossterm, image, clap

### Image Rendering

The application uses a two-stage rendering approach:

1. **Ratatui frame** - Draws the UI structure (borders, file list, text)
2. **Direct terminal writes** - Renders the color image after the ratatui frame

Images are rendered using half-block characters (▀) where:
- **Foreground color** = top pixel RGB
- **Background color** = bottom pixel RGB
- Effectively doubles vertical resolution (2 pixels per terminal cell)

Example rendering code:
```rust
write!(stdout, "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m▀\x1b[0m",
       top_r, top_g, top_b,    // Foreground (top pixel)
       bot_r, bot_g, bot_b)    // Background (bottom pixel)
```

### Performance Optimizations

**Preview caching** - Images are decoded and resized only when:
- A different image is selected
- Terminal is resized
- Cache is empty

Cached data stored in `AppState`:
- `preview_cache: Option<(PathBuf, String)>` - Tracks which image is cached
- `preview_image: Option<DynamicImage>` - Decoded and resized image data
- `last_terminal_size: Option<(u16, u16)>` - Detects resize events

### Terminal Resize Handling

On resize detection:
1. Clear ratatui internal buffer (`term.clear()`)
2. Clear physical terminal screen (`Clear(ClearType::All)`)
3. Invalidate image cache (force reload at new dimensions)
4. Flush output to ensure clean slate

This is critical because direct ANSI writes bypass ratatui's rendering system.

## Wallpaper System Integration

This application integrates with **omadora's symlink-based wallpaper system**:

1. **Symlink management** - Creates/updates `~/.config/omadora/current/background` to point at selected image
2. **swaybg control** - Kills existing swaybg process and launches new one with updated symlink
3. **uwsm integration** - Uses `uwsm app --` to properly manage swaybg as a Wayland app

The symlink approach ensures persistence across sessions - omadora startup scripts read the symlink to restore the wallpaper.

## Building

```bash
# Build with image preview support (default)
cargo build --release

# Build without image previews (text-only mode)
cargo build --release --no-default-features
```

## Supported Image Formats

- JPEG (.jpg, .jpeg)
- PNG (.png)
- GIF (.gif)
- BMP (.bmp)
- TIFF (.tiff, .tif)
- WebP (.webp)

## Advanced Usage

### Integration with Other Wallpaper Systems

While designed for omadora/swaybg, you can customize the wallpaper backend by modifying `src/wall.rs` to work with:
- hyprpaper
- swww
- wpaperd
- Or any other Wayland wallpaper daemon

### SDDM Login Screen Wallpaper

Use the `--sddm` flag to also set your SDDM login screen wallpaper:

```bash
swaybg-tui --sddm
```

This requires:
- sudo access (passwordless sudo recommended for smoother experience)
- SDDM theme installed at configured path

## Technical Notes

### Why Direct Terminal Writes?

Ratatui's `Paragraph` widget escapes ANSI codes, rendering them as literal text. To display true color images, we bypass ratatui by writing ANSI escape sequences directly to stdout after ratatui draws its frame. This requires:
- Using `cursor::MoveTo` to position each line
- Manual flush with `stdout.flush()`
- Clearing physical terminal on resize (not just ratatui buffer)

### Aspect Ratio Preservation

Images are resized with `image::imageops::resize()` using Lanczos3 filtering:
```rust
let scale = f32::min(max_width as f32 / w as f32, max_height as f32 / h as f32);
let nw = (w as f32 * scale) as u32;
let nh = (h as f32 * scale) as u32;
```

Images are then centered with padding calculations to prevent stretching.

### Borrow Checker Considerations

The main loop carefully orders operations to avoid borrow conflicts:
1. Detect resize (reads `app.last_terminal_size`)
2. Mutate app state if needed (clear caches)
3. Borrow from app for rendering (call `app.selected()`)

This ordering ensures no overlapping mutable and immutable borrows.

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.
