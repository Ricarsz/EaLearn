//! 增量 SSE 解析。
//!
//! 字节流到达时边界是任意的，一个 `data:` 行可能被切成两半；这里只保存不完整
//! 的尾巴，凑齐整行再处理。事件由空行分隔，`data: [DONE]` 是结束哨兵。

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseEvent {
    /// `data:` 行合并后的负载（多行 data 用 \n 连接）。
    Data(String),
    /// `data: [DONE]`
    Done,
}

#[derive(Debug, Default)]
pub struct SseParser {
    buf: String,
    data: Vec<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// 喂入任意长度的片段，返回本片段内已经完整的事件。
    pub fn push(&mut self, chunk: &str) -> Vec<SseEvent> {
        self.buf.push_str(chunk);
        let mut out = Vec::new();

        while let Some(nl) = self.buf.find('\n') {
            let mut line = self.buf[..nl].to_string();
            self.buf.drain(..=nl);
            if line.ends_with('\r') {
                line.pop();
            }

            if line.is_empty() {
                if let Some(event) = self.flush_event() {
                    let done = event == SseEvent::Done;
                    out.push(event);
                    if done {
                        return out;
                    }
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("data:") {
                self.data
                    .push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
            }
            // `event:` / `id:` / `:` 注释行直接忽略
        }
        out
    }

    /// 流结束：把没有以空行收尾的最后一个事件也吐出来。
    pub fn finish(&mut self) -> Vec<SseEvent> {
        let mut out = Vec::new();
        if !self.buf.trim().is_empty() {
            let line = std::mem::take(&mut self.buf);
            if let Some(rest) = line.trim_end().strip_prefix("data:") {
                self.data
                    .push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
            }
        }
        if let Some(event) = self.flush_event() {
            out.push(event);
        }
        out
    }

    fn flush_event(&mut self) -> Option<SseEvent> {
        if self.data.is_empty() {
            return None;
        }
        let payload = self.data.join("\n");
        self.data.clear();
        if payload.trim() == "[DONE]" {
            Some(SseEvent::Done)
        } else {
            Some(SseEvent::Data(payload))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_event() {
        let mut p = SseParser::new();
        let ev = p.push("data: {\"a\":1}\n\n");
        assert_eq!(ev, vec![SseEvent::Data("{\"a\":1}".into())]);
    }

    #[test]
    fn survives_split_lines() {
        let mut p = SseParser::new();
        assert!(p.push("data: {\"a\":").is_empty());
        assert!(p.push("1}").is_empty());
        let ev = p.push("\n\n");
        assert_eq!(ev, vec![SseEvent::Data("{\"a\":1}".into())]);
    }

    #[test]
    fn handles_crlf() {
        let mut p = SseParser::new();
        let ev = p.push("data: x\r\n\r\n");
        assert_eq!(ev, vec![SseEvent::Data("x".into())]);
    }

    #[test]
    fn detects_done() {
        let mut p = SseParser::new();
        let ev = p.push("data: [DONE]\n\n");
        assert_eq!(ev, vec![SseEvent::Done]);
    }

    #[test]
    fn ignores_comments_and_event_lines() {
        let mut p = SseParser::new();
        let ev = p.push(": keep-alive\nevent: message\ndata: hi\n\n");
        assert_eq!(ev, vec![SseEvent::Data("hi".into())]);
    }

    #[test]
    fn finish_flushes_unterminated_event() {
        let mut p = SseParser::new();
        assert!(p.push("data: tail").is_empty());
        assert_eq!(p.finish(), vec![SseEvent::Data("tail".into())]);
    }

    #[test]
    fn multiple_data_lines_join() {
        let mut p = SseParser::new();
        let ev = p.push("data: a\ndata: b\n\n");
        assert_eq!(ev, vec![SseEvent::Data("a\nb".into())]);
    }
}
