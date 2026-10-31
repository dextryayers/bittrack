//! veritas-models: model download, verification, and cache management.
//!
//! Models are stored as plain GGUF files in a per-user cache directory. A small
//! sidecar marker records a successful SHA-256 verification so that listing the
//! catalog does not re-hash multi-gigabyte files on every call. Downloads resume
//! from the current file length with an HTTP `Range` request.

#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;
use tracing::info;

/// A model available for download.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    /// Unique id. Also the GGUF file stem in the cache.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Download URL.
    pub url: String,
    /// Expected SHA-256 of the finished file.
    pub sha256: String,
    /// Size in bytes.
    pub size_bytes: u64,
    /// Minimum recommended RAM in bytes.
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
    /// Partially downloaded or not yet hash-verified.
    Partial,
    /// Downloaded and verified.
    Ready,
    /// Downloaded but corrupt or larger than expected.
    Corrupt,
}

impl ModelStatus {
    /// Stable string form used in FFI JSON.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ModelStatus::Missing => "missing",
            ModelStatus::Partial => "partial",
            ModelStatus::Ready => "ready",
            ModelStatus::Corrupt => "corrupt",
        }
    }
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
    /// The caller aborted the download.
    #[error("download cancelled")]
    Cancelled,
    /// The finished file has the wrong size.
    #[error("incomplete download: expected {expected} bytes, got {actual}")]
    Incomplete {
        /// Expected size.
        expected: u64,
        /// Actual size.
        actual: u64,
    },
}

/// Default model id used when nothing else is configured.
pub const DEFAULT_MODEL_ID: &str = "qwen2.5-3b-instruct-q4_k_m";

/// Return the built-in model catalog.
///
/// All entries are real GGUF blobs from the public Ollama registry; the digest
/// in the URL is the blob SHA-256 and matches [`ModelSpec::sha256`].
#[must_use]
pub fn default_specs() -> Vec<ModelSpec> {
    let blob = |digest: &str| {
        format!("https://registry.ollama.ai/v2/library/qwen2.5/blobs/sha256:{digest}")
    };
    vec![
        ModelSpec {
            id: DEFAULT_MODEL_ID.into(),
            name: "Qwen 2.5 3B Instruct (Q4_K_M)".into(),
            url: blob("5ee4f07cdb9beadbbb293e85803c569b01bd37ed059d2715faa7bb405f31caa6"),
            sha256: "5ee4f07cdb9beadbbb293e85803c569b01bd37ed059d2715faa7bb405f31caa6".into(),
            size_bytes: 1_929_903_008,
            min_ram_bytes: 4 * 1024 * 1024 * 1024,
            tier: ModelTier::Lightweight,
        },
        ModelSpec {
            id: "qwen2.5-7b-instruct-q4_k_m".into(),
            name: "Qwen 2.5 7B Instruct (Q4_K_M)".into(),
            url: blob("2bada8a7450677000f678be90653b85d364de7db25eb5ea54136ada5f3933730"),
            sha256: "2bada8a7450677000f678be90653b85d364de7db25eb5ea54136ada5f3933730".into(),
            size_bytes: 4_683_073_952,
            min_ram_bytes: 8 * 1024 * 1024 * 1024,
            tier: ModelTier::Standard,
        },
        ModelSpec {
            id: "qwen2.5-1.5b-instruct-q4_k_m".into(),
            name: "Qwen 2.5 1.5B Instruct (Q4_K_M)".into(),
            url: blob("183715c435899236895da3869489cc30ac241476b4971a20285b1a462818a5b4"),
            sha256: "183715c435899236895da3869489cc30ac241476b4971a20285b1a462818a5b4".into(),
            size_bytes: 986_048_512,
            min_ram_bytes: 2 * 1024 * 1024 * 1024,
            tier: ModelTier::Tiny,
        },
        ModelSpec {
            id: "qwen2.5-0.5b-instruct-q4_k_m".into(),
            name: "Qwen 2.5 0.5B Instruct (Q4_K_M)".into(),
            url: blob("c5396e06af294bd101b30dce59131a76d2b773e76950acc870eda801d3ab0515"),
            sha256: "c5396e06af294bd101b30dce59131a76d2b773e76950acc870eda801d3ab0515".into(),
            size_bytes: 397_807_936,
            min_ram_bytes: 1024 * 1024 * 1024,
            tier: ModelTier::Tiny,
        },
    ]
}

