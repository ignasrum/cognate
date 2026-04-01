use crate::json::reader::read_json_file;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Configuration {
    pub theme: String,
    pub notebook_path: String,
    pub scale: f32,
    pub config_path: String,
    pub version: String,
}

#[derive(Debug, Deserialize)]
struct RawConfiguration {
    theme: String,
    #[serde(default)]
    notebook_path: Option<String>,
    #[serde(default)]
    scale: Option<f32>,
}

#[cfg(test)]
const FAIL_CONFIG_ATOMIC_RENAME_MARKER: &str = ".cognate_fail_config_atomic_rename";

fn config_directory(config_path: &Path) -> &Path {
    config_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn invalid_config(message: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::new(std::io::Error::new(ErrorKind::InvalidData, message.into()))
}

fn build_atomic_temp_path(target_path: &Path) -> PathBuf {
    let directory = config_directory(target_path);
    let target_name = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("cognate_config");
    let timestamp_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    directory.join(format!(
        ".{}.cognate_tmp_{}_{}",
        target_name,
        process::id(),
        timestamp_nanos
    ))
}

fn atomic_rename(from: &Path, to: &Path) -> Result<(), std::io::Error> {
    #[cfg(test)]
    if config_directory(to)
        .join(FAIL_CONFIG_ATOMIC_RENAME_MARKER)
        .exists()
    {
        return Err(std::io::Error::other(format!(
            "Simulated config atomic rename failure for '{}'",
            to.display()
        )));
    }

    fs::rename(from, to)
}

fn write_string_atomically(target_path: &Path, content: &str) -> Result<(), String> {
    let directory = config_directory(target_path);
    fs::create_dir_all(directory)
        .map_err(|err| format!("Failed to create config directory: {}", err))?;

    let temp_path = build_atomic_temp_path(target_path);
    fs::write(&temp_path, content)
        .map_err(|err| format!("Failed to write temporary config file: {}", err))?;

    if let Err(err) = atomic_rename(&temp_path, target_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(format!("Failed to atomically replace config file: {}", err));
    }

    Ok(())
}

pub fn read_configuration(file_path: &str) -> Result<Configuration, Box<dyn std::error::Error>> {
    // Get the version at compile time
    let version = env!("CARGO_PKG_VERSION").to_string();

    let raw: RawConfiguration = read_json_file(file_path)?;

    if raw.theme.trim().is_empty() {
        return Err(invalid_config(
            "Theme in config.json must be a non-empty string.",
        ));
    }

    let scale = match raw.scale {
        None => 1.0,
        Some(scale) if scale.is_finite() && scale > 0.0 => scale,
        Some(scale) => {
            return Err(invalid_config(format!(
                "Scale in config.json must be a positive finite number, got '{}'.",
                scale
            )));
        }
    };

    Ok(Configuration {
        theme: raw.theme,
        notebook_path: raw.notebook_path.unwrap_or_default(),
        scale,
        config_path: file_path.to_string(),
        version,
    })
}

pub fn save_scale_to_config(file_path: &str, scale: f32) -> Result<(), String> {
    if !(scale.is_finite() && scale > 0.0) {
        return Err("Scale must be a positive finite number.".to_string());
    }

    let config_path = std::path::Path::new(file_path);
    let mut json_value = match std::fs::read_to_string(config_path) {
        Ok(contents) => serde_json::from_str::<Value>(&contents)
            .map_err(|err| format!("Failed to parse config file: {}", err))?,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Value::Object(Default::default()),
        Err(err) => return Err(format!("Failed to read config file: {}", err)),
    };

    if !json_value.is_object() {
        return Err("Config file root must be a JSON object.".to_string());
    }

    json_value["scale"] = serde_json::json!(scale);

    let serialized = serde_json::to_string_pretty(&json_value)
        .map_err(|err| format!("Failed to serialize config JSON: {}", err))?;
    write_string_atomically(config_path, &format!("{serialized}\n"))?;

    Ok(())
}
