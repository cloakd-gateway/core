use crate::error::CloakdError;
use crate::vault::{SessionVault, TOKEN_PREFIX, TOKEN_SUFFIX};
use bytes::{Bytes, BytesMut};
use futures_util::Stream;
use serde_json::Value;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

/// Maximum allowed length for a pseudonymization token (e.g., `{{__VAR_CREDIT_CARD_99999__}}`).
/// If an accumulating token exceeds this length, it is treated as regular text and flushed.
const MAX_TOKEN_LEN: usize = 64;

/// Finite State Machine to track and reconstruct fragmented tokens across text deltas.
#[derive(Debug)]
pub struct TokenStreamFsm {
    /// Pending characters that could form a token or prefix.
    pending_buffer: String,
    /// Maximum length before forced flush.
    max_token_len: usize,
}

impl TokenStreamFsm {
    pub fn new() -> Self {
        Self {
            pending_buffer: String::with_capacity(MAX_TOKEN_LEN),
            max_token_len: MAX_TOKEN_LEN,
        }
    }

    /// Processes an incoming text chunk (e.g. from delta.content) and produces de-tokenized output text.
    pub fn process_chunk(&mut self, chunk: &str, vault: &SessionVault) -> String {
        let mut output = String::new();

        for c in chunk.chars() {
            if self.pending_buffer.is_empty() {
                if c == '{' {
                    // Possible start of TOKEN_PREFIX "{{__VAR_"
                    self.pending_buffer.push(c);
                } else {
                    output.push(c);
                }
            } else {
                // We have characters in pending_buffer
                self.pending_buffer.push(c);

                if self.pending_buffer.starts_with(TOKEN_PREFIX) {
                    // We are accumulating characters after the prefix
                    if self.pending_buffer.ends_with(TOKEN_SUFFIX) {
                        // Complete token candidate!
                        if let Some(original) = vault.restore_token(&self.pending_buffer) {
                            output.push_str(original);
                        } else {
                            // Token not in vault, output as-is
                            output.push_str(&self.pending_buffer);
                        }
                        self.pending_buffer.clear();
                    } else if self.pending_buffer.len() >= self.max_token_len {
                        output.push_str(&self.pending_buffer);
                        self.pending_buffer.clear();
                    } else {
                        let is_body_char = c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_';
                        let is_partial_suffix = c == '}' && self.pending_buffer.ends_with("__}");
                        if !is_body_char && !is_partial_suffix {
                            output.push_str(&self.pending_buffer);
                            self.pending_buffer.clear();
                        }
                    }
                } else if TOKEN_PREFIX.starts_with(&self.pending_buffer) {
                    // Still matching the prefix prefix (e.g. "{", "{{", "{{_", "{{__V")
                    // Keep accumulating
                } else {
                    // Mismatch with prefix! Flush buffered characters
                    output.push_str(&self.pending_buffer);
                    self.pending_buffer.clear();
                }
            }
        }

        output
    }

    /// Flushes any remaining characters in the buffer (e.g., at end of stream).
    pub fn flush(&mut self, vault: &SessionVault) -> String {
        if self.pending_buffer.is_empty() {
            return String::new();
        }

        let mut output = String::new();
        if self.pending_buffer.starts_with(TOKEN_PREFIX)
            && self.pending_buffer.ends_with(TOKEN_SUFFIX)
        {
            if let Some(original) = vault.restore_token(&self.pending_buffer) {
                output.push_str(original);
            } else {
                output.push_str(&self.pending_buffer);
            }
        } else {
            output.push_str(&self.pending_buffer);
        }
        self.pending_buffer.clear();
        output
    }
}

impl Default for TokenStreamFsm {
    fn default() -> Self {
        Self::new()
    }
}

/// Transforms raw byte stream from upstream SSE into de-tokenized SSE byte chunks.
pub struct SseStreamTransformer<S> {
    upstream: S,
    vault: Arc<SessionVault>,
    byte_buffer: BytesMut,
    fsm: TokenStreamFsm,
    is_done: bool,
}

