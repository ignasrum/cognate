#[path = "../json/reader.rs"]
mod json;

use json::read_json_file;
use serde_json::Value;

#[derive(Default)]
pub struct Configuration {
    pub theme: String,
}

pub fn read_configuration(file_path: &str) -> Result<Configuration, Box<dyn std::error::Error>> {
    let json: Result<Value, Box<dyn std::error::Error>> = read_json_file(file_path);

    match json {
        Ok(json_value) => {
            // Extract the theme value
            let theme = json_value["theme"]
                .as_str()
                .ok_or("Theme not found or not an integer")?
                .to_string();

            Ok(Configuration { theme })
        }
        Err(err) => {
            eprintln!("Error reading JSON: {}", err);
            Err(err)
        }
    }
}