/// Manages the local model cache.
pub struct ModelManager {
    cache_dir: PathBuf,
    specs: Vec<ModelSpec>,
}

impl ModelManager {
    /// Create a manager with the given cache dir and known specs.
    #[must_use]
    pub fn new(cache_dir: PathBuf, specs: Vec<ModelSpec>) -> Self {
        Self { cache_dir, specs }
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

    /// Path of the verification marker for a model.
    fn marker_path(&self, id: &str) -> PathBuf {
        self.cache_dir.join(format!("{id}.gguf.sha256"))
    }

    /// Status of a model.
    ///
    /// A same-size file without a marker is hashed once; the result is cached in
    /// the marker so subsequent calls are O(1).
    #[must_use]
    pub fn status(&self, id: &str) -> ModelStatus {
        let Some(spec) = self.specs.iter().find(|s| s.id == id) else {
            return ModelStatus::Missing;
        };
        let path = self.model_path(id);
        let Ok(meta) = std::fs::metadata(&path) else {
            return ModelStatus::Missing;
        };
        if meta.len() > spec.size_bytes {
            return ModelStatus::Corrupt;
        }
        if meta.len() < spec.size_bytes {
            return ModelStatus::Partial;
        }
        if marker_matches(&self.marker_path(id), &spec.sha256) {
            return ModelStatus::Ready;
        }
        match sha256_file(&path) {
            Ok(actual) if actual.eq_ignore_ascii_case(&spec.sha256) => {
                let _ = write_marker(&self.marker_path(id), &spec.sha256);
                ModelStatus::Ready
            }
            Ok(_) => ModelStatus::Corrupt,
            Err(_) => ModelStatus::Corrupt,
        }
    }

    /// List all known models with status.
    #[must_use]
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
    #[must_use]
    pub fn spec(&self, id: &str) -> Option<&ModelSpec> {
        self.specs.iter().find(|s| s.id == id)
    }

    /// The best installed model, preferring higher tiers and larger files.
    #[must_use]
    pub fn best_installed(&self) -> Option<ModelSpec> {
        let rank = |tier: ModelTier| match tier {
            ModelTier::Standard => 0,
            ModelTier::Lightweight => 1,
            ModelTier::Tiny => 2,
            ModelTier::Embedding => 3,
        };
        self.specs
            .iter()
            .filter(|s| self.status(&s.id) == ModelStatus::Ready)
            .cloned()
            .min_by(|a, b| {
                rank(a.tier)
                    .cmp(&rank(b.tier))
                    .then_with(|| b.size_bytes.cmp(&a.size_bytes))
            })
    }

    /// Download a model, resuming a partial file when possible.
    ///
    /// `on_progress` receives `(downloaded, total)` and returns `false` to abort.
    ///
    /// # Errors
    /// Returns [`ModelError`] on network failure, hash mismatch, or cancellation.
    pub fn download<F>(&self, id: &str, mut on_progress: F) -> Result<PathBuf, ModelError>
    where
        F: FnMut(u64, u64) -> bool,
    {
        let spec = self
            .spec(id)
            .ok_or_else(|| ModelError::NotFound(id.to_string()))?
            .clone();
        std::fs::create_dir_all(&self.cache_dir)?;
        let path = self.model_path(id);
        let marker = self.marker_path(id);

        if self.status(id) == ModelStatus::Ready {
            on_progress(spec.size_bytes, spec.size_bytes);
            return Ok(path);
        }
        let existing = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if existing > spec.size_bytes {
            std::fs::remove_file(&path)?;
        }
        let _ = std::fs::remove_file(&marker);
        let offset = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!(
                "Veritas/",
                env!("CARGO_PKG_VERSION"),
                " (local fact checker; model download)"
            ))
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(1800))
            .build()?;
        let mut request = client.get(&spec.url);
        if offset > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
        }
        let mut response = request.send()?.error_for_status()?;

        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&path)?;
        let resumed = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        let mut downloaded = if resumed {
            offset
        } else {
            // Server ignored the range request; restart from zero.
            file.set_len(0)?;
            file.seek(SeekFrom::Start(0))?;
            0
        };
        let total = if resumed {
            response
                .content_length()
                .map(|len| len + offset)
                .unwrap_or(spec.size_bytes)
        } else {
            response
                .content_length()
                .filter(|len| *len > 0)
                .unwrap_or(spec.size_bytes)
        };
        if !on_progress(downloaded, total) {
            return Err(ModelError::Cancelled);
        }

        let mut buffer = vec![0u8; 256 * 1024];
        loop {
            let read = response.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read])?;
            downloaded += read as u64;
            if !on_progress(downloaded, total) {
                file.flush()?;
                return Err(ModelError::Cancelled);
            }
        }
        file.flush()?;
        file.sync_all()?;
        drop(file);

        let actual_size = std::fs::metadata(&path)?.len();
        if actual_size != spec.size_bytes {
            return Err(ModelError::Incomplete {
                expected: spec.size_bytes,
                actual: actual_size,
            });
        }
        let actual = sha256_file(&path)?;
        if !actual.eq_ignore_ascii_case(&spec.sha256) {
            let _ = std::fs::remove_file(&path);
            return Err(ModelError::HashMismatch {
                expected: spec.sha256,
                actual,
            });
        }
        write_marker(&marker, &spec.sha256)?;
        info!(model = %id, bytes = actual_size, "model downloaded and verified");
        Ok(path)
    }
}

