#[path = "configuration/reader.rs"]
mod configuration;
#[path = "components/editor/editor.rs"]
mod editor;
mod notebook;

use std::env;
use std::process::exit;

use editor::Editor;
use iced::Application;

fn main() -> iced::Result {
    let config_path_env_var = "COGNATE_CONFIG_PATH";
    let default_config_path = "./config.json";

    let config_path = match env::var(config_path_env_var) {
        Ok(path) => {
            println!(
                "Using configuration path from environment variable {}: {}",
                config_path_env_var, path
            );
            path
        }
        Err(_) => {
            println!(
                "Environment variable {} not set. Using default path: {}",
                config_path_env_var, default_config_path
            );
            default_config_path.to_string()
        }
    };

    match configuration::read_configuration(&config_path) {
        Ok(config) => {
            println!("Theme: {}", config.theme);
            println!("Notebook Path: {}", config.notebook_path);
            println!("App Version: {}", config.version); // Print the read version
            let settings = iced::Settings {
                window: iced::window::Settings {
                    size: iced::Size::new(1000.0, 800.0),
                    ..iced::window::Settings::default()
                },
                flags: config, // Pass the entire config struct as flags
                ..iced::Settings::default()
            };
            let _ = Editor::run(settings);
        }
        Err(err) => {
            eprintln!("Failed to read configuration or Cargo.toml: {}", err);
            exit(1);
        }
    }
    Ok(())
}
