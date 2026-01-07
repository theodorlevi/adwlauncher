use std::path::{Path, PathBuf};

const ICON_SIZES: &[u32] = &[256, 128, 96, 64, 48, 32, 24, 16];
const ICON_THEMES: &[&str] = &["hicolor", "Adwaita", "gnome"];
const ICON_EXTENSIONS: &[&str] = &[".png", ".svg", ".xpm"];

pub fn resolve_icon_path(icon_name: &str) -> Option<String> {
    // If it's already an absolute path and exists, use it
    if icon_name.starts_with('/') {
        let path = Path::new(icon_name);
        if path.exists() {
            return Some(icon_name.to_string());
        }
    }

    // If it has an extension, it might be a filename
    if icon_name.contains('.') {
        // Try finding it in pixmaps directories
        if let Some(path) = find_in_pixmaps(icon_name) {
            return Some(path);
        }
    }

    // Try to find in icon theme directories
    if let Some(path) = find_in_icon_themes(icon_name) {
        return Some(path);
    }

    // If nothing found, return the original name (GTK will handle it)
    Some(icon_name.to_string())
}

fn find_in_pixmaps(icon_name: &str) -> Option<String> {
    let pixmap_dirs = vec![
        PathBuf::from("/usr/share/pixmaps"),
        PathBuf::from("/usr/share/icons"),
    ];

    for dir in pixmap_dirs {
        let path = dir.join(icon_name);
        if path.exists() {
            return path.to_str().map(String::from);
        }
    }

    None
}

fn find_in_icon_themes(icon_name: &str) -> Option<String> {
    // Strip any extension from the icon name
    let icon_base = icon_name
        .trim_end_matches(".png")
        .trim_end_matches(".svg")
        .trim_end_matches(".xpm");

    let home = std::env::var("HOME").unwrap_or_default();
    let icon_base_dirs = vec![
        PathBuf::from("/usr/share/icons"),
        PathBuf::from(format!("{}/.local/share/icons", home)),
        PathBuf::from(format!("{}/.icons", home)),
    ];

    // Try each theme
    for base_dir in &icon_base_dirs {
        for theme in ICON_THEMES {
            // Try each size (larger sizes first)
            for &size in ICON_SIZES {
                let size_dirs = vec![
                    format!("{size}x{size}/apps"),
                    format!("{size}x{size}/places"),
                    format!("{size}x{size}/mimetypes"),
                    "scalable/apps".to_string(),
                    "scalable/places".to_string(),
                ];

                for size_dir in &size_dirs {
                    let dir = base_dir.join(theme).join(size_dir);

                    // Try each extension
                    for ext in ICON_EXTENSIONS {
                        let path = dir.join(format!("{}{}", icon_base, ext));
                        if path.exists() {
                            return path.to_str().map(String::from);
                        }
                    }
                }
            }

            // Also try theme root directory
            for ext in ICON_EXTENSIONS {
                let path = base_dir.join(theme).join(format!("{}{}", icon_base, ext));
                if path.exists() {
                    return path.to_str().map(String::from);
                }
            }
        }
    }

    None
}

pub fn get_fallback_icon() -> &'static str {
    "application-x-executable"
}
