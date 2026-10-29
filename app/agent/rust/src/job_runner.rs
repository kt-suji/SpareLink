use std::sync::Arc;
use std::time::Instant;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::RwLock;

use crate::config::AppConfig;
use crate::models::{ComputeJob, JobStatus, LocalAgentTelemetryMessage};
use crate::p2p::{P2PMessage, PeerManager};

/**
 * 分散ジョブ実行マネージャー (Job Runner)
 * - タスクの投入・最適な余剰ノードへの自動オフロード
 * - ローカルでのプロセス実行（Python / コマンド）
 * - 実行結果・stdout/stderrの追跡
 */
pub struct JobRunner {
    local_id: String,
    local_name: String,
    jobs: Arc<RwLock<Vec<ComputeJob>>>,
    peer_manager: Arc<PeerManager>,
    config: Arc<RwLock<AppConfig>>,
    local_telemetry: Arc<RwLock<Option<LocalAgentTelemetryMessage>>>,
}

impl JobRunner {
    pub fn new(
        local_id: String,
        local_name: String,
        peer_manager: Arc<PeerManager>,
        config: Arc<RwLock<AppConfig>>,
        local_telemetry: Arc<RwLock<Option<LocalAgentTelemetryMessage>>>,
    ) -> Self {
        Self {
            local_id,
            local_name,
            jobs: Arc::new(RwLock::new(Vec::new())),
            peer_manager,
            config,
            local_telemetry,
        }
    }

    /// 全ジョブの一覧を取得
    pub async fn get_all_jobs(&self) -> Vec<ComputeJob> {
        let lock = self.jobs.read().await;
        lock.clone()
    }

    /// ジョブ情報の更新または追加
    pub async fn update_job(&self, job: ComputeJob) {
        let mut lock = self.jobs.write().await;
        if let Some(idx) = lock.iter().position(|j| j.id == job.id) {
            lock[idx] = job;
        } else {
            lock.insert(0, job);
        }
    }

    /// 新規ジョブの投入・実行先決定・オフロード
    pub async fn submit_job(&self, mut job: ComputeJob) -> Result<ComputeJob, Box<dyn std::error::Error + Send + Sync>> {
        job.submitted_at = chrono_or_fallback_now();
        job.requester_node_id = self.local_id.clone();
        job.requester_node_name = self.local_name.clone();
        job.status = JobStatus::Pending;

        // 実行対象ノードの決定
        let target_node_id = job.target_node_id.clone();
        let (selected_id, selected_name, is_local) = if target_node_id == "auto" {
            self.select_best_surplus_node().await
        } else if target_node_id == self.local_id || target_node_id == "local" {
            (self.local_id.clone(), self.local_name.clone(), true)
        } else {
            let peers = self.peer_manager.get_all_peers().await;
            let peer = peers.into_iter().find(|p| p.id == target_node_id || p.name == target_node_id);
            if let Some(p) = peer {
                (p.id, p.name, false)
            } else {
                (self.local_id.clone(), self.local_name.clone(), true)
            }
        };

        job.assigned_node_id = Some(selected_id.clone());
        job.assigned_node_name = Some(selected_name.clone());

        // 内部リストに追加
        self.update_job(job.clone()).await;

        if is_local {
            println!(
                "[SpareLink Job] ジョブ '{}' ({}) を自PCで実行開始します",
                job.name, job.id
            );
            let runner_clone = self.clone_runner();
            let job_clone = job.clone();
            tokio::spawn(async move {
                runner_clone.execute_local_job(job_clone).await;
            });
        } else {
            println!(
                "[SpareLink Job] ジョブ '{}' ({}) を余剰トップノード '{}' ({}) へオフロード転送します",
                job.name, job.id, selected_name, selected_id
            );
            // リモートピアへWebSocket経由で送信
            let msg = P2PMessage::JobSubmit { job: job.clone() };
            if let Err(e) = self.peer_manager.send_p2p_message_to_peer(&selected_id, msg).await {
                eprintln!("[SpareLink Job] リモート転送失敗: {}. 自PCへフォールバックします。", e);
                job.assigned_node_id = Some(self.local_id.clone());
                job.assigned_node_name = Some(self.local_name.clone());
                self.update_job(job.clone()).await;
                let runner_clone = self.clone_runner();
                let job_clone = job.clone();
                tokio::spawn(async move {
                    runner_clone.execute_local_job(job_clone).await;
                });
            }
        }

        Ok(job)
    }

