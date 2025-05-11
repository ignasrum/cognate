#[path = "editor/editor.rs"]
mod editor;
use editor::Editor;

fn main() -> iced::Result {
    Editor::run(iced::Settings {
        window: iced::window::Settings {
            size: iced::Size::new(600.0, 400.0),
            ..iced::window::Settings::default()
        },
        ..iced::Settings::default()
    })
}
