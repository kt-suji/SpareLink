use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const CHUNK_SIZE: usize = 64 * 1024;
const STAGING_TTL_SECS: u64 = 7200; // 2 hours

pub struct HttpRequestHead {
    pub method: String,
    pub path: String,
    pub content_length: Option<u64>,
    pub expect_continue: bool,
    pub body_offset: usize,
    pub remainder: Vec<u8>,
}

/// 一時ファイルステージング用ディレクトリ (%LOCALAPPDATA%\SpareLink\staging)
pub fn staging_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(local_app_data).join("SpareLink").join("staging");
        }
    }
    std::env::temp_dir().join("sparelink-staging")
}

/// パストラバーサルを防ぐため安全なファイルIDのみ許可
pub fn is_valid_file_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && !id.starts_with('.')
        && !id.contains("..")
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
}

async fn cleanup_expired_files(dir: &Path, max_age_secs: u64) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return;
    };
    let now = SystemTime::now();
    let max_age = Duration::from_secs(max_age_secs);
    while let Ok(Some(entry)) = entries.next_entry().await {
        if let Ok(meta) = entry.metadata().await
            && meta.is_file()
            && let Ok(modified) = meta.modified()
            && let Ok(age) = now.duration_since(modified)
            && age > max_age
        {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
}

/// `/api/files/<id>` の PUT / POST / GET / DELETE / OPTIONS をストリーミング処理
pub async fn handle_file_request(
    mut stream: TcpStream,
    head: HttpRequestHead,
    self_ip: &str,
    port: u16,
) {
    if head.method == "OPTIONS" {
        super::write_cors_preflight(&mut stream, "GET, POST, PUT, DELETE, OPTIONS").await;
        return;
    }

    let file_id = head.path.trim_start_matches("/api/files/");
    if !is_valid_file_id(file_id) {
        let err = br#"{"status":"error","message":"Invalid file ID"}"#;
        super::write_json_response(&mut stream, "400 Bad Request", err, true).await;
        return;
    }

    let dir = staging_dir();
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        let err = format!(r#"{{"status":"error","message":"Failed to prepare staging dir: {}"}}"#, e);
        super::write_json_response(&mut stream, "500 Internal Server Error", err.as_bytes(), true).await;
        return;
    }

    let dest_path = dir.join(file_id);

    if head.method == "PUT" || head.method == "POST" {
        cleanup_expired_files(&dir, STAGING_TTL_SECS).await;

        let Some(total_len) = head.content_length else {
            let err = br#"{"status":"error","message":"Content-Length header is required"}"#;
            super::write_json_response(&mut stream, "411 Length Required", err, true).await;
            return;
        };

        if head.expect_continue {
            let _ = stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").await;
            let _ = stream.flush().await;
        }

        let part_path = dir.join(format!(".{}.part", file_id));
        let Ok(mut file) = tokio::fs::File::create(&part_path).await else {
            let err = br#"{"status":"error","message":"Failed to create staging file"}"#;
            super::write_json_response(&mut stream, "500 Internal Server Error", err, true).await;
            return;
        };

        let mut received: u64 = 0;
        let write_initial = head.remainder.len().min(total_len as usize);
        if write_initial > 0 {
            if file.write_all(&head.remainder[..write_initial]).await.is_err() {
                let _ = tokio::fs::remove_file(&part_path).await;
                return;
            }
            received += write_initial as u64;
        }

        let mut buf = vec![0u8; CHUNK_SIZE];
        while received < total_len {
            let to_read = ((total_len - received) as usize).min(CHUNK_SIZE);
            match stream.read(&mut buf[..to_read]).await {
                Ok(0) => break, // 接続が途中で切れた
                Ok(n) => {
                    if file.write_all(&buf[..n]).await.is_err() {
                        let _ = tokio::fs::remove_file(&part_path).await;
                        return;
                    }
                    received += n as u64;
                }
                Err(_) => break,
            }
        }
        let _ = file.flush().await;
        drop(file);

        if received != total_len {
            let _ = tokio::fs::remove_file(&part_path).await;
            let err = format!(
                r#"{{"status":"error","message":"Incomplete upload ({}/{})"}}"#,
                received, total_len
            );
            super::write_json_response(&mut stream, "400 Bad Request", err.as_bytes(), true).await;
            return;
        }

        let _ = tokio::fs::remove_file(&dest_path).await;
        if let Err(e) = tokio::fs::rename(&part_path, &dest_path).await {
            let _ = tokio::fs::remove_file(&part_path).await;
            let err = format!(r#"{{"status":"error","message":"Failed to finalize file: {}"}}"#, e);
            super::write_json_response(&mut stream, "500 Internal Server Error", err.as_bytes(), true).await;
            return;
        }

        let resp_json = serde_json::json!({
            "status": "ok",
            "id": file_id,
            "size": total_len,
            "url": format!("http://{}:{}/api/files/{}", self_ip, port, file_id),
        })
        .to_string();
        super::write_json_response(&mut stream, "200 OK", resp_json.as_bytes(), true).await;
        return;
    }

    if head.method == "GET" {
        let Ok(mut file) = tokio::fs::File::open(&dest_path).await else {
            let err = br#"{"status":"error","message":"File not found"}"#;
            super::write_json_response(&mut stream, "404 Not Found", err, true).await;
            return;
        };
        let Ok(meta) = file.metadata().await else {
            return;
        };
        let total_len = meta.len();
        let header = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: application/octet-stream\r\n\
             Content-Length: {}\r\n\
             Access-Control-Allow-Origin: *\r\n\
             Cache-Control: no-cache\r\n\
             Connection: close\r\n\r\n",
            total_len
        );
        if stream.write_all(header.as_bytes()).await.is_err() {
            return;
        }
        let mut buf = vec![0u8; CHUNK_SIZE];
        loop {
            match file.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if stream.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = stream.flush().await;
        return;
    }

    if head.method == "DELETE" {
        let _ = tokio::fs::remove_file(&dest_path).await;
        let ok = br#"{"status":"deleted"}"#;
        super::write_json_response(&mut stream, "200 OK", ok, true).await;
        return;
    }

    let err = br#"{"status":"error","message":"Method not allowed"}"#;
    super::write_json_response(&mut stream, "405 Method Not Allowed", err, true).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_file_id() {
        assert!(is_valid_file_id("aviutl_123-video.raw"));
        assert!(is_valid_file_id("out-99.mp4"));
        assert!(!is_valid_file_id(""));
        assert!(!is_valid_file_id(".hidden"));
        assert!(!is_valid_file_id("../secret"));
        assert!(!is_valid_file_id("a/b"));
        assert!(!is_valid_file_id("a\\b"));
    }

    #[tokio::test]
    async fn test_file_staging_put_get_delete_over_tcp() {
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let file_id = format!("unit_test_staging_{}.bin", std::process::id());
        let payload: Vec<u8> = (0u8..=255).cycle().take(150_000).collect();

        let server = tokio::spawn(async move {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let (mut raw, head) = super::super::read_http_head(&mut stream).await.unwrap();
                let _ = &mut raw;
                handle_file_request(stream, head, "127.0.0.1", port).await;
            }
        });

        // 1. PUT with Expect: 100-continue
        {
            let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            let req = format!(
                "PUT /api/files/{} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nExpect: 100-continue\r\n\r\n",
                file_id, payload.len()
            );
            client.write_all(req.as_bytes()).await.unwrap();
            let mut cont = [0u8; 64];
            let cn = client.read(&mut cont).await.unwrap();
            assert!(String::from_utf8_lossy(&cont[..cn]).contains("100 Continue"));

            client.write_all(&payload).await.unwrap();
            let mut resp = String::new();
            client.read_to_string(&mut resp).await.unwrap();
            assert!(resp.contains("200 OK"), "PUT failed: {}", resp);
            assert!(resp.contains(&format!("/api/files/{}", file_id)));
        }

        // 2. GET and verify exact bytes
        {
            let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            let req = format!("GET /api/files/{} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n", file_id);
            client.write_all(req.as_bytes()).await.unwrap();
            let mut full = Vec::new();
            client.read_to_end(&mut full).await.unwrap();
            let pos = full.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
            assert_eq!(&full[pos + 4..], payload.as_slice());
        }

        // 3. DELETE
        {
            let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            let req = format!("DELETE /api/files/{} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n", file_id);
            client.write_all(req.as_bytes()).await.unwrap();
            let mut resp = String::new();
            client.read_to_string(&mut resp).await.unwrap();
            assert!(resp.contains("deleted"));
        }

        server.await.unwrap();
    }
}