    /// 最も余剰計算資源（CPU pt）が高いオンラインノードを選定
    async fn select_best_surplus_node(&self) -> (String, String, bool) {
        let mut best_id = self.local_id.clone();
        let mut best_name = self.local_name.clone();
        let mut best_score = 0u32;
        let mut is_local = true;

        // 自PCの余剰スコア
        if let Some(ref telem) = *self.local_telemetry.read().await {
            best_score = telem.surplus.allocatable_cpu_score.unwrap_or(0);
        }

        // 全ピアの余剰スコアを比較
        let peers = self.peer_manager.get_all_peers().await;
        for p in peers {
            if p.status == "online" {
                let peer_surplus = p.surplus.allocatable_cpu_score.unwrap_or(0);
                if peer_surplus > best_score {
                    best_score = peer_surplus;
                    best_id = p.id;
                    best_name = p.name;
                    is_local = false;
                }
            }
        }

        (best_id, best_name, is_local)
    }

    /// リモートPCから受け取ったジョブを実行
    pub async fn handle_remote_job_submit(&self, mut job: ComputeJob) {
        println!(
            "[SpareLink Job] リモートノード '{}' からジョブ要求を受信: {} ({})",
            job.requester_node_name, job.name, job.id
        );

        // 安全チェック：自PCが高負荷の場合は拒否
        let is_overloaded = {
            let cfg = self.config.read().await;
            let telem = self.local_telemetry.read().await;
            if let Some(ref t) = *telem {
                t.cpu_percent >= cfg.evaluator.high_load_threshold
            } else {
                false
            }
        };

        if is_overloaded {
            println!("[SpareLink Job] 自PCが高負荷閾値超過のため、ジョブ要求を拒否します");
            job.status = JobStatus::Failed;
            job.stderr = "ホストPCのCPU使用率が安全閾値を超えているため、ジョブを実行できません".to_string();
            job.completed_at = Some(chrono_or_fallback_now());
            self.update_job(job.clone()).await;

            // 送信元へ通知
            let requester_id = job.requester_node_id.clone();
            let _ = self.peer_manager.send_p2p_message_to_peer(
                &requester_id,
                P2PMessage::JobUpdate { job },
            ).await;
            return;
        }

        job.assigned_node_id = Some(self.local_id.clone());
        job.assigned_node_name = Some(self.local_name.clone());
        self.update_job(job.clone()).await;

        let runner_clone = self.clone_runner();
        tokio::spawn(async move {
            runner_clone.execute_local_job(job).await;
        });
    }

    /// ローカルプロセスでジョブを実行し、結果を記録・通知
    async fn execute_local_job(&self, mut job: ComputeJob) {
        job.status = JobStatus::Running;
        job.started_at = Some(chrono_or_fallback_now());
        self.update_job(job.clone()).await;

        // リモートジョブなら実行中ステータスを送信元へ通知
        if job.requester_node_id != self.local_id {
            let _ = self.peer_manager.send_p2p_message_to_peer(
                &job.requester_node_id,
                P2PMessage::JobUpdate { job: job.clone() },
            ).await;
        }

        let start_time = Instant::now();
        let exec_result = run_os_command(&job.job_type, &job.command).await;
        let duration_secs = start_time.elapsed().as_secs_f64();

        job.completed_at = Some(chrono_or_fallback_now());
        match exec_result {
            Ok((code, stdout, stderr)) => {
                job.exit_code = Some(code);
                job.stdout = stdout;
                job.stderr = if stderr.is_empty() {
                    format!("完了 (所要時間: {:.2}秒)", duration_secs)
                } else {
                    format!("{}\n(所要時間: {:.2}秒)", stderr, duration_secs)
                };
                job.status = if code == 0 {
                    JobStatus::Completed
                } else {
                    JobStatus::Failed
                };
            }
            Err(e) => {
                job.status = JobStatus::Failed;
                job.stderr = format!("プロセス起動エラー: {}\n(所要時間: {:.2}秒)", e, duration_secs);
            }
        }

        println!(
            "[SpareLink Job] ジョブ '{}' ({}) 終了: {:?} (所要時間: {:.2}s)",
            job.name, job.id, job.status, duration_secs
        );

        self.update_job(job.clone()).await;

        // リモートジョブなら完了結果を送信元へ返却
        if job.requester_node_id != self.local_id {
            let requester_id = job.requester_node_id.clone();
            let _ = self.peer_manager.send_p2p_message_to_peer(
                &requester_id,
                P2PMessage::JobUpdate { job },
            ).await;
        }
    }

