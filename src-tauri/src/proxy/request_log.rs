//! 内存请求日志缓冲区
//!
//! Ring buffer 保存最近 N 次请求/响应的原始内容，重启后丢失

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::Mutex;

/// 单次请求日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestLogEntry {
    /// Unix 毫秒
    pub timestamp: u64,
    /// claude / codex / gemini
    pub app_type: String,
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    pub request_body: Value,
    pub response_body: Value,
    pub status_code: u16,
    pub latency_ms: u64,
    pub success: bool,
}

/// Ring buffer，容量固定，满则丢弃最老的条目
pub struct RequestLogBuffer {
    entries: Mutex<VecDeque<RequestLogEntry>>,
    capacity: usize,
}

impl RequestLogBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
        }
    }

    /// 推入一条记录，满时 pop_front
    pub fn push(&self, entry: RequestLogEntry) {
        let mut guard = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if guard.len() >= self.capacity {
            guard.pop_front();
        }
        guard.push_back(entry);
    }

    /// 获取最近 N 条（按时间倒序），limit 为 None 时返回全部
    pub fn recent(&self, limit: Option<usize>) -> Vec<RequestLogEntry> {
        let guard = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let len = guard.len();
        let start = limit.map(|l| len.saturating_sub(l)).unwrap_or(0);
        guard.range(start..).cloned().collect()
    }

    /// 按应用类型过滤后返回最近 N 条
    pub fn by_app_type(&self, app_type: &str, limit: Option<usize>) -> Vec<RequestLogEntry> {
        let guard = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let mut result = Vec::new();
        for entry in guard.iter().rev() {
            if entry.app_type == app_type {
                result.push(entry.clone());
            }
            if let Some(l) = limit {
                if result.len() >= l {
                    break;
                }
            }
        }
        result
    }
}

impl std::fmt::Debug for RequestLogBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let guard = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        f.debug_struct("RequestLogBuffer")
            .field("capacity", &self.capacity)
            .field("len", &guard.len())
            .finish()
    }
}

/// 简化请求体，只保留关键字段以避免内存占用过大
pub fn simplify_request_body(body: &Value) -> Value {
    let mut out = Map::new();
    use serde_json::Map;

    if let Some(model) = body.get("model") {
        out.insert("model".into(), model.clone());
    }

    if let Some(messages) = body.get("messages") {
        if let Some(arr) = messages.as_array() {
            // 只保留第一条 user message 的 content（截断到 2000 字符）
            let mut simplified = Vec::new();
            for msg in arr {
                let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
                if role == "user" && simplified.is_empty() {
                    let content = msg
                        .get("content")
                        .and_then(|v| v.as_str())
                        .map(|s| truncate_str(s, 2000))
                        .unwrap_or_default();
                    if !content.is_empty() {
                        let mut m = Map::new();
                        m.insert("role".into(), Value::String("user".into()));
                        m.insert("content".into(), Value::String(content));
                        simplified.push(Value::Object(m));
                        break;
                    }
                }
            }
            if simplified.is_empty() {
                out.insert("messages".into(), messages.clone());
            } else {
                out.insert("messages".into(), Value::Array(simplified));
            }
        }
    }

    if let Some(stream) = body.get("stream") {
        out.insert("stream".into(), stream.clone());
    }

    if let Some(stream_options) = body.get("stream_options") {
        out.insert("stream_options".into(), stream_options.clone());
    }

    if let Some(max_tokens) = body.get("max_tokens") {
        out.insert("max_tokens".into(), max_tokens.clone());
    }

    if let Some(thinking) = body.get("thinking") {
        out.insert("thinking".into(), thinking.clone());
    }

    Value::Object(out)
}

/// 简化响应体
pub fn simplify_response_body(body: &Value) -> Value {
    let json_str = serde_json::to_string(body).unwrap_or_default();
    if json_str.len() <= 5 * 1024 {
        return body.clone();
    }

    // 超过 5KB 时截断 content 字段
    let mut out = body.clone();
    if let Some(obj) = out.as_object_mut() {
        // Claude API: content 数组
        if let Some(content) = obj.get_mut("content") {
            if let Some(arr) = content.as_array_mut() {
                for item in arr.iter_mut() {
                    if let Some(Value::String(text)) = item.get_mut("text") {
                        *text = truncate_str(text, 3000);
                    }
                }
            }
        }
        // OpenAI 格式: choices[].message.content
        if let Some(choices) = obj.get_mut("choices").and_then(|v| v.as_array_mut()) {
            for choice in choices {
                if let Some(message) = choice.get_mut("message") {
                    if let Some(Value::String(content)) = message.get_mut("content") {
                        *content = truncate_str(content, 3000);
                    }
                }
            }
        }
        // Gemini: candidates[].content.parts[].text
        if let Some(candidates) = obj
            .get_mut("candidates")
            .and_then(|v| v.as_array_mut())
        {
            for candidate in candidates {
                if let Some(content) = candidate.get_mut("content") {
                    if let Some(parts) = content.get_mut("parts").and_then(|v| v.as_array_mut()) {
                        for part in parts {
                            if let Some(Value::String(text)) = part.get_mut("text") {
                                *text = truncate_str(text, 3000);
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_chars).collect();
    format!("{truncated}... (truncated)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_capacity() {
        let buf = RequestLogBuffer::new(3);
        for i in 0..5 {
            buf.push(RequestLogEntry {
                timestamp: i as u64,
                app_type: "claude".into(),
                provider_id: "p".into(),
                provider_name: "P".into(),
                model: "m".into(),
                request_body: Value::Null,
                response_body: Value::Null,
                status_code: 200,
                latency_ms: 100,
                success: true,
            });
        }
        let recent = buf.recent(None);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].timestamp, 2);
        assert_eq!(recent[1].timestamp, 3);
        assert_eq!(recent[2].timestamp, 4);
    }

    #[test]
    fn test_recent_limit() {
        let buf = RequestLogBuffer::new(10);
        for i in 0..5 {
            buf.push(RequestLogEntry {
                timestamp: i as u64,
                app_type: "claude".into(),
                provider_id: "p".into(),
                provider_name: "P".into(),
                model: "m".into(),
                request_body: Value::Null,
                response_body: Value::Null,
                status_code: 200,
                latency_ms: 100,
                success: true,
            });
        }
        let recent = buf.recent(Some(2));
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].timestamp, 3);
        assert_eq!(recent[1].timestamp, 4);
    }

    #[test]
    fn test_by_app_type() {
        let buf = RequestLogBuffer::new(10);
        buf.push(RequestLogEntry {
            timestamp: 0,
            app_type: "claude".into(),
            provider_id: "p".into(),
            provider_name: "P".into(),
            model: "m".into(),
            request_body: Value::Null,
            response_body: Value::Null,
            status_code: 200,
            latency_ms: 100,
            success: true,
        });
        buf.push(RequestLogEntry {
            timestamp: 1,
            app_type: "codex".into(),
            provider_id: "p".into(),
            provider_name: "P".into(),
            model: "m".into(),
            request_body: Value::Null,
            response_body: Value::Null,
            status_code: 200,
            latency_ms: 100,
            success: true,
        });
        let filtered = buf.by_app_type("codex", None);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].app_type, "codex");
    }

    #[test]
    fn test_truncate_str() {
        let s = "hello".repeat(100);
        let truncated = truncate_str(&s, 10);
        assert!(truncated.ends_with("(truncated)"));
        assert!(truncated.len() < s.len());
    }
}
