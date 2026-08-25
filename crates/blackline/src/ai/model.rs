//! Opinionated default model, plus an override.

use std::path::{Path, PathBuf};

use super::error::AiError;

/// Default: Kalosm's quantized Phi-3.5 mini (reasoning, fits a 16 GB MacBook).
pub const DEFAULT_MODEL: &str = "phi-3.5";

/// A named preset or a local GGUF file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ModelId {
    /// `LlamaSource::phi_3_5_mini_4k_instruct` (default).
    #[default]
    Phi35,
    /// `LlamaSource::phi_3_mini_4k_instruct`.
    Phi3,
    /// Llama 3.2 1B Instruct — smallest preset.
    Llama32_1b,
    /// Llama 3.2 3B Instruct.
    Llama32_3b,
    /// Llama 3.1 8B Instruct — wants a GPU.
    Llama31_8b,
    /// Qwen2.5 1.5B Instruct.
    Qwen25_15b,
    /// Qwen2.5 3B Instruct.
    Qwen25_3b,
    /// Qwen2.5 7B Instruct — wants a GPU.
    Qwen25_7b,
    /// TinyLlama 1.1B Chat.
    TinyLlama,
    /// A GGUF on disk (`--model ./path.gguf`).
    Gguf(PathBuf),
}

impl ModelId {
    /// Parse a `--model` value. A path that exists or ends in `.gguf` is a
    /// local file; everything else must be a known preset name.
    pub fn parse(raw: &str) -> Result<Self, AiError> {
        let t = raw.trim();
        if t.is_empty() {
            return Err(AiError::usage(
                "pass --model NAME or a .gguf path".to_string(),
            ));
        }
        let path = Path::new(t);
        if t.ends_with(".gguf") || path.exists() {
            return Ok(Self::Gguf(path.to_path_buf()));
        }
        Ok(match t {
            "phi-3.5" | "phi3.5" | "default" => Self::Phi35,
            "phi-3" | "phi3" => Self::Phi3,
            "llama3.2-1b" | "llama-3.2-1b" => Self::Llama32_1b,
            "llama3.2-3b" | "llama-3.2-3b" => Self::Llama32_3b,
            "llama3.1-8b" | "llama-3.1-8b" => Self::Llama31_8b,
            "qwen2.5-1.5b" | "qwen-2.5-1.5b" => Self::Qwen25_15b,
            "qwen2.5-3b" | "qwen-2.5-3b" => Self::Qwen25_3b,
            "qwen2.5-7b" | "qwen-2.5-7b" => Self::Qwen25_7b,
            "tinyllama" | "tiny-llama" => Self::TinyLlama,
            other => {
                return Err(AiError::usage(format!(
                    "unknown model {other:?}. presets: {}; or pass a .gguf path",
                    Self::presets().join(", ")
                )));
            }
        })
    }

    /// Names accepted by [`Self::parse`].
    pub fn presets() -> &'static [&'static str] {
        &[
            "phi-3.5",
            "phi-3",
            "llama3.2-1b",
            "llama3.2-3b",
            "llama3.1-8b",
            "qwen2.5-1.5b",
            "qwen2.5-3b",
            "qwen2.5-7b",
            "tinyllama",
        ]
    }

    /// Stable label for JSON reports.
    pub fn as_str(&self) -> String {
        match self {
            Self::Phi35 => "phi-3.5".into(),
            Self::Phi3 => "phi-3".into(),
            Self::Llama32_1b => "llama3.2-1b".into(),
            Self::Llama32_3b => "llama3.2-3b".into(),
            Self::Llama31_8b => "llama3.1-8b".into(),
            Self::Qwen25_15b => "qwen2.5-1.5b".into(),
            Self::Qwen25_3b => "qwen2.5-3b".into(),
            Self::Qwen25_7b => "qwen2.5-7b".into(),
            Self::TinyLlama => "tinyllama".into(),
            Self::Gguf(p) => p.display().to_string(),
        }
    }
}

/// Load the model and wrap it as a [`super::plan::Completer`].
///
/// Kalosm's `Llama` is a channel handle. The quantized weights and Metal /
/// CUDA buffers live on a worker thread. Dropping the last handle closes
/// the channel; the worker exits and `Drop`s the tensors. There is no
/// unload API. The GGUF file stays in [`super::cache::cache_dir`].
#[cfg(feature = "kalosm")]
pub async fn load(id: &ModelId, verbose: bool) -> Result<KalosmCompleter, AiError> {
    use kalosm::language::{Cache, FileSource, Llama, LlamaSource};

    let source = match id {
        ModelId::Phi35 => LlamaSource::phi_3_5_mini_4k_instruct(),
        ModelId::Phi3 => LlamaSource::phi_3_mini_4k_instruct(),
        ModelId::Llama32_1b => LlamaSource::llama_3_2_1b_chat(),
        ModelId::Llama32_3b => LlamaSource::llama_3_2_3b_chat(),
        ModelId::Llama31_8b => LlamaSource::llama_3_1_8b_chat(),
        ModelId::Qwen25_15b => LlamaSource::qwen_2_5_1_5b_instruct(),
        ModelId::Qwen25_3b => LlamaSource::qwen_2_5_3b_instruct(),
        ModelId::Qwen25_7b => LlamaSource::qwen_2_5_7b_instruct(),
        ModelId::TinyLlama => LlamaSource::tiny_llama_1_1b_chat(),
        ModelId::Gguf(path) => LlamaSource::new(FileSource::local(path.clone())),
    };
    let source = source.with_cache(Cache::new(super::cache::cache_dir()?));

    if verbose {
        eprintln!(
            "loading model {}  cache={}",
            id.as_str(),
            super::cache::cache_dir()?.display()
        );
    }
    let llama = Llama::builder()
        .with_source(source)
        .build()
        .await
        .map_err(|e| AiError::Model(format!("failed to load {}: {e}", id.as_str())))?;
    Ok(KalosmCompleter { llama })
}

/// Kalosm-backed completer: constrained generation into [`super::plan::Plan`].
#[cfg(feature = "kalosm")]
pub struct KalosmCompleter {
    llama: kalosm::language::Llama,
}

#[cfg(feature = "kalosm")]
impl super::plan::Completer for KalosmCompleter {
    async fn complete(
        &self,
        view: &super::view::DocumentView,
        instruction: &str,
    ) -> Result<super::plan::Plan, AiError> {
        use super::plan::{system_prompt, user_prompt, Plan};
        use kalosm::language::ChatModelExt;
        use std::sync::Arc;

        let task = self
            .llama
            .task(system_prompt(view.format))
            .with_constraints(Arc::new(Plan::new_parser()));
        let user = user_prompt(view, instruction);
        task(&user).await.map_err(|e| AiError::Model(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::ModelId;

    #[test]
    fn default_is_phi35() {
        assert_eq!(ModelId::parse("phi-3.5").unwrap(), ModelId::Phi35);
        assert_eq!(ModelId::parse("default").unwrap(), ModelId::Phi35);
    }

    #[test]
    fn gguf_path_wins() {
        match ModelId::parse("./models/foo.gguf").unwrap() {
            ModelId::Gguf(p) => assert!(p.ends_with("foo.gguf")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unknown_is_usage() {
        let err = ModelId::parse("gpt-4").unwrap_err();
        assert!(err.is_usage());
    }
}
