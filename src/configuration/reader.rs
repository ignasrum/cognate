#[path = "../json/reader.rs"]
mod json;

use json::read_json_file;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::fs;
use std::io::Read; // Still needed for read_json_file
use toml; // toml is still needed for read_json_file (via dependency, though not directly used here) // Still needed for env! macro

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Configuration {
    pub theme: String,
    pub notebook_path: String,
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

            Ok(Configuration {
                theme,
                notebook_path,
                version, // Include the embedded version
            })
        }
        Err(err) => {
            eprintln!("Error reading config.json: {}", err);
            // If config.json fails, return a default configuration but include the embedded version
            Ok(Configuration {
                theme: "Dark".to_string(),    // Default theme if config.json fails
                notebook_path: String::new(), // Empty path if config.json fails
                version,                      // Still include the embedded version
            })
        }
    }
}
