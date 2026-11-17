use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{RwLock, broadcast};
use tokio_tungstenite::tungstenite::Message;

use crate::job_runner::JobRunner;
use crate::p2p::{P2PMessage, PeerManager};
use crate::updater;

/// WebSocket API & P2P リスナーの待ち受けループ
pub async fn serve(
    listener: TcpListener,
    peer_manager: Arc<PeerManager>,
    job_runner: Arc<JobRunner>,
    latest_metrics: Arc<RwLock<Option<String>>>,
    tx: broadcast::Sender<String>,
) {
    while let Ok((stream, peer_addr)) = listener.accept().await {
        let _ = stream.set_nodelay(true);
        let rx = tx.subscribe();
        tokio::spawn(handle_client(
            stream,
            peer_addr,
            rx,
            latest_metrics.clone(),
            peer_manager.clone(),
            job_runner.clone(),
        ));
    }
}

async fn handle_client(
    stream: TcpStream,
    peer_addr: SocketAddr,
    mut rx: broadcast::Receiver<String>,
    latest_metrics: Arc<RwLock<Option<String>>>,
    peer_manager: Arc<PeerManager>,
    job_runner: Arc<JobRunner>,
) {
    let mut ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("[SpareLink] WebSocket handshake error from {}: {}", peer_addr, e);
            return;
        }
    };

    if let Some(ref json) = *latest_metrics.read().await {
        let _ = ws_stream.send(Message::Text(json.clone().into())).await;
    }

    loop {
        tokio::select! {
            msg_opt = ws_stream.next() => {
                match msg_opt {
                    Some(Ok(Message::Text(text))) => {
                        handle_text_message(&mut ws_stream, &text, &peer_manager, &job_runner).await;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            telemetry_res = rx.recv() => {
                match telemetry_res {
                    Ok(json) => {
                        if ws_stream.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    }
}

async fn handle_text_message(
    ws_stream: &mut tokio_tungstenite::WebSocketStream<TcpStream>,
    text: &str,
    peer_manager: &PeerManager,
    job_runner: &JobRunner,
) {
    let Ok(p2p_msg) = serde_json::from_str::<P2PMessage>(text) else {
        return;
    };
    match p2p_msg {
        P2PMessage::Ping { sender_id, sender_name, telemetry, .. } => {
            if let Some(pong) = peer_manager.handle_incoming_ping(sender_id, sender_name, telemetry).await
                && let Ok(pong_json) = serde_json::to_string(&pong)
            {
                let _ = ws_stream.send(Message::Text(pong_json.into())).await;
            }
        }
        P2PMessage::JobSubmit { job } => {
            job_runner.handle_remote_job_submit(job).await;
        }
        P2PMessage::JobUpdate { job } => {
            job_runner.update_job(job).await;
        }
        P2PMessage::NodeUpdate { target_version, download_url } => {
            println!("[SpareLink] 受信: 自動アップデート要求 (バージョン: {:?})", target_version);
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                if let Err(e) = updater::perform_self_update(download_url.as_deref()).await {
                    eprintln!("[SpareLink Update] 自動アップデート失敗: {}", e);
                }
            });
        }
        _ => {}
    }
}
