use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use crate::p2p::{P2PMessage, PeerManager};

/// 起動時に古いアップデート残骸（*.old.exe）をクリーンアップする
pub fn cleanup_old_binaries() {
    if let Ok(current_exe) = std::env::current_exe() {
        let old_exe = current_exe.with_extension("old.exe");
        if old_exe.exists() {
            let _ = std::fs::remove_file(old_exe);
        }
        let old_bak = current_exe.with_extension("bak.exe");
        if old_bak.exists() {
            let _ = std::fs::remove_file(old_bak);
        }
    }
}

/// Googleドライブなどの共有フォルダから直接実行されている場合、
/// 共有ファイルの排他ロックを避けるためローカルの `%LOCALAPPDATA%\SpareLink\bin` へ自己移管して実行する。
/// ※ 開発環境（カレントディレクトリがgit等で管理されている場合）は移管をスキップする。
pub fn ensure_local_execution_if_needed() -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    // すでにローカル移管済みプロセスの場合はスキップ
    if std::env::var("SPARELINK_LOCAL_RUNNER").is_ok() {
        return Ok(false);
    }

    let current_exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return Ok(false),
    };
    let current_exe_str = current_exe.to_string_lossy().to_lowercase();

    // 開発リポジトリ内（target/debug, target/release）の場合はスキップ
    if current_exe_str.contains("target\\debug") || current_exe_str.contains("target\\release") {
        return Ok(false);
    }

    // Googleドライブやdist-agentなどの共有フォルダ内から起動されているかチェック
    let is_shared_dir = current_exe_str.contains("マイドライブ")
        || current_exe_str.contains("my drive")
        || current_exe_str.contains("dist-agent")
        || current_exe_str.contains("google drive");

    if !is_shared_dir {
        return Ok(false);
    }

    // ローカル AppData 領域のパスを取得
    #[cfg(target_os = "windows")]
    let appdata = std::env::var("LOCALAPPDATA")
        .unwrap_or_else(|_| "C:\\ProgramData".to_string());
    #[cfg(not(target_os = "windows"))]
    let appdata = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());

    let target_dir = PathBuf::from(appdata).join("SpareLink").join("bin");
    let _ = std::fs::create_dir_all(&target_dir);

    let target_exe = target_dir.join("sparelink-agent.exe");

    // 自身をローカルへコピー
    if let Err(e) = std::fs::copy(&current_exe, &target_exe) {
        eprintln!("[SpareLink Updater] ローカルキャッシュへのコピーをスキップします: {}", e);
        return Ok(false);
    }

    // 設定ファイル sparelink.toml が同階層にあればコピー
    if let Some(parent) = current_exe.parent() {
        let config_src = parent.join("sparelink.toml");
        if config_src.exists() {
            let config_dst = target_dir.join("sparelink.toml");
            let _ = std::fs::copy(config_src, config_dst);
        }
    }

    println!(
        "[SpareLink Updater] 共有フォルダのロック回避のため、ローカルキャッシュへ移管して再起動します: {:?}",
        target_exe
    );

    // ローカルのexeを起動して自身は終了
    let mut cmd = Command::new(&target_exe);
    cmd.args(std::env::args().skip(1));
    cmd.env("SPARELINK_LOCAL_RUNNER", "1");
    if cmd.spawn().is_ok() {
        std::process::exit(0);
    }

    Ok(false)
}

/// 自エージェントのバイナリを最新版に更新し、再起動する
pub async fn perform_self_update(
    download_url: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let current_exe = std::env::current_exe()?;
    let new_exe = current_exe.with_extension("new.exe");
    let old_exe = current_exe.with_extension("old.exe");

    println!("[SpareLink Updater] 自己アップデートを開始します (現exe: {:?})", current_exe);

    // 1. 最新バイナリの調達
    let mut download_success = false;

    // A. URL指定がある場合は PowerShell でダウンロード
    if let Some(url) = download_url {
        println!("[SpareLink Updater] URLから最新バイナリを取得試行: {}", url);
        let ps_cmd = format!(
            "Invoke-WebRequest -Uri '{}' -OutFile '{}' -TimeoutSec 20",
            url,
            new_exe.to_string_lossy()
        );
        let output = Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps_cmd])
            .output();

        if let Ok(out) = output {
            if out.status.success() && new_exe.exists() {
                if let Ok(meta) = std::fs::metadata(&new_exe) {
                    if meta.len() > 1_000_000 {
                        download_success = true;
                    }
                }
            }
        }
    }

    // B. URLダウンロードが失敗または未指定の場合、Googleドライブなどの共有パスを探してコピー
    if !download_success {
        println!("[SpareLink Updater] ローカル共有フォルダ (dist-agent) からの更新を試行します...");
        let search_paths = [
            PathBuf::from("G:\\マイドライブ\\SpareLink\\dist-agent\\sparelink-agent.exe"),
            PathBuf::from("G:\\My Drive\\SpareLink\\dist-agent\\sparelink-agent.exe"),
            PathBuf::from("dist-agent\\sparelink-agent.exe"),
            PathBuf::from("..\\dist-agent\\sparelink-agent.exe"),
        ];

        for path in &search_paths {
            if path.exists() && path != &current_exe {
                if let Ok(meta) = std::fs::metadata(path) {
                    if meta.len() > 1_000_000 {
                        if std::fs::copy(path, &new_exe).is_ok() {
                            download_success = true;
                            println!("[SpareLink Updater] 共有パス {:?} から最新バイナリをコピーしました", path);
                            break;
                        }
                    }
                }
            }
        }
    }

    if !download_success || !new_exe.exists() {
        return Err("最新バイナリを取得できませんでした (URLおよび共有フォルダ両方で未発見)".into());
    }

    // 2. Windows In-Place リネーム置換
    // 既存の .old を削除
    if old_exe.exists() {
        let _ = std::fs::remove_file(&old_exe);
    }

    // 実行中exeを .old にリネーム（Windowsは実行中exeのリネームを許可）
    std::fs::rename(&current_exe, &old_exe)?;

    // 新バイナリを正規のパスに移動
    std::fs::rename(&new_exe, &current_exe)?;

    println!("[SpareLink Updater] バイナリ置換完了。新バージョンを起動します: {:?}", current_exe);

    // 3. 新バイナリを別プロセスで起動
    let mut cmd = Command::new(&current_exe);
    cmd.args(std::env::args().skip(1));
    cmd.env("SPARELINK_LOCAL_RUNNER", "1");
    cmd.spawn()?;

    println!("[SpareLink Updater] 新プロセスの起動に成功しました。旧プロセスを正常終了します。");
    std::process::exit(0);
}

/// クラスタ内の全オンラインピアへ更新要求を一括送信する
pub async fn trigger_cluster_update(
    peer_manager: &Arc<PeerManager>,
    target_version: &str,
    download_url: Option<String>,
) -> Vec<String> {
    let peers = peer_manager.get_all_peers().await;
    let mut updated_peers = Vec::new();

    let msg = P2PMessage::NodeUpdate {
        target_version: target_version.to_string(),
        download_url: download_url.clone(),
    };

    for peer in peers {
        if peer.status == "online" {
            let peer_id = peer.id.clone();
            let msg_clone = msg.clone();
            let pm_clone = Arc::clone(peer_manager);

            tokio::spawn(async move {
                let _ = pm_clone.send_p2p_message_to_peer(&peer_id, msg_clone).await;
            });
            updated_peers.push(peer.name);
        }
    }

    updated_peers
}