impl<S> SseStreamTransformer<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    pub fn new(upstream: S, vault: Arc<SessionVault>) -> Self {
        Self {
            upstream,
            vault,
            byte_buffer: BytesMut::with_capacity(4096),
            fsm: TokenStreamFsm::new(),
            is_done: false,
        }
    }

    /// Processes a complete SSE line or block (e.g. `data: {...}\n\n`).
    fn process_sse_frame(&mut self, frame: &str) -> Option<String> {
        let trimmed = frame.trim();
        if trimmed.is_empty() {
            return Some(frame.to_string());
        }

        // Check for SSE comment or ping
        if trimmed.starts_with(':') {
            return Some(frame.to_string());
        }

        // Handle `data: [DONE]`
        if trimmed == "data: [DONE]" {
            self.is_done = true;
            let flushed = self.fsm.flush(&self.vault);
            if !flushed.is_empty() {
                // Emit synthetic chunk with flushed content before [DONE]
                let synthetic_json = serde_json::json!({
                    "choices": [{
                        "index": 0,
                        "delta": { "content": flushed },
                        "finish_reason": null
                    }]
                });
                return Some(format!("data: {synthetic_json}\n\ndata: [DONE]\n\n"));
            }
            return Some("data: [DONE]\n\n".to_string());
        }

        // Handle `data: {...}`
        if let Some(json_payload) = trimmed.strip_prefix("data:") {
            let json_payload = json_payload.trim();
            match serde_json::from_str::<Value>(json_payload) {
                Ok(mut value) => {
                    let mut modified = false;

                    if let Some(choices) = value.get_mut("choices").and_then(Value::as_array_mut) {
                        for choice in choices {
                            if let Some(delta) = choice.get_mut("delta") {
                                if let Some(content) = delta.get_mut("content") {
                                    if let Some(content_str) = content.as_str() {
                                        let transformed =
                                            self.fsm.process_chunk(content_str, &self.vault);
                                        *content = Value::String(transformed);
                                        modified = true;
                                    }
                                }
                                // Also support reasoning_content (DeepSeek, etc.)
                                if let Some(reasoning) = delta.get_mut("reasoning_content") {
                                    if let Some(reasoning_str) = reasoning.as_str() {
                                        let transformed =
                                            self.fsm.process_chunk(reasoning_str, &self.vault);
                                        *reasoning = Value::String(transformed);
                                        modified = true;
                                    }
                                }
                            }
                        }
                    }

                    if modified {
                        let serialized = serde_json::to_string(&value).unwrap_or_else(|_| json_payload.to_string());
                        return Some(format!("data: {serialized}\n\n"));
                    } else {
                        return Some(format!("data: {json_payload}\n\n"));
                    }
                }
                Err(_) => {
                    // Non-JSON or malformed payload, pass through
                    return Some(frame.to_string());
                }
            }
        }

        Some(frame.to_string())
    }
}

