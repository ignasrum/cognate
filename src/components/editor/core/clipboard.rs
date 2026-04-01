use iced::widget::text_editor::{Action, Edit};
use std::path::{Path, PathBuf};

pub(super) enum ClipboardPastePayload {
    Text(String),
    ImageBase64(String),
}

pub(super) fn read_clipboard_paste_payload() -> Result<Option<ClipboardPastePayload>, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|err| format!("Failed to open clipboard: {}", err))?;

    if let Ok(text) = clipboard.get_text()
        && !text.is_empty()
    {
        if let Some(image_base64) = read_clipboard_image_file_as_base64_from_text(&text) {
            return Ok(Some(ClipboardPastePayload::ImageBase64(image_base64)));
        }

        return Ok(Some(ClipboardPastePayload::Text(text)));
    }

    Ok(read_clipboard_image_as_base64_png_from(&mut clipboard)?
        .map(ClipboardPastePayload::ImageBase64))
}

fn read_clipboard_image_as_base64_png_from(
    clipboard: &mut arboard::Clipboard,
) -> Result<Option<String>, String> {
    use base64::Engine;

    let attempt_count = clipboard_image_retry_attempts();
    let mut image = None;

    for attempt in 0..attempt_count {
        match clipboard.get_image() {
            Ok(value) => {
                image = Some(value);
                break;
            }
            Err(_) if attempt + 1 < attempt_count => {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(_) => break,
        }
    }

    let Some(image) = image else {
        return Ok(None);
    };

    let mut encoded_png = Vec::new();
    {
        let mut encoder =
            png::Encoder::new(&mut encoded_png, image.width as u32, image.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|err| format!("Failed to write image header: {}", err))?;
        writer
            .write_image_data(image.bytes.as_ref())
            .map_err(|err| format!("Failed to encode clipboard image as PNG: {}", err))?;
    }

    Ok(Some(
        base64::engine::general_purpose::STANDARD.encode(encoded_png),
    ))
}

fn clipboard_image_retry_attempts() -> usize {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return 3;
        }
    }

    1
}

pub(super) fn read_clipboard_image_file_as_base64_from_text(text: &str) -> Option<String> {
    parse_clipboard_image_file_paths(text)
        .into_iter()
        .find_map(|path| read_image_file_as_base64(&path))
}

pub(super) fn paste_text_from_action(action: &Action) -> Option<String> {
    let Action::Edit(Edit::Paste(text)) = action else {
        return None;
    };

    Some((**text).clone())
}

pub(super) fn parse_clipboard_image_file_paths(text: &str) -> Vec<PathBuf> {
    let mut lines: Vec<&str> = text.lines().collect();
    if let Some(first) = lines.first().copied()
        && matches!(first.trim().to_ascii_lowercase().as_str(), "copy" | "cut")
    {
        lines.remove(0);
    }

    lines
        .into_iter()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(parse_clipboard_file_line)
        .filter(|path| path.is_file() && is_probably_image_file(path))
        .collect()
}

fn parse_clipboard_file_line(line: &str) -> Option<PathBuf> {
    let trimmed = line.trim();
    let unquoted = if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };

    if unquoted.starts_with("file://") {
        parse_file_uri_to_path(unquoted)
    } else {
        Some(PathBuf::from(unquoted))
    }
}

pub(super) fn parse_file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.trim().strip_prefix("file://")?;
    let path_part = if rest.starts_with('/') {
        rest.to_string()
    } else {
        let (host, path) = rest.split_once('/')?;
        if !(host.is_empty() || host.eq_ignore_ascii_case("localhost")) {
            return None;
        }
        format!("/{}", path)
    };

    percent_decode(&path_part).map(PathBuf::from)
}

pub(super) fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut cursor = 0usize;

    while cursor < bytes.len() {
        if bytes[cursor] == b'%' {
            if cursor + 2 >= bytes.len() {
                return None;
            }

            let hi = (bytes[cursor + 1] as char).to_digit(16)?;
            let lo = (bytes[cursor + 2] as char).to_digit(16)?;
            decoded.push(((hi << 4) + lo) as u8);
            cursor += 3;
        } else {
            decoded.push(bytes[cursor]);
            cursor += 1;
        }
    }

    String::from_utf8(decoded).ok()
}

pub(super) fn is_probably_image_file(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };

    matches!(
        ext.to_ascii_lowercase().as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "bmp"
            | "webp"
            | "tif"
            | "tiff"
            | "ico"
            | "avif"
            | "heic"
            | "heif"
    )
}

pub(super) fn read_image_file_as_base64(path: &Path) -> Option<String> {
    use base64::Engine;

    if !is_probably_image_file(path) {
        return None;
    }

    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() == 0 || metadata.len() > 50 * 1024 * 1024 {
        return None;
    }

    let bytes = std::fs::read(path).ok()?;
    Some(base64::engine::general_purpose::STANDARD.encode(bytes))
}
