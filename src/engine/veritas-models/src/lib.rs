//! Local GGUF model management.
//!
//! Models are plain `.gguf` files stored in a per-user directory. There is no
//! catalog and no network download: the user supplies model files (for example
//! by uploading them from the desktop app) and every `.gguf` file in the cache
//! directory becomes an available model. This keeps the engine fully offline.

#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Model tier. Kept for compatibility with the FFI/JSON contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelTier {
    /// Primary accuracy model.
    Standard,
    /// Low RAM / fast.
    Lightweight,
    /// Embedding model.
    Embedding,
    /// Tiny fallback.
    Tiny,
}

/// Install status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelStatus {
    /// File present and usable.
    Ready,
    /// No usable file in the cache directory.
    Missing,
}

impl ModelStatus {
    /// Stable string form used in FFI JSON.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ModelStatus::Ready => "ready",
            ModelStatus::Missing => "missing",
        }
    }
}

/// A model available locally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    /// Unique id. Also the GGUF file stem in the cache.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Unused (no download). Kept for contract compatibility.
    pub url: String,
    /// Unused (no verification). Kept for contract compatibility.
    pub sha256: String,
    /// Size in bytes.
    pub size_bytes: u64,
    /// Minimum recommended RAM in bytes.
    pub min_ram_bytes: u64,
    /// Tier.
    pub tier: ModelTier,
}

/// Errors from model management.
#[derive(Debug, Error)]
pub enum ModelError {
    /// I/O failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Model not found.
    #[error("model not found: {0}")]
    NotFound(String),
}

/// Manages the local model cache.
#[derive(Debug, Clone)]
pub struct ModelManager {
    cache_dir: PathBuf,
}

impl ModelManager {
    /// Create a manager for the given cache dir.
    #[must_use]
    pub fn new(cache_dir: PathBuf, _specs: Vec<ModelSpec>) -> Self {
        let _ = _specs;
        Self { cache_dir }
    }

    /// Create a manager for the given cache dir (no specs).
    #[must_use]
    pub fn from_dir(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Default cache dir for the current OS.
    #[must_use]
    pub fn default_cache_dir() -> PathBuf {
        if cfg!(target_os = "windows") {
            std::env::var("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("Veritas")
                .join("models")
        } else if cfg!(target_os = "macos") {
            dirs_home().join("Library/Application Support/Veritas/models")
        } else {
            dirs_home().join(".local/share/veritas/models")
        }
    }

    /// The cache directory.
    #[must_use]
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Path where a model is stored.
    ///
    /// Returns the exact file on disk even when the uploaded file uses an
    /// upper-case extension (`.GGUF`), which happens on case-sensitive file
    /// systems. Falls back to `{id}.gguf` when no file matches.
    #[must_use]
    pub fn model_path(&self, id: &str) -> PathBuf {
        if let Ok(entries) = fs::read_dir(&self.cache_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !is_gguf(&path) {
                    continue;
                }
                if path.file_stem().and_then(|s| s.to_str()) == Some(id) {
                    return path;
                }
            }
        }
        self.cache_dir.join(format!("{id}.gguf"))
    }

    /// Status of a model. Only `Ready` when a non-empty `.gguf` file exists.
    #[must_use]
    pub fn status(&self, id: &str) -> ModelStatus {
        let path = self.model_path(id);
        match fs::metadata(&path) {
            Ok(meta) if meta.is_file() && meta.len() > 0 => ModelStatus::Ready,
            _ => ModelStatus::Missing,
        }
    }

    /// List all known models: every usable `.gguf` file in the cache dir.
    ///
    /// Hidden files, in-progress downloads (`.part`, `.tmp`, `.download`) and
    /// empty files are ignored so a half-copied upload never appears as a model
    /// the user can select.
    #[must_use]
    pub fn list(&self) -> Vec<(ModelSpec, ModelStatus)> {
        let mut out = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.cache_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !is_gguf(&path) || is_incomplete(&path) {
                    continue;
                }
                let id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                if id.is_empty() {
                    continue;
                }
                let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                if size == 0 {
                    continue;
                }
                out.push((
                    ModelSpec {
                        id: id.clone(),
                        name: path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or(&id)
                            .to_string(),
                        url: String::new(),
                        sha256: String::new(),
                        size_bytes: size,
                        min_ram_bytes: 0,
                        tier: ModelTier::Lightweight,
                    },
                    ModelStatus::Ready,
                ));
            }
        }
        out.sort_by(|a, b| a.0.name.cmp(&b.0.name));
        out
    }

    /// Find a spec by id.
    #[must_use]
    pub fn spec(&self, id: &str) -> Option<ModelSpec> {
        self.list()
            .into_iter()
            .find(|(s, _)| s.id == id)
            .map(|(s, _)| s)
    }

    /// The best installed model: prefers non-reasoning instruct models, then
    /// larger files. Reasoning models are slower and less reliable for
    /// structured JSON output, so they are only chosen when nothing else exists.
    #[must_use]
    pub fn best_installed(&self) -> Option<ModelSpec> {
        let mut list = self.list();
        list.sort_by(|a, b| {
            let a_reasoning = is_reasoning_name(&a.0.name);
            let b_reasoning = is_reasoning_name(&b.0.name);
            b_reasoning
                .cmp(&a_reasoning)
                .then_with(|| b.0.size_bytes.cmp(&a.0.size_bytes))
                .then_with(|| a.0.name.cmp(&b.0.name))
        });
        list.into_iter().next().map(|(s, _)| s)
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Whether the path has a `.gguf` extension, case-insensitively.
fn is_gguf(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
}

/// Whether the file looks like a partial upload rather than a finished model.
fn is_incomplete(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return true;
    };
    name.starts_with('.')
        || name.starts_with('~')
        || name.to_ascii_lowercase().ends_with(".part")
        || name.to_ascii_lowercase().ends_with(".tmp")
        || name.to_ascii_lowercase().ends_with(".download")
}

