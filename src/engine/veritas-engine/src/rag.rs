//! Retrieval-augmented generation: local fact store and embedding.

use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::PathBuf;
use veritas_core::Evidence;

const EMBED_DIM: usize = 256;

/// Embedding backend.
pub trait Embedder: Send + Sync {
    /// Embed text into a fixed-dimension vector.
    fn embed(&self, text: &str) -> Vec<f32>;
    /// Dimension.
    fn dim(&self) -> usize {
        EMBED_DIM
    }
}

/// Deterministic hashing embedder (bag of character n-grams). Offline, no model.
#[derive(Debug, Clone, Copy)]
pub struct HashingEmbedder {
    dim: usize,
}

impl HashingEmbedder {
    /// Create with given dimension.
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }
}

impl Default for HashingEmbedder {
    fn default() -> Self {
        Self { dim: EMBED_DIM }
    }
}

impl Embedder for HashingEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0.0f32; self.dim];
        let lower = text.to_lowercase();
        let chars: Vec<char> = lower.chars().collect();
        for n in 3..=5usize {
            if chars.len() < n {
                continue;
            }
            for i in 0..=chars.len() - n {
                let gram: String = chars[i..i + n].iter().collect();
                let mut h: u64 = 5381;
                for b in gram.bytes() {
                    h = h.wrapping_mul(33).wrapping_add(b as u64);
                }
                let idx = (h as usize) % self.dim;
                v[idx] += 1.0;
            }
        }
        // L2 normalize
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut v {
                *x /= norm;
            }
        }
        v
    }

    fn dim(&self) -> usize {
        self.dim
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    let denom = (na.sqrt() * nb.sqrt()).max(1e-9);
    dot / denom
}

/// A stored fact claim.
#[derive(Debug, Clone, Serialize)]
pub struct FactClaim {
    /// Id.
    pub id: i64,
    /// Claim text.
    pub text: String,
    /// Verdict label.
    pub verdict: String,
    /// Source name.
    pub source: String,
    /// Source URL.
    pub url: String,
    /// Date.
    pub date: String,
}

/// Local fact database backed by SQLite.
pub struct FactDb {
    conn: Connection,
    embedder: Box<dyn Embedder>,
}

impl FactDb {
    /// Open or create a database at the given path.
    pub fn open(path: &std::path::Path, embedder: Box<dyn Embedder>) -> Result<Self, rusqlite::Error> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        let db = Self { conn, embedder };
        db.init_schema()?;
        Ok(db)
    }

    /// Open an in-memory database (tests/dev).
    pub fn open_in_memory(embedder: Box<dyn Embedder>) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn, embedder };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<(), rusqlite::Error> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS facts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                text TEXT NOT NULL,
                verdict TEXT NOT NULL,
                source TEXT NOT NULL,
                url TEXT NOT NULL DEFAULT '',
                date TEXT NOT NULL DEFAULT '',
                embedding BLOB NOT NULL
            );",
        )?;
        Ok(())
    }

    /// Number of stored facts.
    pub fn count(&self) -> usize {
        self.conn
            .query_row("SELECT COUNT(*) FROM facts", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0) as usize
    }

    /// Insert a fact.
    pub fn insert(
        &self,
        text: &str,
        verdict: &str,
        source: &str,
        url: &str,
        date: &str,
    ) -> Result<i64, rusqlite::Error> {
        let emb = self.embedder.embed(text);
        let blob = f32s_to_bytes(&emb);
        self.conn.execute(
            "INSERT INTO facts (text, verdict, source, url, date, embedding) VALUES (?1,?2,?3,?4,?5,?6)",
            params![text, verdict, source, url, date, blob],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Search top-k by cosine similarity.
    pub fn search(&self, query: &str, top_k: usize) -> Vec<(FactClaim, f32)> {
        let qemb = self.embedder.embed(query);
        let mut stmt = match self
            .conn
            .prepare("SELECT id, text, verdict, source, url, date, embedding FROM facts")
        {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Vec<u8>>(6)?,
            ))
        });
        let mut results: Vec<(FactClaim, f32)> = Vec::new();
        if let Ok(rows) = rows {
            for r in rows.flatten() {
                let emb = bytes_to_f32s(&r.6);
                if emb.len() != qemb.len() {
                    continue;
                }
                let score = cosine(&qemb, &emb);
                results.push((
                    FactClaim {
                        id: r.0,
                        text: r.1,
                        verdict: r.2,
                        source: r.3,
                        url: r.4,
                        date: r.5,
                    },
                    score,
                ));
            }
        }
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        results
    }

    /// Seed with a list of (text, verdict, source, url, date).
    pub fn seed(&self, items: &[(&str, &str, &str, &str, &str)]) -> usize {
        let mut n = 0;
        for (t, v, s, u, d) in items {
            if self.insert(t, v, s, u, d).is_ok() {
                n += 1;
            }
        }
        n
    }
}

fn f32s_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

fn bytes_to_f32s(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Default cache/db path.
pub fn default_db_path() -> PathBuf {
    if cfg!(target_os = "windows") {
        std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("Veritas")
            .join("factdb.sqlite")
    } else if cfg!(target_os = "macos") {
        dirs_home().join("Library/Application Support/Veritas/factdb.sqlite")
    } else {
        dirs_home().join(".local/share/veritas/factdb.sqlite")
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Built-in seed claims (Indonesian). Small, for offline RAG baseline.
pub fn builtin_seed() -> Vec<(&'static str, &'static str, &'static str, &'static str, &'static str)> {
    vec![
        ("Vaksin COVID-19 mengandung microchip untuk pelacakan", "hoax", "turnbackhoax", "https://turnbackhoax.id", "2021-01-01"),
        ("Bawang putih bisa sembuhkan COVID-19 dalam 5 menit", "hoax", "turnbackhoax", "https://turnbackhoax.id", "2020-03-01"),
        ("Pemerintah akan memblokir WhatsApp mulai besok", "hoax", "mafino", "https://mafindo.or.id", "2022-01-01"),
        ("Gempa bumi di Jakarta diprediksi akan terjadi minggu depan", "hoax", "kompas", "https://www.kompas.com/cekfakta", "2023-01-01"),
        ("Presiden mengumumkan libur nasional tambahan", "valid", "kompas", "https://www.kompas.com", "2023-06-01"),
        ("Bank Indonesia menaikkan suku bunga acuan", "valid", "antaranews", "https://www.antaranews.com", "2023-07-01"),
        ("Kementerian Kesehatan merilis data vaksinasi terbaru", "valid", "kemenkes", "https://www.kemkes.go.id", "2023-08-01"),
        ("Berita viral ini wajib dibagikan ke semua grup WhatsApp", "misleading", "turnbackhoax", "https://turnbackhoax.id", "2022-05-01"),
        ("Share ke 10 orang agar tidak sial", "misleading", "turnbackhoax", "https://turnbackhoax.id", "2022-06-01"),
        ("Viralkan sebelum dihapus pemerintah", "misleading", "mafino", "https://mafindo.or.id", "2022-07-01"),
    ]
}
