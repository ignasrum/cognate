#[path = "configuration/reader.rs"]
mod configuration;
#[path = "editor/editor.rs"]
mod editor;

use std::process::exit;

use editor::Editor;
use iced::Application;

fn main() -> iced::Result {
    match configuration::read_configuration("./config.json") {
        Ok(config) => {
            println!("Theme: {}", config.theme);
            let settings = iced::Settings {
                window: iced::window::Settings {
                    size: iced::Size::new(600.0, 400.0),
                    ..iced::window::Settings::default()
                },
                flags: config,
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
