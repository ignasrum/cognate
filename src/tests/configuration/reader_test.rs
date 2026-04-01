#[cfg(test)]
mod tests {
    use crate::configuration::read_configuration;
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
}
