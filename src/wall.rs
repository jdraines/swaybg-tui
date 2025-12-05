use crate::config::Config;
use anyhow::{Context, Result, bail};
use std::{fs, path::{Path, PathBuf}, process::Command, os::unix::fs as unix_fs};

/// Get the current user's username
fn get_current_username() -> Result<String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .context("Could not determine current username from USER or USERNAME environment variables")
}

/// Set the SDDM login screen wallpaper
///
/// Copies the image to the SDDM theme directory and updates theme.conf.
/// Requires sudo access as SDDM theme files are in /usr/share.
///
/// # Errors
/// Returns an error if:
/// - Sudo access is not available
/// - Image copy fails
/// - theme.conf cannot be read or written
pub fn set_sddm_wallpaper(img: &Path, config: &Config) -> Result<()> {
    let theme_dir = &config.sddm_theme_dir;
    let theme_conf = &config.sddm_theme_conf;

    // Get the image filename
    let img_filename = img.file_name()
        .context("Failed to get image filename")?
        .to_str()
        .context("Filename is not valid UTF-8")?;

    let backgrounds_dir = PathBuf::from(theme_dir).join("Backgrounds");
    let dest_path = backgrounds_dir.join(img_filename);

    // Check if we have sudo access (we'll need it for copying and editing)
    let sudo_check = Command::new("sudo")
        .args(["-n", "true"])
        .output();

    let needs_password = match sudo_check {
        Ok(output) => !output.status.success(),
        Err(_) => true,
    };

    if needs_password {
        bail!("SDDM wallpaper setting requires sudo access. Run with sudo or configure passwordless sudo.");
    }

    // Copy the image to the theme's Backgrounds directory
    let copy_status = Command::new("sudo")
        .args([
            "cp",
            img.to_str().unwrap_or(""),
            dest_path.to_str().unwrap_or(""),
        ])
        .status()
        .context("Failed to copy image to SDDM theme directory")?;

    if !copy_status.success() {
        bail!("Failed to copy image to SDDM theme directory");
    }

    // Set ownership to current user
    let username = get_current_username()?;
    let ownership = format!("{}:{}", username, username);
    let chown_status = Command::new("sudo")
        .args([
            "chown",
            &ownership,
            dest_path.to_str().unwrap_or(""),
        ])
        .status()
        .context("Failed to set ownership on the file")?;

    if !chown_status.success() {
        bail!("Failed to set ownership on the image in the SDDM theme directory");
    }

    // Read the current theme.conf
    let conf_content = fs::read_to_string(theme_conf)
        .with_context(|| format!("Failed to read theme.conf at {} - ensure SDDM theme is installed", theme_conf))?;

    // Parse and update the background setting
    let mut new_lines = Vec::new();
    let mut found_background = false;

    for line in conf_content.lines() {
        if line.trim_start().starts_with("background=") || line.trim_start().starts_with("Background=") {
            // Update the background line
            new_lines.push(format!("background=Backgrounds/{}", img_filename));
            found_background = true;
        } else {
            new_lines.push(line.to_string());
        }
    }

    // If no background line was found, add it
    if !found_background {
        new_lines.push(format!("background=Backgrounds/{}", img_filename));
    }

    let new_conf = new_lines.join("\n");

    // Write the updated config using sudo
    let write_status = Command::new("sudo")
        .args(["tee", theme_conf])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(new_conf.as_bytes())?;
            }
            child.wait()
        })
        .context("Failed to write theme.conf")?;

    if !write_status.success() {
        bail!("Failed to write updated theme.conf");
    }

    Ok(())
}

/// Set the desktop wallpaper using swaybg and omadora's symlink system
///
/// Creates/updates the omadora background symlink and restarts swaybg to apply the change.
/// The symlink provides persistence across sessions when omadora startup scripts run.
///
/// # Arguments
/// * `img` - Path to the wallpaper image file
/// * `persist` - Currently unused (persistence is automatic via symlink)
/// * `config` - Application configuration with omadora paths
///
/// # Errors
/// Returns an error if:
/// - Symlink creation fails
/// - swaybg launch fails
pub fn set_wallpaper(img: &Path, persist: bool, config: &Config) -> Result<()> {
    // Omadora wallpaper system: uses a symlink and swaybg
    let background_link = PathBuf::from(&config.omadora_background_path);

    // Create the symlink (or update existing one)
    if background_link.exists() || background_link.read_link().is_ok() {
        fs::remove_file(&background_link)?;
    }

    unix_fs::symlink(img, &background_link)
        .with_context(|| format!("Failed to create symlink at {}", background_link.display()))?;

    // Restart swaybg with the new background
    // Kill existing swaybg
    let _ = Command::new("pkill")
        .args(["-x", "swaybg"])
        .output();

    // Launch new swaybg with the symlink
    Command::new("setsid")
        .args([
            "uwsm", "app", "--",
            "swaybg",
            "-i", background_link.to_str().unwrap_or(""),
            "-m", "fill"
        ])
        .spawn()
        .context("Failed to launch swaybg")?;

    if persist {
        // For omadora, persistence is inherent in the symlink system
        // The symlink at ~/.config/omadora/current/background persists across sessions
        // No additional action needed
    }

    Ok(())
}

