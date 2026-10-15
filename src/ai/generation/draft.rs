//! Conservative preview boundary: complete prose sentences only, never tool/code
//! envelopes, citations or action claims. Final validation remains authoritative.
use std::time::{Duration, Instant};

pub(super) struct DraftBuffer {
    sentence: String,
    ready: String,
    blocked: bool,
    flushed_at: Instant,
}

impl Default for DraftBuffer {
    fn default() -> Self {
        Self {
            sentence: String::new(),
            ready: String::new(),
            blocked: false,
            flushed_at: Instant::now(),
        }
    }
}

impl DraftBuffer {
    pub(super) fn push(&mut self, delta: &str) {
        if self.blocked {
            return;
        }
        for ch in delta.chars() {
            self.sentence.push(ch);
            // Once a protocol/evidence/code opener appears, buffer the rest of
            // this request for the existing full-response validator. This also
            // catches markers split across arbitrary network chunks.
            if matches!(ch, '<' | '[' | '{' | '`') || self.sentence.len() > 4096 {
                self.blocked = true;
                self.sentence.clear();
                return;
            }
            if matches!(ch, '。' | '！' | '？' | '\n' | '.' | '!' | '?') {
                if unsafe_sentence(&self.sentence) {
                    self.blocked = true;
                    self.sentence.clear();
                    return;
                }
                self.ready.push_str(&std::mem::take(&mut self.sentence));
            }
        }
    }

    pub(super) fn take_checkpoint(&mut self, force: bool) -> Option<String> {
        if self.ready.is_empty()
            || (!force
                && self.ready.len() < 2048
                && self.flushed_at.elapsed() < Duration::from_millis(100))
        {
            return None;
        }
        self.flushed_at = Instant::now();
        let mut end = self.ready.len().min(2048);
        while !self.ready.is_char_boundary(end) {
            end -= 1;
        }
        Some(self.ready.drain(..end).collect())
    }
}

fn unsafe_sentence(sentence: &str) -> bool {
    // Preview deliberately over-buffers even negated or quoted claims. It does
    // not decide whether a write happened; only the final receipt validator can.
    let lower = sentence.to_lowercase();
    [
        "保存",
        "存好",
        "记下",
        "记录好",
        "记好",
        "创建",
        "新建",
        "建好",
        "归档",
        "入库",
        "生效",
        "写入",
        "更新",
        "删除",
        "完成",
        "执行",
        "已经",
        "已将",
        "已把",
        "已为",
        "已帮",
        "稍等",
        "稍后",
        "我会",
        "我将",
        "我来",
        "让我",
        "接下来",
        "工具",
        "推理",
        "思考",
        "saved",
        "stored",
        "created",
        "updated",
        "deleted",
        "recorded",
        "executed",
        "completed",
        "done",
        "i will",
        "i'll",
        "let me",
        "tool_call",
        "function_call",
        "arguments",
        "reasoning",
        "analysis:",
    ]
    .iter()
    .any(|word| lower.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_character_split_buffers_protocols_and_save_claims() {
        for suffix in [
            "<tool_call>save",
            "[工具调用]{\"name\":\"save\"}",
            "已保存到知识库。",
            "I saved your note.",
            "```json\nsecret",
            "<think>private",
        ] {
            let mut buffer = DraftBuffer::default();
            for ch in format!("普通说明。{suffix}").chars() {
                buffer.push(&ch.to_string());
            }
            assert_eq!(buffer.take_checkpoint(true).as_deref(), Some("普通说明。"));
        }
    }

    #[test]
    fn incomplete_sentences_are_not_checkpoints() {
        let mut buffer = DraftBuffer::default();
        buffer.push("说明。尚未结束的句子");
        assert_eq!(buffer.take_checkpoint(true).as_deref(), Some("说明。"));
        assert!(buffer.take_checkpoint(true).is_none());
    }

    #[test]
    fn long_unbroken_text_is_bounded_until_final_validation() {
        let mut buffer = DraftBuffer::default();
        buffer.push(&"x".repeat(5000));
        buffer.push("done.");
        assert!(buffer.sentence.is_empty());
        assert!(buffer.take_checkpoint(true).is_none());
    }
}
