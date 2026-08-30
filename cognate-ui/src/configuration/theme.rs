use iced::Theme;

fn normalize_theme_name(theme_name: &str) -> String {
    let mut normalized = String::new();
    for ch in theme_name.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                normalized.push(lower);
            }
        }
    }
    normalized
}

pub fn convert_str_to_theme(theme_str: &str) -> Theme {
    let normalized_input = normalize_theme_name(theme_str);

    if normalized_input.is_empty() {
        eprintln!("Warning: Theme value is empty. Defaulting to Dark.");
        return Theme::Dark;
    }

    for theme_variant in Theme::ALL.iter() {
        // Check against the Debug representation (e.g., "CatppuccinMacchiato")
        let debug_name = format!("{:?}", theme_variant);
        // Check against the Display representation (e.g., "Catppuccin Macchiato")
        let display_name = theme_variant.to_string();

        if normalized_input == normalize_theme_name(&debug_name)
            || normalized_input == normalize_theme_name(&display_name)
        {
            return theme_variant.clone();
        }
    }

    eprintln!(
        "Warning: Theme '{}' not recognized or is a custom theme. Defaulting to Dark.",
        theme_str
    );
    Theme::Dark
}
