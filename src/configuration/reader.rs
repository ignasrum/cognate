#[path = "../json/reader.rs"]
mod json;

use json::read_json_file;
use serde::{Deserialize, Serialize};
use serde_json::Value; // Import Serialize and Deserialize traits

#[derive(Default, Clone, Debug, Serialize, Deserialize)] // Add Clone, Debug, Serialize, Deserialize for completeness
pub struct Configuration {
    pub theme: String,
    pub notebook_path: String, // Add notebook_path field
}

pub fn read_configuration(file_path: &str) -> Result<Configuration, Box<dyn std::error::Error>> {
    let json: Result<Value, Box<dyn std::error::Error>> = read_json_file(file_path);

    match json {
        Ok(json_value) => {
            // Extract the theme value
            let theme = json_value["theme"]
                .as_str()
                .ok_or("Theme not found or not a string")?
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
            })
        }
        Err(err) => {
            eprintln!("Error reading JSON: {}", err);
            Err(err)
        }
    }
}
