#[cfg(test)]
mod tests {
    use crate::json::reader::read_json_file;

    #[test]
    fn test_read_json_file() {
        // Create a dummy JSON file for testing in the project root
        let test_file_path = "test_data.json";
        let json_data = r#"{"name": "John Doe", "age": 30, "is_active": true}"#;
        std::fs::write(test_file_path, json_data).expect("Failed to create test file");

        // Read the JSON file
        let result = read_json_file(test_file_path);

        // Assert that the result is Ok
        assert!(result.is_ok());

        // Assert that the JSON data is parsed correctly
        let json = result.unwrap();
        assert_eq!(json["name"], "John Doe");
        assert_eq!(json["age"], 30);
        assert_eq!(json["is_active"], true);

        // Clean up the test file from the project root
        std::fs::remove_file(test_file_path).expect("Failed to remove test file");
    }

    #[test]
    fn test_read_json_file_not_found() {
        // This path should correctly indicate a non-existent file relative to the project root
        let result = read_json_file("non_existent_file.json");
        assert!(result.is_err());
    }

    #[test]
    fn test_read_json_file_invalid_json() {
        // Create a dummy JSON file with invalid JSON data in the project root
        let test_file_path = "test_data_invalid.json";
        let invalid_json_data = r#"{"name": "John Doe", "age": 30, "is_active": true"#; // Missing closing brace
        std::fs::write(test_file_path, invalid_json_data).expect("Failed to create test file");

        // Read the JSON file
        let result = read_json_file(test_file_path);

        // Assert that the result is Err
        assert!(result.is_err());

        // Clean up the test file from the project root
        std::fs::remove_file(test_file_path).expect("Failed to remove test file");
    }
}
