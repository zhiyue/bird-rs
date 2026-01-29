//! Storage monitoring utilities for tracking database size during sync operations.

use std::path::{Path, PathBuf};

/// Storage monitor for checking database size during sync.
#[derive(Debug, Clone)]
pub struct StorageMonitor {
    /// Path to the storage directory (e.g., RocksDB path).
    storage_path: Option<PathBuf>,
    /// Maximum allowed storage size in bytes. If exceeded, sync should stop.
    max_bytes: Option<u64>,
}

impl StorageMonitor {
    /// Create a new storage monitor.
    pub fn new(storage_path: Option<PathBuf>, max_bytes: Option<u64>) -> Self {
        Self {
            storage_path,
            max_bytes,
        }
    }

    /// Create from a SurrealDB endpoint string.
    pub fn from_endpoint(endpoint: &str, max_bytes: Option<u64>) -> Self {
        let storage_path = rocksdb_path_from_endpoint(endpoint);
        Self::new(storage_path, max_bytes)
    }

    /// Check if storage monitoring is available.
    pub fn is_available(&self) -> bool {
        self.storage_path.is_some()
    }

    /// Get the current storage size in bytes.
    pub fn current_size(&self) -> Option<u64> {
        self.storage_path
            .as_ref()
            .and_then(|path| directory_size_bytes(path).ok())
    }

    /// Get the current storage size formatted as a human-readable string.
    pub fn current_size_formatted(&self) -> Option<String> {
        self.current_size().map(format_bytes)
    }

    /// Check if storage limit is exceeded.
    /// Returns `Some(current_size)` if exceeded, `None` if within limits or no limit set.
    pub fn check_limit(&self) -> Result<(), StorageLimitExceeded> {
        let Some(max_bytes) = self.max_bytes else {
            return Ok(());
        };

        let Some(current_size) = self.current_size() else {
            return Ok(());
        };

        if current_size > max_bytes {
            Err(StorageLimitExceeded {
                current_bytes: current_size,
                max_bytes,
            })
        } else {
            Ok(())
        }
    }

    /// Get the storage path if available.
    pub fn storage_path(&self) -> Option<&Path> {
        self.storage_path.as_deref()
    }

    /// Get the max bytes limit if set.
    pub fn max_bytes(&self) -> Option<u64> {
        self.max_bytes
    }

    /// Get progress info for display.
    pub fn progress_info(&self) -> StorageProgress {
        let current_bytes = self.current_size();
        StorageProgress {
            current_bytes,
            max_bytes: self.max_bytes,
            current_formatted: current_bytes.map(format_bytes),
            max_formatted: self.max_bytes.map(format_bytes),
        }
    }
}

/// Error when storage limit is exceeded.
#[derive(Debug, Clone)]
pub struct StorageLimitExceeded {
    pub current_bytes: u64,
    pub max_bytes: u64,
}

impl std::fmt::Display for StorageLimitExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Storage limit exceeded: {} > {} limit",
            format_bytes(self.current_bytes),
            format_bytes(self.max_bytes)
        )
    }
}

impl std::error::Error for StorageLimitExceeded {}

/// Progress information for storage.
#[derive(Debug, Clone)]
pub struct StorageProgress {
    pub current_bytes: Option<u64>,
    pub max_bytes: Option<u64>,
    pub current_formatted: Option<String>,
    pub max_formatted: Option<String>,
}

impl StorageProgress {
    /// Format as a progress string like "1.2 GiB / 5.0 GiB" or "1.2 GiB" if no limit.
    pub fn format(&self) -> String {
        match (&self.current_formatted, &self.max_formatted) {
            (Some(current), Some(max)) => format!("{} / {}", current, max),
            (Some(current), None) => current.clone(),
            _ => "unknown".to_string(),
        }
    }

    /// Get percentage used (0-100), or None if no limit set.
    pub fn percentage(&self) -> Option<f64> {
        match (self.current_bytes, self.max_bytes) {
            (Some(current), Some(max)) if max > 0 => Some((current as f64 / max as f64) * 100.0),
            _ => None,
        }
    }
}

/// Extract RocksDB path from endpoint string.
pub fn rocksdb_path_from_endpoint(endpoint: &str) -> Option<PathBuf> {
    const PREFIX: &str = "rocksdb://";
    let path = endpoint.strip_prefix(PREFIX)?;
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

/// Calculate the total size of a directory in bytes.
pub fn directory_size_bytes(path: &Path) -> anyhow::Result<u64> {
    let metadata = std::fs::symlink_metadata(path).map_err(|err| {
        anyhow::anyhow!("Failed to read storage path {}: {}", path.display(), err)
    })?;

    if metadata.is_file() {
        return Ok(metadata.len());
    }

    if !metadata.is_dir() {
        return Ok(0);
    }

    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|err| {
            anyhow::anyhow!("Failed to read directory {}: {}", dir.display(), err)
        })?;

        for entry in entries {
            let entry = entry.map_err(|err| anyhow::anyhow!("Failed to read entry: {}", err))?;
            let entry_path = entry.path();
            let entry_meta = std::fs::symlink_metadata(&entry_path).map_err(|err| {
                anyhow::anyhow!(
                    "Failed to read metadata for {}: {}",
                    entry_path.display(),
                    err
                )
            })?;

            if entry_meta.file_type().is_symlink() {
                continue;
            }

            if entry_meta.is_dir() {
                stack.push(entry_path);
            } else {
                total = total.saturating_add(entry_meta.len());
            }
        }
    }

    Ok(total)
}

/// Format bytes as a human-readable string (e.g., "1.5 GiB").
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0usize;

    while size >= 1024.0 && unit + 1 < UNITS.len() {
        size /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.1} {}", size, UNITS[unit])
    }
}

/// Parse a size string like "5GB", "500MB", "5GiB" into bytes.
pub fn parse_size(s: &str) -> Result<u64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Empty size string".to_string());
    }

    // Find where the number ends and the unit begins
    let (num_part, unit_part) = s
        .char_indices()
        .find(|(_, c)| c.is_alphabetic())
        .map(|(i, _)| (&s[..i], &s[i..]))
        .unwrap_or((s, ""));

    let num: f64 = num_part
        .trim()
        .parse()
        .map_err(|_| format!("Invalid number: {}", num_part))?;

    let multiplier: u64 = match unit_part.trim().to_uppercase().as_str() {
        "" | "B" => 1,
        "K" | "KB" | "KIB" => 1024,
        "M" | "MB" | "MIB" => 1024 * 1024,
        "G" | "GB" | "GIB" => 1024 * 1024 * 1024,
        "T" | "TB" | "TIB" => 1024 * 1024 * 1024 * 1024,
        other => return Err(format!("Unknown unit: {}", other)),
    };

    Ok((num * multiplier as f64) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("1024").unwrap(), 1024);
        assert_eq!(parse_size("1KB").unwrap(), 1024);
        assert_eq!(parse_size("1 KB").unwrap(), 1024);
        assert_eq!(parse_size("1KiB").unwrap(), 1024);
        assert_eq!(parse_size("1MB").unwrap(), 1024 * 1024);
        assert_eq!(parse_size("1GB").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_size("5GB").unwrap(), 5 * 1024 * 1024 * 1024);
        assert_eq!(parse_size("5 GiB").unwrap(), 5 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MiB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GiB");
    }
}
