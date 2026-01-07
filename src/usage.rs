use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UsageStats {
    pub last_used: u64,
    pub use_count: u32,
}

impl UsageStats {
    fn new() -> Self {
        Self {
            last_used: current_timestamp(),
            use_count: 1,
        }
    }

    fn update(&mut self) {
        self.last_used = current_timestamp();
        self.use_count += 1;
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct UsageTracker {
    // Key is the app name, value is usage stats
    stats: HashMap<String, UsageStats>,
}

impl UsageTracker {
    pub fn new() -> Self {
        Self {
            stats: HashMap::new(),
        }
    }

    pub fn load() -> Result<Self> {
        let path = Self::get_storage_path()?;

        if !path.exists() {
            return Ok(Self::new());
        }

        let data = fs::read(&path)?;
        postcard::from_bytes(&data).map_err(|e| {
            crate::error::LauncherError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to deserialize usage data: {}", e),
            ))
        })
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::get_storage_path()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let data = postcard::to_allocvec(self).map_err(|e| {
            crate::error::LauncherError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize usage data: {}", e),
            ))
        })?;

        fs::write(&path, data)?;
        Ok(())
    }

    pub fn record_launch(&mut self, app_name: &str) {
        self.stats
            .entry(app_name.to_string())
            .and_modify(|stats| stats.update())
            .or_insert_with(UsageStats::new);
    }

    pub fn get_stats(&self, app_name: &str) -> Option<&UsageStats> {
        self.stats.get(app_name)
    }

    /// Calculate a boost score for an app based on usage
    /// Returns a value between 0.0 and 1.0
    pub fn calculate_boost(&self, app_name: &str) -> f64 {
        let stats = match self.get_stats(app_name) {
            Some(s) => s,
            None => return 0.0,
        };

        let now = current_timestamp();
        let age_seconds = now.saturating_sub(stats.last_used);

        // Recency boost: decays exponentially
        // Apps used in last hour get full boost, decays over 30 days
        let recency_boost = if age_seconds < 3600 {
            1.0
        } else if age_seconds < 86400 {
            // Last 24 hours: 0.8-1.0
            0.8 + 0.2 * (1.0 - (age_seconds as f64 / 86400.0))
        } else if age_seconds < 604800 {
            // Last week: 0.5-0.8
            0.5 + 0.3 * (1.0 - (age_seconds as f64 / 604800.0))
        } else if age_seconds < 2592000 {
            // Last month: 0.2-0.5
            0.2 + 0.3 * (1.0 - (age_seconds as f64 / 2592000.0))
        } else {
            // Older than a month: minimal boost
            0.1
        };

        // Frequency boost: logarithmic scale
        let frequency_boost = (stats.use_count as f64).ln() / 10.0;
        let frequency_boost = frequency_boost.min(1.0);

        // Combine recency (70%) and frequency (30%)
        recency_boost * 0.7 + frequency_boost * 0.3
    }

    fn get_storage_path() -> Result<PathBuf> {
        let cache_dir = dirs::cache_dir().ok_or_else(|| {
            crate::error::LauncherError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not find cache directory",
            ))
        })?;

        Ok(cache_dir.join("adwlauncher").join("usage.dat"))
    }
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