impl<S> Stream for SseStreamTransformer<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<Bytes, CloakdError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            // First check if we have a complete SSE message in our byte buffer
            // An SSE message ends with \n\n or \r\n\r\n
            if let Some(delimiter_pos) = find_sse_delimiter(&self.byte_buffer) {
                let frame_bytes = self.byte_buffer.split_to(delimiter_pos.end);
                if let Ok(frame_str) = std::str::from_utf8(&frame_bytes) {
                    if let Some(processed) = self.process_sse_frame(frame_str) {
                        return Poll::Ready(Some(Ok(Bytes::from(processed))));
                    }
                } else {
                    // Pass invalid UTF-8 through directly
                    return Poll::Ready(Some(Ok(frame_bytes.freeze())));
                }
                continue;
            }

            if self.is_done {
                return Poll::Ready(None);
            }

            // Need more data from upstream
            match Pin::new(&mut self.upstream).poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    self.byte_buffer.extend_from_slice(&chunk);
                }
                Poll::Ready(Some(Err(err))) => {
                    return Poll::Ready(Some(Err(CloakdError::Stream(err.to_string()))));
                }
                Poll::Ready(None) => {
                    // Upstream closed
                    if !self.byte_buffer.is_empty() {
                        let remaining = self.byte_buffer.split().freeze();
                        if let Ok(remaining_str) = std::str::from_utf8(&remaining) {
                            if let Some(processed) = self.process_sse_frame(remaining_str) {
                                return Poll::Ready(Some(Ok(Bytes::from(processed))));
                            }
                        }
                        return Poll::Ready(Some(Ok(remaining)));
                    }

                    let vault = self.vault.clone();
                    let flushed = self.fsm.flush(&vault);
                    if !flushed.is_empty() {
                        let synthetic_json = serde_json::json!({
                            "choices": [{
                                "index": 0,
                                "delta": { "content": flushed },
                                "finish_reason": null
                            }]
                        });
                        return Poll::Ready(Some(Ok(Bytes::from(format!(
                            "data: {synthetic_json}\n\n"
                        )))));
                    }

                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

struct DelimiterMatch {
    end: usize,
}

fn find_sse_delimiter(buf: &[u8]) -> Option<DelimiterMatch> {
    for i in 0..buf.len() {
        if buf[i] == b'\n' {
            if i + 1 < buf.len() && buf[i + 1] == b'\n' {
                return Some(DelimiterMatch { end: i + 2 });
            }
            if i + 2 < buf.len() && buf[i + 1] == b'\r' && buf[i + 2] == b'\n' {
                return Some(DelimiterMatch { end: i + 3 });
            }
        } else if buf[i] == b'\r' && i + 3 < buf.len() && &buf[i..i + 4] == b"\r\n\r\n" {
            return Some(DelimiterMatch { end: i + 4 });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fsm_single_chunk_unmasked() {
        let mut vault = SessionVault::new();
        let token = vault.get_or_create_token("EMAIL", "jean.dupont@corp.fr");
        let mut fsm = TokenStreamFsm::new();

        let input = format!("Bonjour {token}, bienvenue!");
        let output = fsm.process_chunk(&input, &vault);
        assert_eq!(output, "Bonjour jean.dupont@corp.fr, bienvenue!");
    }

    #[test]
    fn test_fsm_token_fragmented_across_chunks() {
        let mut vault = SessionVault::new();
        let token = vault.get_or_create_token("EMAIL", "jean.dupont@corp.fr");
        assert_eq!(token, "{{__VAR_EMAIL_1__}}");

        let mut fsm = TokenStreamFsm::new();

        // Chunk 1: regular text + start of token prefix
        let c1 = fsm.process_chunk("Hello ", &vault);
        assert_eq!(c1, "Hello ");

        // Chunk 2: prefix and partial token
        let c2 = fsm.process_chunk("{{__VAR_EMA", &vault);
        assert_eq!(c2, ""); // Held back in sliding buffer

        // Chunk 3: middle of token
        let c3 = fsm.process_chunk("IL_1", &vault);
        assert_eq!(c3, ""); // Still held back

        // Chunk 4: suffix and trailing text
        let c4 = fsm.process_chunk("__}}! How can I help?", &vault);
        assert_eq!(c4, "jean.dupont@corp.fr! How can I help?");
    }

    #[test]
    fn test_fsm_non_token_false_alarm() {
        let vault = SessionVault::new();
        let mut fsm = TokenStreamFsm::new();

        // Looks like a token start but has a space
        let c1 = fsm.process_chunk("Equation: {{__VAR_ foo", &vault);
        assert_eq!(c1, "Equation: {{__VAR_ foo");
    }

    #[test]
    fn test_fsm_end_of_stream_flush() {
        let vault = SessionVault::new();
        let mut fsm = TokenStreamFsm::new();

        let c1 = fsm.process_chunk("Partial {{__VAR_", &vault);
        assert_eq!(c1, "Partial ");

        let flushed = fsm.flush(&vault);
        assert_eq!(flushed, "{{__VAR_");
    }
}
