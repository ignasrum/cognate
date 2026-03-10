mod configuration;
mod components;

mod notebook;
mod json;

#[cfg(test)]
mod tests;

use std::env;
use std::process::exit;
use components::editor::Editor;
use configuration::theme::convert_str_to_theme;

pub fn main() -> iced::Result {
    let config_path_env_var = "COGNATE_CONFIG_PATH";
    let default_config_path = "./config.json";

    // Attempt to get the config path from the environment variable,
    // falling back to the default path if not set.
    let config_path = env::var(config_path_env_var).unwrap_or_else(|_| {
        println!(
            "Environment variable {} not set. Using default path: {}",
            config_path_env_var, default_config_path
        );
        default_config_path.to_string()
    });

    // Read configuration
    let config = match configuration::read_configuration(&config_path) {
        Ok(cfg) => {
            println!("Theme: {}", cfg.theme);
            println!("Notebook Path: {}", cfg.notebook_path);
            println!("App Version: {}", cfg.version);
            cfg
        }
        Err(err) => {
            eprintln!("Failed to read configuration: {}", err);
            exit(1);
        }
    };

    // Resolve the configured theme once and use it consistently across app startup.
    let app_theme = convert_str_to_theme(&config.theme);

    // Create the editor directly using the create function
    let (editor, initial_task) = Editor::create(config);

    // Setup the application using the simplified approach
    let app = iced::application("Cognate", Editor::update, Editor::view)
        .theme(move |_| app_theme.clone())
        .subscription(Editor::subscription);
        
    // Use a simple function that returns the editor and initial_task
    // instead of trying to implement a non-existent Initializer trait
    app.run_with(|| (editor, initial_task))
}
