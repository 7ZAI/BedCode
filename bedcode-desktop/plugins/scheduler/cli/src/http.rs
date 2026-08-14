//! 极简 HTTP/1.1 客户端（localhost 专用，零依赖）
//!
//! 桌面端网关为本地 HTTP 服务（免 token 放行 /api/plugin/*），无需 TLS；
//! 不引入 reqwest/tokio，产物小、构建快。响应按 `Connection: close`
//! 读到 EOF 解析（actix 对小 JSON 响应带 Content-Length，同样兼容）。

use std::io::{Read, Write};
use std::net::TcpStream;

/// 发起 HTTP 请求，返回 `{ status, body }`
///
/// `path` 含查询串（如 `/api/plugin/com.bedcode.scheduler/task-scheduler/list`）。
/// 连接失败（桌面端未运行）返回 Err("desktop not running")。
pub fn request(
    port: u16,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> Result<serde_json::Value, String> {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .map_err(|e| format!("desktop not running (connect {} failed: {})", port, e))?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(15)))
        .map_err(|e| format!("set read timeout failed: {}", e))?;

    let mut req = format!(
        "{} {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n",
        method, path, port
    );
    if let Some(b) = body {
        req.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            b.len()
        ));
    }
    req.push_str("\r\n");
    if let Some(b) = body {
        req.push_str(b);
    }

    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("write request failed: {}", e))?;

    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) => return Err(format!("read response failed: {}", e)),
        }
    }
    parse_http_response(&buf)
}

/// 解析 HTTP/1.1 响应：状态行 + 头 + 体（体为 JSON）
///
/// 返回 `{ "status": <u16>, "body": <json> }`；体为空时 body 为 null。
pub fn parse_http_response(buf: &[u8]) -> Result<serde_json::Value, String> {
    let header_end = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| "invalid HTTP response: missing header terminator".to_string())?;
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let status_line = head
        .lines()
        .next()
        .ok_or_else(|| "invalid HTTP response: empty status line".to_string())?;
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("invalid HTTP status line: {}", status_line))?;

    let body_bytes = &buf[header_end + 4..];
    let body = if body_bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(body_bytes)
            .map_err(|e| format!("invalid JSON in HTTP body: {}", e))?
    };
    Ok(serde_json::json!({ "status": status, "body": body }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_success_response() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 42\r\n\r\n{\"code\":0,\"message\":\"ok\",\"data\":{\"jobs\":[]}}";
        let v = parse_http_response(raw).unwrap();
        assert_eq!(v["status"], 200);
        assert_eq!(v["body"]["code"], 0);
        assert_eq!(v["body"]["data"]["jobs"], serde_json::json!([]));
    }

    #[test]
    fn parse_error_response() {
        let raw = b"HTTP/1.1 404 Not Found\r\nContent-Length: 30\r\n\r\n{\"code\":404,\"message\":\"nope\"}";
        let v = parse_http_response(raw).unwrap();
        assert_eq!(v["status"], 404);
        assert_eq!(v["body"]["message"], "nope");
    }

    #[test]
    fn parse_empty_body() {
        let raw = b"HTTP/1.1 204 No Content\r\n\r\n";
        let v = parse_http_response(raw).unwrap();
        assert_eq!(v["status"], 204);
        assert!(v["body"].is_null());
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(parse_http_response(b"not http").is_err());
        assert!(parse_http_response(b"HTTP/1.1 xxx\r\n\r\n{}").is_err());
    }
}
