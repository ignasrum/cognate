//! Cognate desktop application entry point.
//!
//! This crate wires configuration loading and Iced application bootstrapping,
//! then delegates feature behavior to modules under `components`, `notebook`,
//! and `configuration`.

mod components;
mod configuration;

mod json;
mod notebook;

#[cfg(test)]
mod tests;

use components::editor::Editor;
use configuration::theme::convert_str_to_theme;
use std::env;
use std::process::exit;

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
            println!("Scale: {}", cfg.scale);
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

    let config_for_boot = config.clone();

    // Setup the application with an explicit boot closure
    let app = iced::application(
        move || Editor::create(config_for_boot.clone()),
        Editor::update,
        Editor::view,
    )
    .title("Cognate")
    .theme(app_theme.clone())
    .scale_factor(Editor::scale_factor)
    .exit_on_close_request(false)
    .subscription(Editor::subscription);

    app.run()
}