fn marker_matches(path: &Path, expected: &str) -> bool {
    std::fs::read_to_string(path)
        .map(|value| value.trim().eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

fn write_marker(path: &Path, sha256: &str) -> Result<(), std::io::Error> {
    std::fs::write(path, sha256.trim().to_ascii_lowercase())
}

fn sha256_file(path: &Path) -> Result<String, std::io::Error> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
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
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use tempfile::tempdir;

    fn spec_for(content: &[u8]) -> ModelSpec {
        let mut hasher = Sha256::new();
        hasher.update(content);
        ModelSpec {
            id: "test-model".into(),
            name: "Test".into(),
            url: String::new(),
            sha256: hex(hasher.finalize().as_slice()),
            size_bytes: content.len() as u64,
            min_ram_bytes: 0,
            tier: ModelTier::Tiny,
        }
    }

    #[test]
    fn catalog_entries_are_consistent() {
        let specs = default_specs();
        assert!(specs.len() >= 4);
        for spec in &specs {
            assert!(spec.url.starts_with("https://registry.ollama.ai/"));
            assert_eq!(spec.sha256.len(), 64);
            assert!(spec.url.ends_with(&spec.sha256));
            assert!(spec.size_bytes > 0);
        }
        let ids: std::collections::BTreeSet<_> = specs.iter().map(|s| s.id.clone()).collect();
        assert_eq!(ids.len(), specs.len());
    }

    #[test]
    fn status_tracks_size_and_hash() {
        let dir = tempdir().unwrap();
        let content = b"GGUF-fake-bytes";
        let spec = spec_for(content);
        let mgr = ModelManager::new(dir.path().to_path_buf(), vec![spec.clone()]);

        assert_eq!(mgr.status(&spec.id), ModelStatus::Missing);

        std::fs::write(mgr.model_path(&spec.id), &content[..4]).unwrap();
        assert_eq!(mgr.status(&spec.id), ModelStatus::Partial);

        std::fs::write(mgr.model_path(&spec.id), content).unwrap();
        assert_eq!(mgr.status(&spec.id), ModelStatus::Ready);
        assert!(mgr.marker_path(&spec.id).exists());

        std::fs::write(mgr.model_path(&spec.id), b"corrupt-content").unwrap();
        assert_eq!(mgr.status(&spec.id), ModelStatus::Corrupt);
    }

    #[test]
    fn best_installed_prefers_higher_tier() {
        let dir = tempdir().unwrap();
        let small = spec_for(b"tiny");
        let mut standard = spec_for(b"standard-bytes");
        standard.id = "standard".into();
        standard.tier = ModelTier::Standard;
        let mut lightweight = spec_for(b"light");
        lightweight.id = "light".into();
        lightweight.tier = ModelTier::Lightweight;
        let mgr = ModelManager::new(
            dir.path().to_path_buf(),
            vec![lightweight.clone(), standard.clone(), small],
        );
        assert!(mgr.best_installed().is_none());

        std::fs::write(mgr.model_path(&lightweight.id), b"light").unwrap();
        std::fs::write(
            mgr.marker_path(&lightweight.id),
            lightweight.sha256.clone(),
        )
        .unwrap();
        assert_eq!(mgr.best_installed().unwrap().id, lightweight.id);

        std::fs::write(mgr.model_path(&standard.id), b"standard-bytes").unwrap();
        std::fs::write(mgr.marker_path(&standard.id), standard.sha256.clone()).unwrap();
        assert_eq!(mgr.best_installed().unwrap().id, standard.id);
    }

    fn serve_once(listener: TcpListener, content: Vec<u8>) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            let mut range_start: Option<usize> = None;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                    range_start = value
                        .split('-')
                        .next()
                        .and_then(|v| v.trim().parse::<usize>().ok());
                }
            }
            let start = range_start.unwrap_or(0).min(content.len());
            let body = &content[start..];
            let mut writer = stream;
            if start > 0 {
                write!(
                    writer,
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
                    body.len(),
                    start,
                    content.len() - 1,
                    content.len()
                )
                .unwrap();
            } else {
                write!(
                    writer,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
            }
            writer.write_all(body).unwrap();
            writer.flush().unwrap();
        })
    }

    #[test]
    fn download_resumes_from_partial_file() {
        let dir = tempdir().unwrap();
        let content: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let mut spec = spec_for(&content);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        spec.url = format!("http://127.0.0.1:{port}/model.gguf");
        let mgr = ModelManager::new(dir.path().to_path_buf(), vec![spec.clone()]);
        std::fs::write(mgr.model_path(&spec.id), &content[..123_456]).unwrap();

        let server = serve_once(listener, content.clone());
        let mut progress = Vec::new();
        let path = mgr
            .download(&spec.id, |done, total| {
                progress.push((done, total));
                true
            })
            .unwrap();
        server.join().unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), content);
        assert_eq!(mgr.status(&spec.id), ModelStatus::Ready);
        assert_eq!(progress.last().unwrap().0, content.len() as u64);
    }

    #[test]
    fn download_can_be_cancelled_and_resumes_later() {
        let dir = tempdir().unwrap();
        let content: Vec<u8> = (0..200_000u32).map(|i| (i % 97) as u8).collect();
        let mut spec = spec_for(&content);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        spec.url = format!("http://127.0.0.1:{port}/model.gguf");
        let mgr = ModelManager::new(dir.path().to_path_buf(), vec![spec.clone()]);

        let server = serve_once(listener, content.clone());
        let error = mgr.download(&spec.id, |_, _| false).unwrap_err();
        let _ = server.join();
        assert!(matches!(error, ModelError::Cancelled));
        assert_eq!(mgr.status(&spec.id), ModelStatus::Partial);
    }
}
