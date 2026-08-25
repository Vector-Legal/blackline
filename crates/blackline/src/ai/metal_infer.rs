//! llama.cpp Metal inference for `bl ai` on macOS.
//!
//! Kalosm's Candle Metal backend returns NaN logits (`No token sampled`)
//! even for TinyLlama 2k. llama.cpp's Metal path is what actually uses
//! the Apple GPU. Context is capped here so a 128k GGUF does not size
//! a 128k KV cache. Generation is capped so a run cannot go forever.

use std::num::NonZeroU32;
use std::path::Path;
use std::sync::OnceLock;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::{send_logs_to_tracing, LogOptions, TokenToStringError};

use super::error::AiError;

/// Offload every transformer layer to Metal.
const GPU_LAYERS: u32 = 999;

/// New tokens for a plan. The object is small; this is also the hard stop.
pub(crate) const PLAN_MAX_TOKENS: u32 = 256;

/// KV / RoPE size we actually allocate. Independent of GGUF metadata.
pub(crate) const INFER_CONTEXT: u32 = 4096;

fn backend() -> Result<&'static LlamaBackend, AiError> {
    static BACKEND: OnceLock<LlamaBackend> = OnceLock::new();
    if let Some(existing) = BACKEND.get() {
        return Ok(existing);
    }
    send_logs_to_tracing(LogOptions::default().with_logs_enabled(false));
    let init = LlamaBackend::init()
        .map_err(|e| AiError::Model(format!("llama.cpp backend init failed: {e}")))?;
    let _ = BACKEND.set(init);
    BACKEND
        .get()
        .ok_or_else(|| AiError::Model("llama.cpp backend missing after init".into()))
}

fn token_piece(model: &LlamaModel, token: LlamaToken) -> String {
    match model.token_to_piece_bytes(token, 32, true, None) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(TokenToStringError::InsufficientBufferSpace(need)) => {
            let n = usize::try_from(need.unsigned_abs()).unwrap_or(256);
            model
                .token_to_piece_bytes(token, n.max(1), true, None)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_default()
        }
        Err(_) => String::new(),
    }
}

/// Run one chat turn on Metal. Returns the raw assistant text.
pub(crate) fn complete(
    gguf: &Path,
    system: &str,
    user: &str,
    context_length: u32,
) -> Result<String, AiError> {
    let backend = backend()?;
    let params = LlamaModelParams::default().with_n_gpu_layers(GPU_LAYERS);
    let model = LlamaModel::load_from_file(backend, gguf, &params)
        .map_err(|e| AiError::Model(format!("llama.cpp failed to load {}: {e}", gguf.display())))?;

    let n_ctx = context_length.clamp(512, INFER_CONTEXT);
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(Some(NonZeroU32::new(n_ctx).expect("n_ctx >= 512")));
    let mut ctx = model
        .new_context(backend, ctx_params)
        .map_err(|e| AiError::Model(format!("llama.cpp context failed: {e}")))?;

    let prompt = chat_prompt(&model, system, user)?;
    let tokens = model
        .str_to_token(&prompt, AddBos::Never)
        .map_err(|e| AiError::Model(format!("tokenize failed: {e}")))?;
    if tokens.is_empty() {
        return Err(AiError::Model("prompt tokenized to nothing".into()));
    }
    let prompt_len = u32::try_from(tokens.len()).unwrap_or(u32::MAX);
    if prompt_len >= n_ctx {
        return Err(AiError::Model(format!(
            "prompt is {prompt_len} tokens; context is {n_ctx}. \
             Pass --from / --to to shrink the view."
        )));
    }
    let max_new = PLAN_MAX_TOKENS.min(n_ctx.saturating_sub(prompt_len));

    let mut batch = LlamaBatch::new(n_ctx as usize, 1);
    let last = i32::try_from(tokens.len().saturating_sub(1)).unwrap_or(0);
    for (i, token) in (0_i32..).zip(tokens) {
        batch
            .add(token, i, &[0], i == last)
            .map_err(|e| AiError::Model(format!("llama.cpp batch: {e}")))?;
    }
    ctx.decode(&mut batch)
        .map_err(|e| AiError::Model(format!("llama.cpp decode prompt: {e}")))?;

    let mut sampler = LlamaSampler::chain_simple([LlamaSampler::greedy()]);
    let mut n_cur = batch.n_tokens();
    let mut out = String::new();
    for _ in 0..max_new {
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        sampler.accept(token);
        if model.is_eog_token(token) {
            break;
        }
        out.push_str(&token_piece(&model, token));
        batch.clear();
        batch
            .add(token, n_cur, &[0], true)
            .map_err(|e| AiError::Model(format!("llama.cpp batch: {e}")))?;
        ctx.decode(&mut batch)
            .map_err(|e| AiError::Model(format!("llama.cpp decode: {e}")))?;
        n_cur += 1;
    }
    Ok(out)
}

fn chat_prompt(model: &LlamaModel, system: &str, user: &str) -> Result<String, AiError> {
    if let Ok(tmpl) = model.chat_template(None) {
        let messages = [
            LlamaChatMessage::new("system".into(), system.to_string())
                .map_err(|e| AiError::Model(format!("chat message: {e}")))?,
            LlamaChatMessage::new("user".into(), user.to_string())
                .map_err(|e| AiError::Model(format!("chat message: {e}")))?,
        ];
        if let Ok(prompt) = model.apply_chat_template(&tmpl, &messages, true) {
            return Ok(prompt);
        }
    }
    Ok(format!(
        "<|system|>\n{system}<|end|>\n<|user|>\n{user}<|end|>\n<|assistant|>\n"
    ))
}
