#[cfg(test)]
mod tests {
    use crate::configuration::theme::convert_str_to_theme;
    use iced::Theme;

    #[test]
    fn convert_str_to_theme_supports_case_insensitive_builtin_names() {
        assert_eq!(convert_str_to_theme("dark"), Theme::Dark);
        assert_eq!(convert_str_to_theme("Dark"), Theme::Dark);
        assert_eq!(convert_str_to_theme("LIGHT"), Theme::Light);
    }

    #[test]
    fn convert_str_to_theme_supports_display_name_spacing_variants() {
        assert_eq!(
            convert_str_to_theme("Catppuccin-Macchiato"),
            Theme::CatppuccinMacchiato
        );
        assert_eq!(
            convert_str_to_theme("Catppuccin Macchiato"),
            Theme::CatppuccinMacchiato
        );
        assert_eq!(
            convert_str_to_theme("catppuccinmacchiato"),
            Theme::CatppuccinMacchiato
        );
    }

    #[test]
    fn convert_str_to_theme_defaults_unknown_values_to_dark() {
        assert_eq!(convert_str_to_theme("unknown-theme"), Theme::Dark);
        assert_eq!(convert_str_to_theme(""), Theme::Dark);
    }
}
