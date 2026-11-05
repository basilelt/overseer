use crate::http;
use crate::transport::DaemonConn;
use tokio::io::{AsyncWriteExt, BufReader};

pub async fn stream(
    conn: &DaemonConn,
    last: Option<String>,
    parser: &mut SseParser,
    mut on: impl FnMut(serde_json::Value),
) -> bool {
    let Ok(io) = conn.open().await else {
        return false;
    };
    let mut r = BufReader::new(io);
    let mut headers = vec![("Accept", "text/event-stream")];
    if let Some(id) = last.as_deref() {
        headers.push(("Last-Event-ID", id));
    }
    let req = http::encode_request("GET", &conn.path("/v1/events"), &headers, None);
    if r.get_mut().write_all(&req).await.is_err() {
        return false;
    }
    let Ok(head) = http::read_head(&mut r).await else {
        return false;
    };
    if head.status != 200 {
        return false;
    }
    let mut got = false;
    let _ = http::stream_body(&mut r, &head, |chunk| {
        got = true;
        for ev in parser.push(chunk) {
            if let Some(v) = to_envelope(&ev) {
                on(v);
            }
        }
    })
    .await;
    got
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SseEvent {
    pub id: Option<String>,
    pub event: String,
    pub data: String,
}

#[derive(Default)]
pub struct SseParser {
    buf: Vec<u8>,
    id: Option<String>,
    event: String,
    data: Vec<String>,
    pub last_id: Option<String>,
}

impl SseParser {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let raw: Vec<u8> = self.buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&raw);
            let line = line.trim_end_matches(['\n', '\r']);
            if let Some(ev) = self.line(line) {
                out.push(ev);
            }
        }
        out
    }

    fn line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            if self.data.is_empty() {
                self.event.clear();
                return None;
            }
            let ev = SseEvent {
                id: self.id.take(),
                event: std::mem::take(&mut self.event),
                data: std::mem::take(&mut self.data).join("\n"),
            };
            if ev.id.is_some() {
                self.last_id.clone_from(&ev.id);
            }
            return Some(ev);
        }
        if line.starts_with(':') {
            return None;
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "data" => self.data.push(value.to_string()),
            "event" => self.event = value.to_string(),
            "id" if !value.contains('\0') => self.id = Some(value.to_string()),
            _ => {}
        }
        None
    }
}

pub fn to_envelope(ev: &SseEvent) -> Option<serde_json::Value> {
    let mut v: serde_json::Value = serde_json::from_str(&ev.data).ok()?;
    let obj = v.as_object_mut()?;
    if !obj.contains_key("type") && !ev.event.is_empty() {
        obj.insert("type".into(), ev.event.clone().into());
    }
    if let (false, Some(id)) = (obj.contains_key("id"), &ev.id) {
        obj.insert("id".into(), id.clone().into());
    }
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_events_comments_and_multiline_data() {
        let mut p = SseParser::default();
        let evs = p.push(b": ping\n\nid: 7\nevent: line.updated\ndata: {\"a\":\ndata: 1}\n\n");
        assert_eq!(
            evs,
            vec![SseEvent {
                id: Some("7".into()),
                event: "line.updated".into(),
                data: "{\"a\":\n1}".into()
            }]
        );
        assert_eq!(p.last_id.as_deref(), Some("7"));
    }

    #[test]
    fn handles_partial_chunks_and_crlf() {
        let mut p = SseParser::default();
        assert!(p.push(b"id: 1\r\nevent: task.cre").is_empty());
        assert!(p.push(b"ated\r\ndata:{}").is_empty());
        let evs = p.push(b"\r\n\r\n");
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].event, "task.created");
        assert_eq!(evs[0].data, "{}");
    }

    #[test]
    fn event_without_data_is_dropped() {
        let mut p = SseParser::default();
        assert!(p.push(b"event: x\n\n").is_empty());
        assert_eq!(p.push(b"data: 1\n\n")[0].event, "");
    }

    #[test]
    fn envelope_fills_type_and_id() {
        let ev = SseEvent {
            id: Some("3".into()),
            event: "term.created".into(),
            data: r#"{"data":{"id":"t"}}"#.into(),
        };
        let v = to_envelope(&ev).unwrap();
        assert_eq!(v["type"], "term.created");
        assert_eq!(v["id"], "3");
        let full = SseEvent {
            id: Some("4".into()),
            event: "x".into(),
            data: r#"{"id":"9","type":"line.created","at":"t","data":{}}"#.into(),
        };
        assert_eq!(to_envelope(&full).unwrap()["type"], "line.created");
        assert!(to_envelope(&SseEvent {
            data: "not json".into(),
            ..Default::default()
        })
        .is_none());
    }
}
