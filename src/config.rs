use anyhow::{Context, Result};
use directories::UserDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Default directory to browse for wallpapers
    #[serde(default = "default_wallpaper_dir")]
    pub default_wallpaper_dir: String,

    /// Path to omadora background symlink
    #[serde(default = "default_omadora_background_path")]
    pub omadora_background_path: String,

    /// Path to SDDM theme directory
    #[serde(default = "default_sddm_theme_dir")]
    pub sddm_theme_dir: String,

    /// Path to SDDM theme.conf file
    #[serde(default = "default_sddm_theme_conf")]
    pub sddm_theme_conf: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_wallpaper_dir: default_wallpaper_dir(),
            omadora_background_path: default_omadora_background_path(),
            sddm_theme_dir: default_sddm_theme_dir(),
            sddm_theme_conf: default_sddm_theme_conf(),
        }
    }
}

fn default_wallpaper_dir() -> String {
    if let Some(home) = UserDirs::new().map(|ud| ud.home_dir().to_path_buf()) {
        home.join(".config/omadora/current/theme/backgrounds")
            .to_string_lossy()
            .to_string()
    } else {
        "~/Pictures/Wallpapers".to_string()
    }
}

fn default_omadora_background_path() -> String {
    if let Some(home) = UserDirs::new().map(|ud| ud.home_dir().to_path_buf()) {
        home.join(".config/omadora/current/background")
            .to_string_lossy()
            .to_string()
    } else {
        "~/.config/omadora/current/background".to_string()
    }
}

fn default_sddm_theme_dir() -> String {
    "/usr/share/sddm/themes/simple_sddm_2".to_string()
}

fn default_sddm_theme_conf() -> String {
    "/usr/share/sddm/themes/simple_sddm_2/theme.conf".to_string()
}

impl Config {
    /// Get the configuration file path
    pub fn config_path() -> Option<PathBuf> {
        UserDirs::new().map(|ud| {
            ud.home_dir()
                .join(".config")
                .join("swaybg-tui")
                .join("config.toml")
        })
    }

    /// Load configuration from file, or use defaults if file doesn't exist
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path().context("Could not determine config directory")?;

        if config_path.exists() {
            let contents = fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read config file at {}", config_path.display()))?;

            toml::from_str(&contents)
                .with_context(|| format!("Failed to parse config file at {}", config_path.display()))
        } else {
            // No config file exists, use defaults
            Ok(Self::default())
        }
    }

    /// Save the current configuration to file
    #[allow(dead_code)]
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path().context("Could not determine config directory")?;

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory at {}", parent.display()))?;
        }

        let contents = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;

        fs::write(&config_path, contents)
            .with_context(|| format!("Failed to write config file to {}", config_path.display()))?;

        Ok(())
    }
}
