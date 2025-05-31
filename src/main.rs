#[path = "configuration/reader.rs"]
mod configuration;

// Declare the components module with all the submodules
mod components {
    // Editor module and submodules
    pub mod editor;
    
    // Note explorer module
    pub mod note_explorer {
        #[path = "../note_explorer/note_explorer.rs"]
        pub mod note_explorer;
        pub use self::note_explorer::NoteExplorer;
        pub use self::note_explorer::Message;
    }
    
    // Visualizer module
    pub mod visualizer {
        #[path = "../visualizer/visualizer.rs"]
        pub mod visualizer;
        pub use self::visualizer::Visualizer;
        pub use self::visualizer::Message;
    }
}

mod notebook;
mod json;

#[cfg(test)]
mod tests;

use std::env;
use std::process::exit;

use components::editor::Editor;
use iced::Application;

fn main() -> iced::Result {
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

    // read_configuration now handles potential errors with config.json internally
    // and always provides the embedded version.
    let config = match configuration::read_configuration(&config_path) {
        Ok(cfg) => {
            // Configuration read successfully (potentially with default path/theme)
            println!("Theme: {}", cfg.theme);
            println!("Notebook Path: {}", cfg.notebook_path);
            println!("App Version: {}", cfg.version);
            cfg
        }
        Err(err) => {
            // This branch should theoretically not be reachable anymore if
            // read_configuration always returns Ok(Config) even on config.json error.
            // However, keeping defensive programming is good.
            eprintln!("Failed to read configuration: {}", err);
            exit(1);
        }
    };

    let settings = iced::Settings {
        window: iced::window::Settings {
            size: iced::Size::new(1000.0, 800.0),
            ..iced::window::Settings::default()
        },
        flags: config, // Pass the entire config struct as flags
        ..iced::Settings::default()
    };
    let _ = Editor::run(settings);

    Ok(())
}
