#[cfg(test)]
mod tests {
    use crate::configuration::{read_configuration, save_scale_to_config};
    use serde_json::Value;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestConfigFile {
        path: PathBuf,
    }

    impl TestConfigFile {
        fn new(name: &str, contents: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("System clock error")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "cognate_config_test_{}_{}_{}.json",
                name,
                std::process::id(),
                unique
            ));
            fs::write(&path, contents).expect("Failed to write temporary config file");
            Self { path }
        }

        fn as_str(&self) -> &str {
            self.path
                .to_str()
                .expect("Temporary config path must be valid UTF-8")
        }
    }

    impl Drop for TestConfigFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    fn unique_temp_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System clock error")
            .as_nanos();

        std::env::temp_dir().join(format!(
            "cognate_config_test_{}_{}_{}.json",
            name,
            std::process::id(),
            unique
        ))
    }

    #[test]
    fn read_configuration_reads_theme_and_notebook_path() {
        let config_file = TestConfigFile::new(
            "valid",
            r#"{
                "theme": "CatppuccinMacchiato",
                "notebook_path": "/tmp/my_notebook",
                "scale": 1.25
            }"#,
        );

        let config =
            read_configuration(config_file.as_str()).expect("Expected valid configuration");

        assert_eq!(config.theme, "CatppuccinMacchiato");
        assert_eq!(config.notebook_path, "/tmp/my_notebook");
        assert!((config.scale - 1.25).abs() < f32::EPSILON);
        assert!(!config.version.is_empty());
    }

    #[test]
    fn read_configuration_errors_when_file_missing() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System clock error")
            .as_nanos();
        let missing_path = std::env::temp_dir().join(format!(
            "cognate_missing_config_{}_{}.json",
            std::process::id(),
            unique
        ));

        let result = read_configuration(
            missing_path
                .to_str()
                .expect("Temporary path must be valid UTF-8"),
        );

        assert!(result.is_err());
    }

    #[test]
    fn read_configuration_defaults_scale_when_missing() {
        let config_file = TestConfigFile::new(
            "missing_scale",
            r#"{
                "theme": "Dark",
                "notebook_path": "/tmp/my_notebook"
            }"#,
        );

        let config =
            read_configuration(config_file.as_str()).expect("Expected valid configuration");

        assert!((config.scale - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn read_configuration_defaults_scale_when_invalid() {
        let config_file = TestConfigFile::new(
            "invalid_scale",
            r#"{
                "theme": "Dark",
                "notebook_path": "/tmp/my_notebook",
                "scale": -2
            }"#,
        );

        let config =
            read_configuration(config_file.as_str()).expect("Expected valid configuration");

        assert!((config.scale - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn read_configuration_errors_when_theme_is_missing() {
        let config_file = TestConfigFile::new(
            "missing_theme",
            r#"{
                "notebook_path": "/tmp/my_notebook"
            }"#,
        );

        let result = read_configuration(config_file.as_str());

        assert!(result.is_err());
    }

    #[test]
    fn save_scale_to_config_creates_file_when_missing() {
        let path = unique_temp_path("save_scale_missing");
        let path_str = path
            .to_str()
            .expect("Temporary config path must be valid UTF-8")
            .to_string();

        assert!(!path.exists(), "Test path should start absent");
        save_scale_to_config(&path_str, 1.5).expect("Expected scale save to succeed");

        let stored = fs::read_to_string(&path).expect("Expected config file to be created");
        let json: Value = serde_json::from_str(&stored).expect("Expected valid JSON");
        assert_eq!(json["scale"].as_f64(), Some(1.5));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn save_scale_to_config_preserves_existing_fields() {
        let config_file = TestConfigFile::new(
            "save_scale_preserve_fields",
            r#"{
                "theme": "Dark",
                "notebook_path": "/tmp/my_notebook",
                "scale": 1.0
            }"#,
        );

        save_scale_to_config(config_file.as_str(), 1.75).expect("Expected scale save to succeed");

        let stored =
            fs::read_to_string(config_file.as_str()).expect("Expected config file to be readable");
        let json: Value = serde_json::from_str(&stored).expect("Expected valid JSON");
        assert_eq!(json["theme"].as_str(), Some("Dark"));
        assert_eq!(json["notebook_path"].as_str(), Some("/tmp/my_notebook"));
        assert_eq!(json["scale"].as_f64(), Some(1.75));
    }

    #[test]
    fn save_scale_to_config_errors_when_root_is_not_object() {
        let config_file =
            TestConfigFile::new("save_scale_root_array", r#"["not", "an", "object"]"#);

        let result = save_scale_to_config(config_file.as_str(), 1.2);

        assert!(result.is_err());
        let message = result.expect_err("Expected a validation error");
        assert!(
            message.contains("root must be a JSON object"),
            "Unexpected error message: {}",
            message
        );
    }

    #[test]
    fn save_scale_to_config_rejects_non_positive_or_non_finite_scales() {
        let path = unique_temp_path("save_scale_invalid");
        let path_str = path
            .to_str()
            .expect("Temporary config path must be valid UTF-8")
            .to_string();

        let zero_result = save_scale_to_config(&path_str, 0.0);
        assert!(zero_result.is_err());

        let nan_result = save_scale_to_config(&path_str, f32::NAN);
        assert!(nan_result.is_err());

        assert!(
            !path.exists(),
            "Invalid scales should not create or modify config files"
        );
    }
}
