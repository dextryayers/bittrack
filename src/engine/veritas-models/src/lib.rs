//! veritas-models: model download, verification, and cache management.

#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::info;

/// A model available for download.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    /// Unique id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Download URL.
    pub url: String,
    /// Expected SHA-256.
    pub sha256: String,
    /// Size in bytes.
    pub size_bytes: u64,
    /// Minimum RAM in bytes.
    pub min_ram_bytes: u64,
    /// Tier.
    pub tier: ModelTier,
}

/// Model tier.
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
    /// Not downloaded.
    Missing,
    /// Downloaded and verified.
    Ready,
    /// Downloaded but hash mismatch.
    Corrupt,
}

/// Errors from model management.
#[derive(Debug, Error)]
pub enum ModelError {
    /// I/O failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// HTTP failure.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    /// Hash mismatch.
    #[error("hash mismatch: expected {expected}, got {actual}")]
    HashMismatch {
        /// Expected.
        expected: String,
        /// Actual.
        actual: String,
    },
    /// Model not found.
    #[error("model not found: {0}")]
    NotFound(String),
}

/// Manages the local model cache.
pub struct ModelManager {
    cache_dir: PathBuf,
    specs: Vec<ModelSpec>,
}

/// Return the built-in model catalog (URLs are placeholders until wired).
pub fn default_specs() -> Vec<ModelSpec> {
    vec![
        ModelSpec {
            id: "qwen2.5-7b-q4".into(),
            name: "Qwen 2.5 7B Instruct (Q4)".into(),
            url: "https://example.invalid/qwen2.5-7b-q4.gguf".into(),
            sha256: "0".repeat(64),
            size_bytes: 4_700_000_000,
            min_ram_bytes: 8 * 1024 * 1024 * 1024,
            tier: ModelTier::Standard,
        },
        ModelSpec {
            id: "qwen2.5-3b-q4".into(),
            name: "Qwen 2.5 3B Instruct (Q4)".into(),
            url: "https://example.invalid/qwen2.5-3b-q4.gguf".into(),
            sha256: "0".repeat(64),
            size_bytes: 2_000_000_000,
            min_ram_bytes: 4 * 1024 * 1024 * 1024,
            tier: ModelTier::Lightweight,
        },
    ]
}

impl ModelManager {
    /// Create a manager with the given cache dir and known specs.
    pub fn new(cache_dir: PathBuf, specs: Vec<ModelSpec>) -> Self {
        Self { cache_dir, specs }
    }

    /// Default cache dir for the current OS.
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

    /// Path where a model is stored.
    pub fn model_path(&self, id: &str) -> PathBuf {
        self.cache_dir.join(format!("{id}.gguf"))
    }

    /// Status of a model.
    pub fn status(&self, id: &str) -> ModelStatus {
        let path = self.model_path(id);
        if !path.exists() {
            return ModelStatus::Missing;
        }
        let spec = match self.specs.iter().find(|s| s.id == id) {
            Some(s) => s,
            None => return ModelStatus::Missing,
        };
        match verify_sha256(&path, &spec.sha256) {
            Ok(true) => ModelStatus::Ready,
            Ok(false) => ModelStatus::Corrupt,
            Err(_) => ModelStatus::Corrupt,
        }
    }

    /// List all known models with status.
    pub fn list(&self) -> Vec<(ModelSpec, ModelStatus)> {
        self.specs
            .iter()
            .cloned()
            .map(|s| {
                let st = self.status(&s.id);
                (s, st)
            })
            .collect()
    }

    /// Find a spec by id.
    pub fn spec(&self, id: &str) -> Option<&ModelSpec> {
        self.specs.iter().find(|s| s.id == id)
    }

    /// Download a model with progress callback. Blocking.
    pub async fn download<F>(&self, id: &str, mut on_progress: F) -> Result<PathBuf, ModelError>
    where
        F: FnMut(u64, u64) + Send,
    {
        let spec = self
            .spec(id)
            .ok_or_else(|| ModelError::NotFound(id.to_string()))?
            .clone();
        std::fs::create_dir_all(&self.cache_dir)?;
        let path = self.model_path(id);
        let client = reqwest::Client::new();
        let resp = client.get(&spec.url).send().await?.error_for_status()?;
        let total = resp.content_length().unwrap_or(spec.size_bytes);
        let mut file = std::fs::File::create(&path)?;
        use futures_util::StreamExt;
        use std::io::Write;
        let mut stream = resp.bytes_stream();
        let mut downloaded: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk)?;
            downloaded += chunk.len() as u64;
            on_progress(downloaded, total);
        }
        file.flush()?;
        // verify
        if !verify_sha256(&path, &spec.sha256)? {
            return Err(ModelError::HashMismatch {
                expected: spec.sha256,
                actual: sha256_file(&path)?,
            });
        }
        info!(model = %id, "model downloaded and verified");
        Ok(path)
    }
}

fn verify_sha256(path: &Path, expected: &str) -> Result<bool, std::io::Error> {
    let actual = sha256_file(path)?;
    Ok(actual.eq_ignore_ascii_case(expected))
}

fn sha256_file(path: &Path) -> Result<String, std::io::Error> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(hasher.finalize().as_slice()))
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn status_missing() {
        let dir = tempdir().unwrap();
        let mgr = ModelManager::new(dir.path().to_path_buf(), vec![]);
        // no specs, but path check
        assert_eq!(
            mgr.model_path("x").file_name().unwrap().to_str().unwrap(),
            "x.gguf"
        );
    }
}
