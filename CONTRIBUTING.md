# Contributing to swaybg-tui

Thanks for your interest in contributing! This document provides guidelines for contributing to the project.

## Development Setup

1. Clone the repository
2. Install Rust toolchain (if not already installed)
3. Run tests: `cargo test`
4. Build: `cargo build`
5. Run: `cargo run -- /path/to/images`

## Code Style

- Follow standard Rust formatting: `cargo fmt`
- Run clippy for lints: `cargo clippy`
- Add tests for new functionality
- Document public functions with rustdoc comments

## Making Changes

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/your-feature-name`
3. Make your changes
4. Run tests: `cargo test`
5. Commit with clear messages
6. Push to your fork
7. Open a pull request

## What to Contribute

### Good First Issues
- Add support for additional image formats
- Improve error messages
- Add keyboard shortcuts
- Documentation improvements

### Larger Features
- Integration with other Wayland wallpaper daemons (hyprpaper, swww, etc.)
- Additional configuration options
- Performance optimizations
- Image effects/filters

### Testing
- Unit tests for core functions
- Integration tests for file operations
- Manual testing on different terminals/compositors

## Pull Request Guidelines

- Keep changes focused and atomic
- Include tests for new functionality
- Update documentation as needed
- Ensure `cargo test` and `cargo clippy` pass
- Describe what the PR does and why

## Questions?

Feel free to open an issue for discussion before starting work on larger features.

## Code of Conduct

Be respectful and constructive in all interactions. We're all here to build something useful together.
