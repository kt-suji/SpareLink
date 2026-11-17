use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn find_asset_with_ext(dir: &Path, ext: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut matches: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some(ext))
        .collect();
    matches.sort();
    matches.into_iter().next()
}

fn emit_const(out: &mut String, name: &str, path: Option<&Path>, fallback: &[u8]) {
    if let Some(p) = path
        && p.is_file()
    {
        println!("cargo:rerun-if-changed={}", p.display());
        let abs = fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let path_str = abs.to_string_lossy();
        let clean = path_str.strip_prefix(r"\\?\").unwrap_or(&path_str);
        out.push_str(&format!(
            "pub const {name}: &[u8] = include_bytes!(r#\"{clean}\"#);\n"
        ));
        return;
    }
    println!(
        "cargo:warning=Frontend asset for {name} not found under ../../front/dist; using placeholder."
    );
    out.push_str(&format!("pub const {name}: &[u8] = {fallback:?};\n"));
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let dist_dir = manifest_dir.join("../../front/dist");
    println!("cargo:rerun-if-changed={}", dist_dir.display());
    let assets_dir = dist_dir.join("assets");
    println!("cargo:rerun-if-changed={}", assets_dir.display());

    let index_html = dist_dir.join("index.html");
    let favicon_svg = dist_dir.join("favicon.svg");
    let icons_svg = dist_dir.join("icons.svg");
    let main_js = find_asset_with_ext(&assets_dir, "js");
    let main_css = find_asset_with_ext(&assets_dir, "css");

    let mut generated = String::new();
    emit_const(
        &mut generated,
        "EMBEDDED_INDEX_HTML",
        Some(&index_html),
        b"<!doctype html><html><body><h1>SpareLink Dashboard Not Built</h1><p>Run <code>cd app/front &amp;&amp; npm install &amp;&amp; npm run build</code> then rebuild the agent.</p></body></html>",
    );
    emit_const(
        &mut generated,
        "EMBEDDED_FAVICON_SVG",
        Some(&favicon_svg),
        b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>",
    );
    emit_const(
        &mut generated,
        "EMBEDDED_ICONS_SVG",
        Some(&icons_svg),
        b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>",
    );
    emit_const(
        &mut generated,
        "EMBEDDED_JS",
        main_js.as_deref(),
        b"console.warn('SpareLink frontend assets not embedded.');",
    );
    emit_const(
        &mut generated,
        "EMBEDDED_CSS",
        main_css.as_deref(),
        b"/* SpareLink frontend CSS not embedded */",
    );

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("embedded_assets.rs"), generated).expect("write embedded_assets.rs");
}
