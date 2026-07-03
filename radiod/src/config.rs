use anyhow::Context;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct StationConfig {
    pub name: String,
    pub uri: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub struct DaemonConfig {
    #[serde(default = "default_volume")]
    pub volume: f64,
    pub default_station: Option<String>,
    #[serde(default = "default_poll_interval")]
    pub metadata_poll_interval_secs: u64,
}

fn default_volume() -> f64 {
    0.8
}

fn default_poll_interval() -> u64 {
    30
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Config {
    #[serde(default)]
    pub stations: Vec<StationConfig>,
    pub daemon: DaemonConfig,
}

pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("radiod").join("config.toml"))
}

pub fn data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("radiod"))
}

pub fn load_config() -> anyhow::Result<Config> {
    let path = config_path().context("could not determine XDG config directory")?;
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read config file at {}", path.display()))?;
    let config: Config = toml::from_str(&contents)
        .with_context(|| format!("failed to parse config file at {}", path.display()))?;
    Ok(config)
}

pub fn ensure_data_dir() -> anyhow::Result<PathBuf> {
    let dir = data_dir().context("could not determine XDG data directory")?;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create data directory at {}", dir.display()))?;
    Ok(dir)
}
