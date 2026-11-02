//! GGUF metadata inspection.
//!
//! Reads only the metadata header of a GGUF file. Tensor data is never loaded,
//! so inspecting a 5 GB model costs a few milliseconds and a small allocation.
//! This is what the desktop app calls before importing a file, and what backs
//! the metadata shown for installed models.

use std::path::Path;

use llama_cpp_2::gguf::GgufContext;
use serde::Serialize;

/// Result of inspecting a GGUF file.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ModelInspection {
    /// Whether the file is a readable GGUF decoder model.
    pub(crate) ok: bool,
    /// Machine-readable failure reason when `ok` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
    /// Absolute path that was inspected.
    pub(crate) path: String,
    /// File name including the `.gguf` extension.
    pub(crate) file_name: String,
    /// File size in bytes.
    pub(crate) size_bytes: u64,
    /// GGUF architecture key, for example `qwen2` or `llama`.
    pub(crate) architecture: String,
    /// Human-readable model name from the metadata.
    pub(crate) name: String,
    /// Size label from the metadata, for example `3B`.
    pub(crate) size_label: String,
    /// Trained context length in tokens, 0 when missing.
    pub(crate) context_length: u32,
    /// Embedding width, 0 when missing.
    pub(crate) embedding_length: u32,
    /// Transformer block count, 0 when missing.
    pub(crate) block_count: u32,
    /// Raw `general.file_type` value, 0 when missing.
    pub(crate) file_type: u32,
    /// Quantization label derived from `general.file_type`.
    pub(crate) quantization: String,
    /// Whether the file embeds a chat template.
    pub(crate) has_chat_template: bool,
    /// Whether the architecture is encoder-only / embedding-only.
    pub(crate) is_embedding: bool,
    /// Whether the engine can use this file for verdict generation.
    pub(crate) compatible: bool,
    /// Non-fatal concerns worth showing to the user.
    pub(crate) warnings: Vec<String>,
}

/// Inspect a GGUF file without loading weights.
#[must_use]
pub(crate) fn inspect_file(path: &Path) -> ModelInspection {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut warnings = Vec::new();

    let Some(ctx) = GgufContext::from_file(path) else {
        return ModelInspection {
            ok: false,
            error: Some("File bukan GGUF yang dapat dibaca.".into()),
            path: path.display().to_string(),
            file_name,
            size_bytes,
            architecture: String::new(),
            name: String::new(),
            size_label: String::new(),
            context_length: 0,
            embedding_length: 0,
            block_count: 0,
            file_type: 0,
            quantization: "Tidak diketahui".into(),
            has_chat_template: false,
            is_embedding: false,
            compatible: false,
            warnings,
        };
    };

    let architecture = value_str(&ctx, "general.architecture").unwrap_or_default();
    let name = value_str(&ctx, "general.name")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| file_name.trim_end_matches(".gguf").to_string());
    let size_label = value_str(&ctx, "general.size_label").unwrap_or_default();
    let context_length = value_u32(&ctx, &format!("{architecture}.context_length"));
    let embedding_length = value_u32(&ctx, &format!("{architecture}.embedding_length"));
    let block_count = value_u32(&ctx, &format!("{architecture}.block_count"));
    let file_type = value_u32(&ctx, "general.file_type");
    let quantization = quantization_name(file_type).to_string();
    let has_chat_template = value_str(&ctx, "tokenizer.chat_template").is_some();
    let is_embedding = is_embedding_architecture(&architecture);

    if architecture.is_empty() {
        warnings.push("Metadata arsitektur tidak ditemukan.".into());
    }
    if quantization == "Tidak diketahui" && file_type != 0 {
        warnings.push(format!(
            "Jenis kuantisasi (file_type {file_type}) belum dikenali; model tetap dapat dicoba."
        ));
    }
    if !has_chat_template {
        warnings.push(
            "Model tidak menyertakan chat template. Engine memakai fallback ChatML, sehingga \
             kepatuhan instruksi bisa menurun."
                .into(),
        );
    }
    if context_length > 0 && context_length < 4096 {
        warnings.push(format!(
            "Konteks model hanya {context_length} token; artikel panjang akan dipotong."
        ));
    }
    if is_embedding {
        warnings.push(
            "Arsitektur ini adalah model embedding/encoder, bukan model generatif; \
             model tidak dapat menghasilkan verdict."
                .into(),
        );
    }

    let compatible = !is_embedding && !architecture.is_empty() && size_bytes > 0;
    ModelInspection {
        ok: compatible,
        error: if compatible {
            None
        } else if architecture.is_empty() {
            Some("Metadata GGUF tidak lengkap.".into())
        } else if is_embedding {
            Some("Model embedding tidak didukung untuk analisis.".into())
        } else {
            Some("File model tidak dapat digunakan.".into())
        },
        path: path.display().to_string(),
        file_name,
        size_bytes,
        architecture,
        name,
        size_label,
        context_length,
        embedding_length,
        block_count,
        file_type,
        quantization,
        has_chat_template,
        is_embedding,
        compatible,
        warnings,
    }
}

