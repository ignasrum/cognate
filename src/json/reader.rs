use serde::de::DeserializeOwned;
use std::fs::File;
use std::io::Read;

#[allow(dead_code)]
pub fn read_json_file<T: DeserializeOwned>(
    file_path: &str,
) -> Result<T, Box<dyn std::error::Error>> {
    // Open the file
    let mut file = File::open(file_path)?;

    // Read the file content into a string
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    // Parse the JSON string into the requested type.
    let json = serde_json::from_str::<T>(&contents)?;

    Ok(json)
}
