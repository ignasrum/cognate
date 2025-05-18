#[path = "../json/reader.rs"]
mod json;

use json::read_json_file;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::Read;
use toml; // Corrected import to bring Read trait into scope

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Configuration {
    pub theme: String,
    pub notebook_path: String,
    pub version: String,
}

// Function to read and parse Cargo.toml
fn read_cargo_toml() -> Result<toml::Value, Box<dyn std::error::Error>> {
    let cargo_toml_path = "Cargo.toml";
    let mut file = fs::File::open(cargo_toml_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?; // read_to_string is available via the imported Read trait
    let value = toml::from_str(&contents)?;
    Ok(value)
}

pub fn read_configuration(file_path: &str) -> Result<Configuration, Box<dyn std::error::Error>> {
    let json_config: Result<Value, Box<dyn std::error::Error>> = read_json_file(file_path);
    let cargo_toml = read_cargo_toml()?; // Read and parse Cargo.toml

    // Extract version from Cargo.toml
    let version = cargo_toml
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .ok_or("Version not found or not a string in Cargo.toml")?
        .to_string();

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
                version, // Include the read version
            })
        }
        Err(err) => {
            eprintln!("Error reading config.json: {}", err);
            // Even if config.json fails, try to return version from Cargo.toml
            Ok(Configuration {
                theme: "Dark".to_string(),    // Default theme if config.json fails
                notebook_path: String::new(), // Empty path if config.json fails
                version,                      // Still include the read version
            })
        }
    }
}
