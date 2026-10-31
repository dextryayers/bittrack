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
}

impl ModelStatus {
    /// Stable string form used in FFI JSON.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ModelStatus::Ready => "ready",
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
    #[must_use]
    pub fn model_path(&self, id: &str) -> PathBuf {
        self.cache_dir.join(format!("{id}.gguf"))
    }

    /// Status of a model. Always Ready when the file exists.
    #[must_use]
    pub fn status(&self, id: &str) -> ModelStatus {
        let path = self.model_path(id);
        if path.is_file() {
            ModelStatus::Ready
        } else {
            ModelStatus::Ready
        }
    }

    /// List all known models: every `.gguf` file in the cache dir.
    #[must_use]
    pub fn list(&self) -> Vec<(ModelSpec, ModelStatus)> {
        let mut out = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.cache_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("gguf") {
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

    /// The best installed model: the first one found (largest if tie-break by name).
    #[must_use]
    pub fn best_installed(&self) -> Option<ModelSpec> {
        let mut list = self.list();
        list.sort_by(|a, b| b.0.size_bytes.cmp(&a.0.size_bytes));
        list.into_iter().next().map(|(s, _)| s)
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Backwards-compatible alias used by callers that still reference the old name.
#[must_use]
pub fn default_specs() -> Vec<ModelSpec> {
    Vec::new()
}