    fn clone_runner(&self) -> Self {
        Self {
            local_id: self.local_id.clone(),
            local_name: self.local_name.clone(),
            jobs: self.jobs.clone(),
            peer_manager: self.peer_manager.clone(),
            config: self.config.clone(),
            local_telemetry: self.local_telemetry.clone(),
        }
    }
}

/// OSコマンドまたはPythonコードを実行
async fn run_os_command(job_type: &str, command_str: &str) -> Result<(i32, String, String), Box<dyn std::error::Error + Send + Sync>> {
    let mut cmd = if job_type == "python" {
        let mut c = Command::new("python");
        c.arg("-c").arg(command_str);
        c
    } else {
        #[cfg(target_os = "windows")]
        {
            let mut c = Command::new("cmd");
            c.args(["/C", command_str]);
            c
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut c = Command::new("sh");
            c.arg("-c").arg(command_str);
            c
        }
    };

    #[cfg(target_os = "windows")]
    {
        // コンソールウィンドウを出さずにバックグラウンド実行
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn()?;

    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();

    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_end(&mut stdout_buf).await;
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_end(&mut stderr_buf).await;
    }

    let status = child.wait().await?;
    let code = status.code().unwrap_or(-1);

    let stdout = String::from_utf8_lossy(&stdout_buf).to_string();
    let stderr = String::from_utf8_lossy(&stderr_buf).to_string();

    Ok((code, stdout, stderr))
}

fn chrono_or_fallback_now() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // 簡易時刻文字列
    let secs_in_day = now % 86400;
    let hours = (secs_in_day / 3600 + 9) % 24; // JST (+9)
    let minutes = (secs_in_day % 3600) / 60;
    let seconds = secs_in_day % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_run_os_command() {
        let res = run_os_command("command", "echo HelloSpareLink").await;
        assert!(res.is_ok());
        let (code, stdout, _stderr) = res.unwrap();
        assert_eq!(code, 0);
        assert!(stdout.contains("HelloSpareLink"));
    }

    #[tokio::test]
    async fn test_job_submit_and_execute() {
        use crate::discovery::{create_discovery_provider, DiscoveryConfig, TransportKind};
        let disc = create_discovery_provider(&DiscoveryConfig {
            transport: TransportKind::Static,
            default_p2p_port: 3001,
            wireguard_config_path: None,
            static_peers: vec![],
        });
        let local_telemetry = Arc::new(RwLock::new(None));
        let pm = Arc::new(PeerManager::new("test-node".to_string(), disc, local_telemetry.clone()));
        let config = Arc::new(RwLock::new(AppConfig::default()));

        let runner = JobRunner::new(
            "test-node".to_string(),
            "Test Node".to_string(),
            pm,
            config,
            local_telemetry,
        );

        let job = ComputeJob {
            id: "test-job-1".to_string(),
            name: "Echo Test".to_string(),
            command: "echo 42".to_string(),
            job_type: "command".to_string(),
            target_node_id: "local".to_string(),
            assigned_node_id: None,
            assigned_node_name: None,
            requester_node_id: String::new(),
            requester_node_name: String::new(),
            status: JobStatus::Pending,
            submitted_at: String::new(),
            started_at: None,
            completed_at: None,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
        };

        let submitted = runner.submit_job(job).await.expect("submit failed");
        assert_eq!(submitted.assigned_node_id, Some("test-node".to_string()));

        // 少し待機してジョブ完了を確認
        tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;
        let all_jobs = runner.get_all_jobs().await;
        assert_eq!(all_jobs.len(), 1);
        assert_eq!(all_jobs[0].status, JobStatus::Completed);
        assert!(all_jobs[0].stdout.contains("42"));
    }
}