/// Backwards-compatible alias used by callers that still reference the old name.
#[must_use]
pub fn default_specs() -> Vec<ModelSpec> {
    Vec::new()
}

/// Whether the file name suggests a reasoning/thinking model.
fn is_reasoning_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("reasoning")
        || lower.contains("thinking")
        || lower.contains("-r1")
        || lower.contains("r1-")
        || lower.contains("deepseek-r1")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let unique = format!(
            "veritas-models-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn lists_only_complete_gguf_files() {
        // Arrange
        let dir = temp_dir("list");
        fs::write(dir.join("good.gguf"), b"GGUF....").unwrap();
        fs::write(dir.join("upper.GGUF"), b"GGUF....").unwrap();
        fs::write(dir.join("empty.gguf"), b"").unwrap();
        fs::write(dir.join("partial.gguf.part"), b"GGUF").unwrap();
        fs::write(dir.join("notes.txt"), b"GGUF").unwrap();
        let manager = ModelManager::from_dir(dir.clone());

        // Act
        let ids: Vec<String> = manager.list().into_iter().map(|(s, _)| s.id).collect();

        // Assert
        assert!(ids.contains(&"good".to_string()));
        assert!(ids.contains(&"upper".to_string()));
        assert!(!ids.contains(&"empty".to_string()));
        assert!(!ids.contains(&"partial".to_string()));
        assert!(!ids.contains(&"notes".to_string()));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn resolves_upper_case_extension_to_real_file() {
        // Arrange
        let dir = temp_dir("resolve");
        let path = dir.join("model.GGUF");
        fs::write(&path, b"GGUF....").unwrap();
        let manager = ModelManager::from_dir(dir.clone());

        // Act
        let resolved = manager.model_path("model");

        // Assert
        assert_eq!(resolved, path);
        assert_eq!(manager.status("model"), ModelStatus::Ready);
        assert_eq!(manager.status("absent"), ModelStatus::Missing);

        let _ = fs::remove_dir_all(dir);
    }
}
