use std::{fs::OpenOptions, io::Read, path::PathBuf};

use eyre::{Context, OptionExt, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub data_path: PathBuf,
    pub log_path: PathBuf,
}

pub fn get_default_app_data_path() -> PathBuf {
    let dirs = directories::ProjectDirs::from("com", "Tweoss", "Task List")
        .clone()
        .unwrap();
    let data_dir = dirs.data_dir();
    data_dir.to_path_buf()
}

impl Config {
    pub fn load() -> Result<Self, (Self, eyre::Report)> {
        let mut out = Self {
            data_path: get_default_app_data_path().join("tasks"),
            log_path: get_default_app_data_path().join("logs"),
        };
        match out.read_from_file() {
            Ok(_) => Ok(out),
            Err(e) => Err((out, e)),
        }
    }

    fn read_from_file(&mut self) -> Result<(), eyre::Report> {
        let path = get_config_path()?;
        let mut buf = String::new();
        let msg = format!("reading from {}", path.display());
        OpenOptions::new()
            .read(true)
            .open(&path)
            .wrap_err(msg.clone())?
            .read_to_string(&mut buf)
            .wrap_err(msg)?;
        *self = toml::from_str(&buf)
            .wrap_err(format!("deserializing config from {}", path.display()))?;

        Ok(())
    }
}

pub fn get_config_path() -> Result<PathBuf, eyre::Error> {
    let path = std::env::home_dir().ok_or_eyre("missing home directory env")?;
    let path = path.join(".config/tasks/config.toml");
    Ok(path)
}
