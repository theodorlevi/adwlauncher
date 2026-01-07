use crate::cache::{self, Cache, CacheData};
use crate::error::{LauncherError, Result};
use crate::icon;
use crate::types::{Entry, OpenType};
use freedesktop_desktop_entry::DesktopEntry;
use niri_ipc::{Action, Request, Response};
use rayon::prelude::*;
use std::path::PathBuf;

pub fn get_entries() -> Result<Vec<Entry>> {
    let mut entries = vec![];

    // Get desktop application entries (with caching)
    entries.extend(get_desktop_entries_cached()?);

    // Get open windows (always fresh)
    entries.extend(get_window_entries()?);

    Ok(entries)
}

fn get_desktop_entries_cached() -> Result<Vec<Entry>> {
    let cache = Cache::new()?;
    let app_dirs = cache::get_app_directories();

    // Try to load from cache
    let cache_data = cache.load()?;

    // Check if cache is valid
    if cache.is_valid(&cache_data, &app_dirs) && !cache_data.entries.is_empty() {
        return Ok(cache_data.entries);
    }

    // Cache is invalid or empty, rebuild it
    let entries = get_desktop_entries(&app_dirs)?;

    // Save to cache
    let new_cache_data = CacheData {
        entries: entries.clone(),
        directory_timestamps: cache::collect_directory_timestamps(&app_dirs),
    };

    if let Err(e) = cache.save(&new_cache_data) {
        eprintln!("Failed to save cache: {}", e);
    }

    Ok(entries)
}

fn get_desktop_entries(app_dirs: &[PathBuf]) -> Result<Vec<Entry>> {
    let mut entries = vec![];

    for app_dir in app_dirs {
        let dir = match std::fs::read_dir(app_dir) {
            Ok(dir) => dir,
            Err(_) => continue, // Skip if the directory doesn't exist
        };

        let new_entries: Vec<Entry> = dir
            .collect::<Vec<_>>()
            .into_par_iter()
            .filter_map(|file| {
                let file = file.ok()?;
                let path = file.path();
                parse_desktop_entry(&path).ok()
            })
            .collect();

        entries.extend(new_entries);
    }

    Ok(entries)
}

fn parse_desktop_entry(path: &PathBuf) -> Result<Entry> {
    let desktop_file = DesktopEntry::from_path(path, None::<&[&str]>)
        .map_err(|e| LauncherError::DesktopEntry(format!("Failed to parse desktop file: {}", e)))?;

    let name = desktop_file
        .name(&[""])
        .ok_or_else(|| LauncherError::DesktopEntry("Missing name field".to_string()))?
        .to_string();

    if name.is_empty() {
        return Err(LauncherError::DesktopEntry("Empty name field".to_string()));
    }

    // Resolve icon path properly
    let icon_name = desktop_file.icon().unwrap_or(icon::get_fallback_icon());
    let icon =
        icon::resolve_icon_path(icon_name).unwrap_or_else(|| icon::get_fallback_icon().to_string());

    Ok(Entry {
        name,
        exec: desktop_file.exec().unwrap_or_default().to_string(),
        icon,
        open_type: if desktop_file.terminal() {
            OpenType::Terminal
        } else {
            OpenType::Graphical
        },
    })
}

fn get_window_entries() -> Result<Vec<Entry>> {
    let mut entries = vec![];

    let mut soc = niri_ipc::socket::Socket::connect()
        .map_err(|e| LauncherError::NiriConnection(format!("Failed to connect: {}", e)))?;

    let reply = soc
        .send(Request::Windows)
        .map_err(|e| LauncherError::NiriRequest(format!("Failed to send request: {}", e)))?;

    let response = reply.map_err(|e| LauncherError::NiriRequest(format!("Niri error: {}", e)))?;

    let windows = match response {
        Response::Windows(windows) => windows,
        _ => {
            return Err(LauncherError::NiriRequest(
                "Unexpected response type".to_string(),
            ));
        }
    };

    for window in windows {
        let name = window.title.unwrap_or_default();
        if name.is_empty() {
            continue;
        }

        let app_id = match window.app_id {
            Some(id) => id,
            None => continue,
        };

        // Resolve window icon
        let icon = icon::resolve_icon_path(&app_id).unwrap_or_else(|| app_id.clone());

        entries.push(Entry {
            name,
            exec: window.id.to_string(),
            icon,
            open_type: OpenType::Window,
        });
    }

    Ok(entries)
}

pub fn launch_entry(entry: &Entry) -> Result<()> {
    let mut soc = niri_ipc::socket::Socket::connect()
        .map_err(|e| LauncherError::NiriConnection(format!("Failed to connect: {}", e)))?;

    match entry.open_type {
        OpenType::Terminal => {
            let reply = soc
                .send(Request::Action(Action::Spawn {
                    command: vec!["ghostty".to_string(), "-c".to_string(), entry.exec.clone()],
                }))
                .map_err(|e| {
                    LauncherError::NiriRequest(format!("Failed to spawn terminal: {}", e))
                })?;

            reply.map_err(|e| LauncherError::NiriRequest(format!("Niri error: {}", e)))?;
        }
        OpenType::Graphical => {
            let reply = soc
                .send(Request::Action(Action::Spawn {
                    command: entry
                        .exec
                        .split_whitespace()
                        .map(|s| s.to_string())
                        .filter(|s| !s.contains('%'))
                        .collect(),
                }))
                .map_err(|e| {
                    LauncherError::NiriRequest(format!("Failed to spawn application: {}", e))
                })?;

            reply.map_err(|e| LauncherError::NiriRequest(format!("Niri error: {}", e)))?;
        }
        OpenType::Window => {
            let id = entry.exec.parse::<u64>()?;
            let reply = soc
                .send(Request::Action(Action::FocusWindow { id }))
                .map_err(|e| {
                    LauncherError::NiriRequest(format!("Failed to focus window: {}", e))
                })?;

            reply.map_err(|e| LauncherError::NiriRequest(format!("Niri error: {}", e)))?;
        }
    }

    Ok(())
}
