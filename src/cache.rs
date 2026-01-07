use crate::error::{LauncherError, Result};
use crate::types::Entry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Serialize, Deserialize, Debug)]
pub struct CacheData {
    pub entries: Vec<Entry>,
    pub directory_timestamps: HashMap<PathBuf, SystemTime>,
}

impl CacheData {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            directory_timestamps: HashMap::new(),
        }
    }
}

pub struct Cache {
    cache_path: PathBuf,
}

impl Cache {
    pub fn new() -> Result<Self> {
        let cache_dir = dirs::cache_dir()
            .ok_or_else(|| {
                LauncherError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Could not find cache directory",
                ))
            })?
            .join("adwlauncher");

        fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            cache_path: cache_dir.join("entries.cache"),
        })
    }

    pub fn load(&self) -> Result<CacheData> {
        if !self.cache_path.exists() {
            return Ok(CacheData::new());
        }

        let data = fs::read(&self.cache_path)?;
        postcard::from_bytes(&data).map_err(|e| {
            LauncherError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to deserialize cache: {}", e),
            ))
        })
    }

    pub fn save(&self, cache_data: &CacheData) -> Result<()> {
        let data = postcard::to_allocvec(cache_data).map_err(|e| {
            LauncherError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize cache: {}", e),
            ))
        })?;

        fs::write(&self.cache_path, data)?;
        Ok(())
    }

    pub fn is_valid(&self, cache_data: &CacheData, directories: &[PathBuf]) -> bool {
        // Check if all directories have the same modification time
        for dir in directories {
            if !dir.exists() {
                continue;
            }

            let current_mtime = match get_dir_mtime(dir) {
                Ok(mtime) => mtime,
                Err(_) => return false,
            };

            match cache_data.directory_timestamps.get(dir) {
                Some(&cached_mtime) if cached_mtime == current_mtime => continue,
                _ => return false,
            }
        }

        true
    }
}

fn get_dir_mtime(path: &Path) -> Result<SystemTime> {
    let metadata = fs::metadata(path)?;
    metadata.modified().map_err(|e| e.into())
}

pub fn get_app_directories() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/tmp"));
    vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from(format!("{}/.local/share/applications", home)),
        PathBuf::from("/var/lib/flatpak/exports/share/applications/"),
        PathBuf::from(format!(
            "{}/.local/share/flatpak/exports/share/applications/",
            home
        )),
    ]
}

pub fn collect_directory_timestamps(directories: &[PathBuf]) -> HashMap<PathBuf, SystemTime> {
    directories
        .iter()
        .filter_map(|dir| get_dir_mtime(dir).ok().map(|mtime| (dir.clone(), mtime)))
        .collect()
}
