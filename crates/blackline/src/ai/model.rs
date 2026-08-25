//! Opinionated default model, plus an override.

use std::path::{Path, PathBuf};

use super::error::AiError;

/// Default: Kalosm's quantized Phi-3 mini 4k. Phi-3.5's GGUF is 128k
/// context and Kalosm sizes RoPE / KV from that metadata — tens of GB
/// on Metal, then NaN logits in the sampler.
pub const DEFAULT_MODEL: &str = "phi-3";

/// A named preset or a local GGUF file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ModelId {
    /// `LlamaSource::phi_3_5_mini_4k_instruct`. The GGUF is 128k context.
    Phi35,
    /// `LlamaSource::phi_3_1_mini_4k_instruct` (default). Actually 4k.
    #[default]
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
    /// Parse a `--model` value. Preset names win, even if a same-named
    /// file exists in cwd (a Downloads/`phi-3` leftover must not steal
    /// the default). A path is only a GGUF if it ends in `.gguf` or is an
    /// existing file that is not a preset name.
    pub fn parse(raw: &str) -> Result<Self, AiError> {
        let t = raw.trim();
        if t.is_empty() {
            return Err(AiError::usage(
                "pass --model NAME or a .gguf path".to_string(),
            ));
        }
        if let Some(preset) = Self::from_preset(t) {
            return Ok(preset);
        }
        let path = Path::new(t);
        if t.ends_with(".gguf") || path.is_file() {
            return Ok(Self::Gguf(path.to_path_buf()));
        }
        Err(AiError::usage(format!(
            "unknown model {t:?}. presets: {}; or pass a .gguf path",
            Self::presets().join(", ")
        )))
    }

    fn from_preset(t: &str) -> Option<Self> {
        Some(match t {
            "phi-3.5" | "phi3.5" => Self::Phi35,
            "phi-3" | "phi3" | "default" => Self::Phi3,
            "llama3.2-1b" | "llama-3.2-1b" => Self::Llama32_1b,
            "llama3.2-3b" | "llama-3.2-3b" => Self::Llama32_3b,
            "llama3.1-8b" | "llama-3.1-8b" => Self::Llama31_8b,
            "qwen2.5-1.5b" | "qwen-2.5-1.5b" => Self::Qwen25_15b,
            "qwen2.5-3b" | "qwen-2.5-3b" => Self::Qwen25_3b,
            "qwen2.5-7b" | "qwen-2.5-7b" => Self::Qwen25_7b,
            "tinyllama" | "tiny-llama" => Self::TinyLlama,
            _ => return None,
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

/// Kalosm sizes RoPE / KV from GGUF `*.context_length`. Above this,
/// Metal routinely allocates tens of GB.
const SAFE_CONTEXT: u32 = 8192;

fn context_length_hint(id: &ModelId) -> Option<u32> {
    match id {
        ModelId::Phi35 => Some(131_072),
        ModelId::Phi3 => Some(4096),
        ModelId::TinyLlama => Some(2048),
        ModelId::Gguf(path) => peek_gguf_context_length(path),
        _ => None,
    }
}

fn refuse_long_context(id: &ModelId) -> Result<(), AiError> {
    if matches!(id, ModelId::Phi35) {
        eprintln!(
            "warning: phi-3.5 GGUF is 128k context; Kalosm sizes Metal caches \
             from that (tens of GB). Prefer --model phi-3 or --model tinyllama."
        );
        return Ok(());
    }
    let Some(n) = context_length_hint(id) else {
        return Ok(());
    };
    if n > SAFE_CONTEXT {
        return Err(AiError::Model(format!(
            "GGUF context_length is {n}. Kalosm allocates KV/RoPE from that \
             metadata — tens of GB on Metal, unrelated to how much RAM the \
             machine has. Use --model phi-3 (4k) or --model tinyllama (2k)."
        )));
    }
    Ok(())
}

/// First `*.context_length` that is not `original_context_length`.
fn peek_gguf_context_length(path: &Path) -> Option<u32> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut magic = [0_u8; 4];
    std::io::Read::read_exact(&mut file, &mut magic).ok()?;
    if &magic != b"GGUF" {
        return None;
    }
    let mut hdr = [0_u8; 20];
    std::io::Read::read_exact(&mut file, &mut hdr).ok()?;
    let n_kv = u64::from_le_bytes(hdr[12..20].try_into().ok()?);
    for _ in 0..n_kv {
        let key = read_gguf_string(&mut file)?;
        let ty = read_u32(&mut file)?;
        if key.ends_with(".context_length") && !key.ends_with("original_context_length") {
            return read_gguf_int(ty, &mut file);
        }
        skip_gguf_value(ty, &mut file)?;
    }
    None
}

fn read_u32(file: &mut std::fs::File) -> Option<u32> {
    let mut buf = [0_u8; 4];
    std::io::Read::read_exact(file, &mut buf).ok()?;
    Some(u32::from_le_bytes(buf))
}

fn read_u64(file: &mut std::fs::File) -> Option<u64> {
    let mut buf = [0_u8; 8];
    std::io::Read::read_exact(file, &mut buf).ok()?;
    Some(u64::from_le_bytes(buf))
}

fn read_gguf_string(file: &mut std::fs::File) -> Option<String> {
    let n = read_u64(file)? as usize;
    if n > 1_000_000 {
        return None;
    }
    let mut buf = vec![0_u8; n];
    std::io::Read::read_exact(file, &mut buf).ok()?;
    String::from_utf8(buf).ok()
}

fn read_gguf_int(ty: u32, file: &mut std::fs::File) -> Option<u32> {
    match ty {
        4 => read_u32(file),
        10 => read_u64(file).map(|n| n.min(u64::from(u32::MAX)) as u32),
        _ => {
            skip_gguf_value(ty, file)?;
            None
        }
    }
}

fn skip_gguf_value(ty: u32, file: &mut std::fs::File) -> Option<()> {
    match ty {
        0 | 1 | 7 => {
            let mut b = [0_u8; 1];
            std::io::Read::read_exact(file, &mut b).ok()
        }
        2 | 3 => {
            let mut b = [0_u8; 2];
            std::io::Read::read_exact(file, &mut b).ok()
        }
        4..=6 => {
            let mut b = [0_u8; 4];
            std::io::Read::read_exact(file, &mut b).ok()
        }
        8 => {
            read_gguf_string(file)?;
            Some(())
        }
        9 => {
            let elem = read_u32(file)?;
            let n = read_u64(file)?;
            for _ in 0..n {
                skip_gguf_value(elem, file)?;
            }
            Some(())
        }
        10..=12 => {
            let mut b = [0_u8; 8];
            std::io::Read::read_exact(file, &mut b).ok()
        }
        _ => None,
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
    use kalosm::language::{FileSource, Llama, LlamaSource};
    use kalosm_common::Cache;

    let source = match id {
        ModelId::Phi35 => LlamaSource::phi_3_5_mini_4k_instruct(),
        // Kalosm's `phi_3_mini_4k_instruct` pins a Hugging Face revision that
        // 404s. `phi_3_1_mini_4k_instruct` is the same 4k Q4 on `main`.
        ModelId::Phi3 => LlamaSource::phi_3_1_mini_4k_instruct(),
        ModelId::Llama32_1b => LlamaSource::llama_3_2_1b_chat(),
        ModelId::Llama32_3b => LlamaSource::llama_3_2_3b_chat(),
        ModelId::Llama31_8b => LlamaSource::llama_3_1_8b_chat(),
        ModelId::Qwen25_15b => LlamaSource::qwen_2_5_1_5b_instruct(),
        ModelId::Qwen25_3b => LlamaSource::qwen_2_5_3b_instruct(),
        ModelId::Qwen25_7b => LlamaSource::qwen_2_5_7b_instruct(),
        ModelId::TinyLlama => LlamaSource::tiny_llama_1_1b_chat(),
        ModelId::Gguf(path) => {
            let mut source = LlamaSource::new(FileSource::local(path.clone()));
            // Hugging Face GGUFs often omit the tokenizer. Pair a sibling
            // `tokenizer.json` when the user places one next to the weights.
            if let Some(parent) = path.parent() {
                let tokenizer = parent.join("tokenizer.json");
                if tokenizer.is_file() {
                    source = source.with_tokenizer(FileSource::local(tokenizer));
                }
            }
            source
        }
    };
    let source = source.with_cache(Cache::new(super::cache::cache_dir()?));
    refuse_long_context(id)?;

    let ctx = context_length_hint(id);
    let ctx_label = ctx.map(|n| format!("{n}")).unwrap_or_else(|| "?".into());
    eprintln!(
        "loading model {}  context={}  cache={}",
        id.as_str(),
        ctx_label,
        super::cache::cache_dir()?.display()
    );
    let _ = verbose;
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
        use super::plan::user_prompt;

        let user = user_prompt(view, instruction);

        // Metal + Kalosm structured generation often returns
        // "No valid tokens were sampled": constraint masking leaves only
        // NaN logits, then greedy has nothing to pick. That is a decoder
        // bug, not RAM. Generate JSON in the clear and parse it.
        #[cfg(feature = "metal")]
        {
            return complete_json(&self.llama, view, &user).await;
        }

        #[cfg(not(feature = "metal"))]
        {
            use super::plan::{system_prompt, Plan};
            use kalosm::language::{ChatModelExt, Parse};
            use llm_samplers::prelude::SampleGreedy;
            use std::sync::Arc;

            let task = self
                .llama
                .task(system_prompt(view.format))
                .with_constraints(Arc::new(Plan::new_parser()));
            match task.run(&user).with_sampler(SampleGreedy::new()).await {
                Ok(plan) => Ok(plan),
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("No valid tokens") || msg.contains("weight is invalid") {
                        complete_json(&self.llama, view, &user).await
                    } else {
                        Err(AiError::Model(map_model_error(msg)))
                    }
                }
            }
        }
    }
}

#[cfg(feature = "kalosm")]
async fn complete_json(
    llama: &kalosm::language::Llama,
    view: &super::view::DocumentView,
    user: &str,
) -> Result<super::plan::Plan, AiError> {
    use super::plan::{json_output_instruction, parse_plan_json, system_prompt};
    use kalosm::language::ChatModelExt;
    use llm_samplers::prelude::SampleGreedy;

    let system = format!(
        "{}{}",
        system_prompt(view.format),
        json_output_instruction()
    );
    let text: String = llama
        .task(system)
        .with_example(
            "# docx (1 lines)\n1| Hello world.\n# Instruction\nchange Hello to Hi",
            r#"{"ops":[{"op":"replace","index":1,"old":"Hello","new":"Hi"}]}"#,
        )
        .run(user)
        .with_sampler(SampleGreedy::new())
        .await
        .map_err(|e| AiError::Model(map_model_error(e.to_string())))?;
    parse_plan_json(&text)
}

#[cfg(feature = "kalosm")]
fn map_model_error(msg: String) -> String {
    if msg.contains("No valid tokens") || msg.contains("No token sampled") {
        format!(
            "{msg}. The model produced no usable next token. On Metal that \
             is usually NaN logits after the prompt overran a 2k/4k context. \
             Retry with --from 1 --to 30 (title/header) or a smaller view, \
             not a bigger machine."
        )
    } else if msg.contains("weight is invalid") || msg.contains("Sampler error") {
        format!(
            "{msg}. The sampler hit invalid logits. Retry with --model phi-3 \
             (the default) rather than phi-3.5."
        )
    } else {
        msg
    }
}

#[cfg(test)]
mod tests {
    use super::ModelId;

    #[test]
    fn default_is_phi3() {
        assert_eq!(ModelId::parse("phi-3").unwrap(), ModelId::Phi3);
        assert_eq!(ModelId::parse("default").unwrap(), ModelId::Phi3);
        assert_eq!(ModelId::parse("phi-3.5").unwrap(), ModelId::Phi35);
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

    #[test]
    fn phi3_is_a_preset_not_a_relative_path() {
        // `path.exists()` used to win, so a cwd file named `phi-3` loaded
        // as a GGUF (often the leftover 128k Phi-3.5 weights).
        assert_eq!(ModelId::parse("phi-3").unwrap(), ModelId::Phi3);
        assert_eq!(ModelId::parse("tinyllama").unwrap(), ModelId::TinyLlama);
    }
}
