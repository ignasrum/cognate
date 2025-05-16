#[path = "configuration/reader.rs"]
mod configuration;
#[path = "components/editor/editor.rs"]
mod editor;
mod notebook; // Use the updated notebook module

use std::process::exit;

use editor::Editor;
use iced::Application;

fn main() -> iced::Result {
    match configuration::read_configuration("./config.json") {
        Ok(config) => {
            println!("Theme: {}", config.theme);
            println!("Notebook Path: {}", config.notebook_path); // Print the read notebook path
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
            eprintln!("Failed to read configuration: {}", err);
            exit(1);
        }
    }
    Ok(())
}
