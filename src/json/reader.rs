use serde_json::Value;
use std::fs::File;
use std::io::Read;

pub fn read_json_file(file_path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    // Open the file
    let mut file = File::open(file_path)?;

    // Read the file content into a string
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    // Parse the JSON string into a serde_json::Value
    let json: Value = serde_json::from_str(&contents)?;

    Ok(json)
}
