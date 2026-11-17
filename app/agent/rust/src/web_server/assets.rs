use std::path::PathBuf;

pub const EMBEDDED_INDEX_HTML: &[u8] = include_bytes!("../../../../front/dist/index.html");
pub const EMBEDDED_FAVICON_SVG: &[u8] = include_bytes!("../../../../front/dist/favicon.svg");
pub const EMBEDDED_ICONS_SVG: &[u8] = include_bytes!("../../../../front/dist/icons.svg");
pub const EMBEDDED_JS: &[u8] = include_bytes!("../../../../front/dist/assets/index-lcp2xXp-.js");
pub const EMBEDDED_CSS: &[u8] = include_bytes!("../../../../front/dist/assets/index-UFxAU9hj.css");

pub fn get_content_type(path: &str) -> &'static str {
    if path.ends_with(".html") || path == "/" {
        "text/html; charset=utf-8"
    } else if path.ends_with(".js") {
        "application/javascript; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".json") {
        "application/json"
    } else {
        "application/octet-stream"
    }
}

pub fn try_read_file_from_disk(rel_path: &str) -> Option<Vec<u8>> {
    let clean_path = rel_path.trim_start_matches('/');
    let candidates = [
        PathBuf::from("app/front/dist").join(clean_path),
        PathBuf::from("../front/dist").join(clean_path),
        PathBuf::from("../../front/dist").join(clean_path),
        PathBuf::from("dist").join(clean_path),
    ];

    for path in &candidates {
        if path.is_file()
            && let Ok(data) = std::fs::read(path)
        {
            return Some(data);
        }
    }
    None
}

pub fn resolve_asset(req_path: &str) -> (&'static str, Vec<u8>) {
    let raw_path = req_path.split('?').next().unwrap_or("/");
    let path = if raw_path.is_empty() || raw_path == "/" {
        "/index.html"
    } else {
        raw_path
    };

    // 1. ディスクからの最新読み込みを試行
    if let Some(disk_data) = try_read_file_from_disk(path) {
        return (get_content_type(path), disk_data);
    }

    // 2. 内蔵アセットからの読み込み（単一バイナリ配布用フォールバック）
    match path {
        "/favicon.svg" => ("image/svg+xml", EMBEDDED_FAVICON_SVG.to_vec()),
        "/icons.svg" => ("image/svg+xml", EMBEDDED_ICONS_SVG.to_vec()),
        p if p.ends_with(".js") => ("application/javascript; charset=utf-8", EMBEDDED_JS.to_vec()),
        p if p.ends_with(".css") => ("text/css; charset=utf-8", EMBEDDED_CSS.to_vec()),
        _ => ("text/html; charset=utf-8", EMBEDDED_INDEX_HTML.to_vec()),
    }
}
