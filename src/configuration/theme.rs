use iced::Theme;

pub fn convert_str_to_theme(theme_str: String) -> Theme {
    let mut found_theme: Option<Theme> = None;

    for theme_variant in Theme::ALL.iter() {
        // Check against the Debug representation (e.g., "CatppuccinMacchiato")
        let debug_name = format!("{:?}", theme_variant);
        // Check against the Display representation (e.g., "Catppuccin Macchiato")
        let display_name = theme_variant.to_string();

        if theme_str == debug_name || theme_str == display_name {
            found_theme = Some(theme_variant.clone());
            break;
        }
    }

    let theme = match found_theme {
        Some(t) => t,
        None => {
            eprintln!(
                "Warning: Theme '{}' not recognized or is a custom theme. Defaulting to Dark.",
                theme_str
            );
            Theme::Dark // Default theme if the string doesn't match a known built-in theme
        }
    };

    return theme;
}
