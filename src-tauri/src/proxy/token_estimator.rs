//! Local token estimation using tiktoken (cl100k_base encoder).
//!
//! Used to intercept Claude Desktop's `/v1/messages/count_tokens` requests
//! and return approximate counts without forwarding to upstream, protecting
//! llama.cpp KV cache from pollution.

use serde_json::Value;
use std::sync::LazyLock;

/// Cached cl100k_base encoder — cloned from the parking_lot-protected singleton
/// so callers don't hold the mutex during encode operations.
static ENCODER: LazyLock<tiktoken_rs::CoreBPE> = LazyLock::new(|| {
    let singleton = tiktoken_rs::cl100k_base_singleton();
    let bpe = singleton.lock().clone();
    bpe
});

/// Estimate the number of input tokens in a Claude API messages request body.
pub fn estimate_count_tokens(body: &Value) -> usize {
    count_with_encoder(&ENCODER, body)
}

fn count_with_encoder(bpe: &tiktoken_rs::CoreBPE, body: &Value) -> usize {
    let mut total = 0;

    if let Some(system) = body.get("system") {
        if let Some(text) = system.as_str() {
            total += bpe.encode_ordinary(text).len();
        } else if let Some(blocks) = system.as_array() {
            for block in blocks {
                if let Some(text) = extract_text(block) {
                    total += bpe.encode_ordinary(text).len();
                }
            }
        }
    }

    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        for msg in messages {
            if let Some(role) = msg.get("role").and_then(|r| r.as_str()) {
                total += bpe.encode_ordinary(role).len();
            }
            if let Some(content) = msg.get("content") {
                if let Some(text) = content.as_str() {
                    total += bpe.encode_ordinary(text).len();
                } else if let Some(blocks) = content.as_array() {
                    for block in blocks {
                        if let Some(text) = extract_text(block) {
                            total += bpe.encode_ordinary(text).len();
                        }
                    }
                }
            }
        }
    }

    total
}

fn extract_text(block: &Value) -> Option<&str> {
    if block.get("type").and_then(|t| t.as_str()) == Some("text") {
        return block.get("text").and_then(|t| t.as_str());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn estimate_simple_message() {
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [
                {"role": "user", "content": "Hello, how are you?"}
            ]
        });
        let tokens = estimate_count_tokens(&body);
        assert!(tokens >= 4 && tokens <= 10, "Expected ~5-8 tokens, got {tokens}");
    }

    #[test]
    fn estimate_with_system_prompt() {
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "system": [{"type": "text", "text": "You are a helpful assistant."}],
            "messages": [
                {"role": "user", "content": "What is Rust?"}
            ]
        });
        assert!(estimate_count_tokens(&body) > 0);
    }

    #[test]
    fn estimate_skips_image_blocks() {
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "Analyze this image:"},
                        {"type": "image", "source": {"type": "base64", "data": "xxxx"}}
                    ]
                }
            ]
        });
        assert!(estimate_count_tokens(&body) < 100);
    }

    #[test]
    fn estimate_empty_body_returns_zero() {
        assert_eq!(estimate_count_tokens(&json!({})), 0);
    }

    #[test]
    fn estimate_string_system_prompt() {
        let body = json!({
            "model": "claude-sonnet-4",
            "system": "You are a helpful assistant.",
            "messages": [
                {"role": "user", "content": "Hi"}
            ]
        });
        assert!(estimate_count_tokens(&body) > 0);
    }
}
