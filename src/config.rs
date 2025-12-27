use std::{collections::HashMap, fs::OpenOptions, io::Read, path::PathBuf};

use crossterm::event::KeyCode;
use eyre::{Context, OptionExt, Result};
use serde::{Deserialize, Serialize};
use toml::de::ValueDeserializer;

use crate::tui::app::{KeyAction, Mode};

#[derive(Debug, Clone)]
pub struct Config {
    pub data_path: PathBuf,
    pub log_path: PathBuf,
    pub keybinds: HashMap<Mode, HashMap<KeyCode, KeyAction>>,
}

impl Config {
    pub fn load() -> Result<Self, (Self, eyre::Report)> {
        FileConfig::load()
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct FileConfig {
    data_path: PathBuf,
    log_path: PathBuf,
    keybinds: HashMap<String, HashMap<String, KeyAction>>,
}

pub fn get_default_app_data_path() -> PathBuf {
    let dirs = directories::ProjectDirs::from("com", "Tweoss", "Task List")
        .clone()
        .unwrap();
    let data_dir = dirs.data_dir();
    data_dir.to_path_buf()
}

impl FileConfig {
    fn to_config(&self) -> Result<Config, (Config, eyre::Report)> {
        Ok(Config {
            keybinds: map_keybinds(self.keybinds.clone()).map_err(|e| {
                (
                    Config {
                        data_path: self.data_path.clone(),
                        log_path: self.log_path.clone(),
                        keybinds: HashMap::new(),
                    },
                    e,
                )
            })?,
            data_path: self.data_path.clone(),
            log_path: self.log_path.clone(),
        })
    }

    fn load() -> Result<Config, (Config, eyre::Report)> {
        let mut out = Self {
            data_path: get_default_app_data_path().join("tasks"),
            log_path: get_default_app_data_path().join("logs"),
            keybinds: HashMap::new(),
        };
        match out.read_from_file() {
            Ok(_) => out.to_config(),
            Err(e) => match out.to_config() {
                // Use original error instead of conversion error.
                Ok(out) | Err((out, _)) => Err((out, e)),
            },
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

fn map_keybinds(
    keybinds: HashMap<String, HashMap<String, KeyAction>>,
) -> Result<HashMap<Mode, HashMap<KeyCode, KeyAction>>, eyre::Report> {
    keybinds
        .clone()
        .into_iter()
        .map(|(m, map)| {
            Ok((
                match m.as_str() {
                    "Normal" => Mode::Normal,
                    _ => Mode::Key(string_to_keycode(m)?),
                },
                map.into_iter()
                    .map(|(s, a)| Ok::<_, eyre::Report>((string_to_keycode(s)?, a)))
                    .collect::<Result<HashMap<_, _>, _>>()?,
            ))
        })
        .collect()
}
fn string_to_keycode(s: String) -> Result<KeyCode, eyre::Error> {
    if s.len() == 1 {
        // Assume it's a single character.
        Ok(KeyCode::Char(s.chars().next().unwrap()))
    } else {
        let value_str = format!("\"{}\"", s);
        let deserializer = ValueDeserializer::parse(&value_str).wrap_err("parsing keycode")?;
        KeyCode::deserialize(deserializer).wrap_err("deserializing keycode")
    }
}

pub fn get_config_path() -> Result<PathBuf, eyre::Error> {
    let path = std::env::home_dir().ok_or_eyre("missing home directory env")?;
    let path = path.join(".config/tasks/config.toml");
    Ok(path)
}
