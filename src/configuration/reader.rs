use crate::json::reader::read_json_file;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Configuration {
    pub theme: String,
    pub notebook_path: String,
    pub scale: f32,
    pub config_path: String,
    pub version: String,
}

// Removed the now-unnecessary read_cargo_toml function

pub fn read_configuration(file_path: &str) -> Result<Configuration, Box<dyn std::error::Error>> {
    // Get the version at compile time
    let version = env!("CARGO_PKG_VERSION").to_string();

    let json_config: Result<Value, Box<dyn std::error::Error>> = read_json_file(file_path);

    match json_config {
        Ok(json_value) => {
            // Extract the theme value
            let theme = json_value["theme"]
                .as_str()
                .ok_or("Theme not found or not a string in config.json")?
                .to_string();

            // Extract the notebook_path value
            let notebook_path = json_value["notebook_path"]
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    eprintln!("Warning: 'notebook_path' not found or not a string in config.json. Starting without a notebook.");
                    String::new() // Default to empty string if not found
                });

            // Extract optional scale value (must be > 0), defaulting to 1.0
            let scale = match json_value.get("scale") {
                None => 1.0,
                Some(raw_scale) => {
                    if let Some(scale) = raw_scale.as_f64()
                        && scale > 0.0
                    {
                        scale as f32
                    } else {
                        eprintln!(
                            "Warning: 'scale' must be a positive number in config.json. Using default scale 1.0."
                        );
                        1.0
                    }
                }
            };

            Ok(Configuration {
                theme,
                notebook_path,
                scale,
                config_path: file_path.to_string(),
                version, // Include the embedded version
            })
        }
        Err(err) => {
            eprintln!("Error reading config.json: {}", err);
            // If config.json fails, return a default configuration but include the embedded version
            Ok(Configuration {
                theme: "Dark".to_string(),    // Default theme if config.json fails
                notebook_path: String::new(), // Empty path if config.json fails
                scale: 1.0,                   // Default UI scale
                config_path: file_path.to_string(),
                version, // Still include the embedded version
            })
        }
    }
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

    if let Some(parent) = config_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create config directory: {}", err))?;
    }

    let serialized = serde_json::to_string_pretty(&json_value)
        .map_err(|err| format!("Failed to serialize config JSON: {}", err))?;
    std::fs::write(config_path, format!("{serialized}\n"))
        .map_err(|err| format!("Failed to write config file: {}", err))?;

    Ok(())
}