/// Read a string metadata value by key.
pub(crate) fn value_str(ctx: &GgufContext, key: &str) -> Option<String> {
    let index = ctx.find_key(key);
    if index < 0 {
        return None;
    }
    // 8 is GGUF_TYPE_STRING.
    if ctx.kv_type(index) as i32 != 8 {
        return None;
    }
    ctx.val_str(index).map(str::to_string)
}

fn value_u32(ctx: &GgufContext, key: &str) -> u32 {
    let index = ctx.find_key(key);
    if index < 0 {
        return 0;
    }
    match ctx.kv_type(index) as i32 {
        // 4 = GGUF_TYPE_UINT32, 10 = GGUF_TYPE_UINT64.
        4 => ctx.val_u32(index),
        10 => u32::try_from(ctx.val_u64(index)).unwrap_or(u32::MAX),
        _ => 0,
    }
}

fn is_embedding_architecture(architecture: &str) -> bool {
    matches!(
        architecture,
        "bert"
            | "nomic-bert"
            | "nomic-bert-moe"
            | "jina-bert-v2"
            | "jina-bert-v3"
            | "bge-m3"
            | "modernbert"
            | "t5encoder"
            | "t5"
            | "clip"
            | "vit"
    )
}

/// Map llama.cpp's `llama_ftype` values that are unambiguous.
///
/// Unknown values are reported as unknown instead of being guessed. New
/// quantization types are added by llama.cpp over time; a wrong label is worse
/// than no label.
fn quantization_name(file_type: u32) -> &'static str {
    match file_type {
        0 => "F32",
        1 => "F16",
        2 => "Q4_0",
        3 => "Q4_1",
        7 => "Q8_0",
        8 => "Q5_0",
        9 => "Q5_1",
        10 => "Q2_K",
        11 => "Q3_K_S",
        12 => "Q3_K_M",
        13 => "Q3_K_L",
        14 => "Q4_K_S",
        15 => "Q4_K_M",
        16 => "Q5_K_S",
        17 => "Q5_K_M",
        18 => "Q6_K",
        19 => "IQ2_XXS",
        20 => "IQ2_XS",
        21 => "IQ3_XXS",
        22 => "IQ1_S",
        23 => "IQ4_NL",
        24 => "IQ3_S",
        25 => "IQ2_S",
        26 => "IQ4_XS",
        27 => "I8",
        28 => "I16",
        29 => "I32",
        30 => "I64",
        31 => "F64",
        32 => "IQ1_M",
        _ => "Tidak diketahui",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_gguf_file() {
        let path = std::env::temp_dir().join("veritas-not-a-model.gguf");
        std::fs::write(&path, b"not a gguf file").unwrap();
        let info = inspect_file(&path);
        assert!(!info.ok);
        assert!(!info.compatible);
        assert!(info.error.is_some());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn maps_known_quantization_only() {
        assert_eq!(quantization_name(15), "Q4_K_M");
        assert_eq!(quantization_name(18), "Q6_K");
        assert_eq!(quantization_name(99_999), "Tidak diketahui");
    }

    #[test]
    fn detects_embedding_architectures() {
        assert!(is_embedding_architecture("bert"));
        assert!(is_embedding_architecture("nomic-bert"));
        assert!(!is_embedding_architecture("qwen2"));
        assert!(!is_embedding_architecture("llama"));
    }
}
