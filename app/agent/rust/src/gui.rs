use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use eframe::egui::{self, Align, Color32, Layout, ProgressBar, RichText, Rounding, Stroke, Vec2};

use crate::discovery::tailscale::open_browser;
use crate::models::{LocalAgentTelemetryMessage, TailscaleState};

#[cfg(target_os = "windows")]
unsafe extern "system" {
    fn GetSystemMetrics(nIndex: i32) -> i32;
}

const SM_CXSCREEN: i32 = 0;
const SM_CYSCREEN: i32 = 1;

/// 画面解像度からタスクバー右上の初期ウィンドウ位置（X, Y）を計算する
pub fn get_bottom_right_position(width: f32, height: f32) -> egui::Pos2 {
    #[cfg(target_os = "windows")]
    unsafe {
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        if screen_w > 0 && screen_h > 0 {
            let x = (screen_w as f32 - width - 24.0).max(0.0);
            let y = (screen_h as f32 - height - 60.0).max(0.0); // タスクバーの約60px上
            return egui::pos2(x, y);
        }
    }
    egui::pos2(100.0, 100.0)
}

/// SpareLink 右下常駐ネイティブモニターGUI
pub struct SpareLinkMonitorApp {
    pub telemetry_holder: Arc<RwLock<Option<LocalAgentTelemetryMessage>>>,
    pub sharing_paused: Arc<AtomicBool>,
    pub always_on_top: bool,
    pub cached_telemetry: Option<LocalAgentTelemetryMessage>,
}

impl SpareLinkMonitorApp {
    pub fn new(
        telemetry_holder: Arc<RwLock<Option<LocalAgentTelemetryMessage>>>,
        sharing_paused: Arc<AtomicBool>,
    ) -> Self {
        Self {
            telemetry_holder,
            sharing_paused,
            always_on_top: true,
            cached_telemetry: None,
        }
    }

    /// サイバーパンク・ダークミニマルテーマを設定する
    pub fn configure_theme(ctx: &egui::Context) {
        let mut visuals = egui::Visuals::dark();

        // 背景色（黒に近いディープスレート）
        visuals.panel_fill = Color32::from_rgb(15, 23, 42); // #0f172a
        visuals.window_fill = Color32::from_rgb(15, 23, 42);

        // 各種ウィジェット背景
        visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(24, 33, 49);
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(30, 41, 59);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(51, 65, 85);
        visuals.widgets.active.bg_fill = Color32::from_rgb(6, 182, 212); // #06b6d4

        // 枠線とアクセント
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(51, 65, 85));
        visuals.selection.bg_fill = Color32::from_rgb(6, 182, 212);

        ctx.set_visuals(visuals);
    }
}

impl eframe::App for SpareLinkMonitorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 定期的にテレメトリホルダーから最新実測値を取得（非同期ブロック回避のため try_read）
        if let Ok(lock) = self.telemetry_holder.try_read() {
            if let Some(ref telem) = *lock {
                self.cached_telemetry = Some(telem.clone());
            }
        }

        let is_paused = self.sharing_paused.load(Ordering::SeqCst);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 8.0);

            // 1. ヘッダーバー (SpareLink Core + 最前面ピン)
            ui.horizontal(|ui| {
                ui.label(RichText::new("■").color(Color32::from_rgb(6, 182, 212)).size(13.0));
                ui.label(RichText::new("SpareLink").strong().size(14.0).color(Color32::WHITE));
                ui.label(RichText::new("Core v0.1.0").monospace().size(10.5).color(Color32::from_rgb(148, 163, 184)));

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.selectable_label(self.always_on_top, "📌 最前面").clicked() {
                        self.always_on_top = !self.always_on_top;
                        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                            if self.always_on_top {
                                egui::WindowLevel::AlwaysOnTop
                            } else {
                                egui::WindowLevel::Normal
                            },
                        ));
                    }
                });
            });

            ui.separator();

            // 2. 自PCリソース情報カード
            if let Some(ref telem) = self.cached_telemetry {
                egui::Frame::none()
                    .fill(Color32::from_rgb(24, 33, 49))
                    .rounding(Rounding::same(4.0))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(39, 52, 75)))
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("💻 自PC:").size(11.5).color(Color32::from_rgb(148, 163, 184)));
                            ui.label(RichText::new(&telem.pc_name).strong().color(Color32::WHITE));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if let Some(score) = telem.cpu_benchmark_score {
                                    ui.label(RichText::new(format!("Base: {} pt", score)).size(10.5).color(Color32::from_rgb(100, 116, 139)));
                                }
                            });
                        });

                        ui.add_space(2.0);

                        // CPU使用率
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("CPU").monospace().size(11.0).color(Color32::from_rgb(148, 163, 184)));
                            let cpu_frac = (telem.cpu_percent / 100.0).clamp(0.0, 1.0) as f32;
                            let bar = ProgressBar::new(cpu_frac)
                                .text(format!("{:>4.1}%", telem.cpu_percent))
                                .fill(if telem.cpu_percent > 80.0 {
                                    Color32::from_rgb(239, 68, 68) // 赤
                                } else {
                                    Color32::from_rgb(6, 182, 212) // シアン
                                });
                            ui.add(bar);
                        });

                        // メモリ使用率
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("MEM").monospace().size(11.0).color(Color32::from_rgb(148, 163, 184)));
                            let mem_frac = (telem.memory_percent / 100.0).clamp(0.0, 1.0) as f32;
                            let used_gb = (telem.memory_total_gb - telem.memory_available_gb).max(0.0);
                            let bar = ProgressBar::new(mem_frac)
                                .text(format!("{:.1}/{:.1}GB ({:.0}%)", used_gb, telem.memory_total_gb, telem.memory_percent))
                                .fill(Color32::from_rgb(59, 130, 246));
                            ui.add(bar);
                        });

                        ui.add_space(4.0);

                        // 余剰ポイントハイライトボックス
                        egui::Frame::none()
                            .fill(if is_paused {
                                Color32::from_rgba_premultiplied(245, 158, 11, 20)
                            } else {
                                Color32::from_rgba_premultiplied(34, 197, 94, 25)
                            })
                            .rounding(Rounding::same(3.0))
                            .inner_margin(6.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if is_paused {
                                        ui.label(RichText::new("⏸ 共有一時停止中").strong().size(12.0).color(Color32::from_rgb(245, 158, 11)));
                                    } else {
                                        let surplus_pt = telem.surplus.allocatable_cpu_score.unwrap_or(0);
                                        let surplus_mem = telem.surplus.allocatable_memory_gb.unwrap_or(0.0);
                                        ui.label(RichText::new("余剰CPU:").size(11.0).color(Color32::from_rgb(148, 163, 184)));
                                        ui.label(RichText::new(format!("{} pt", surplus_pt)).strong().size(14.0).color(Color32::from_rgb(34, 197, 94)));
                                        ui.label(RichText::new(format!("(RAM: {:.1}GB)", surplus_mem)).size(11.0).color(Color32::from_rgb(148, 163, 184)));
                                    }
                                });
                            });
                    });
            } else {
                ui.label(RichText::new("システム情報を取得中...").italics().color(Color32::from_rgb(148, 163, 184)));
            }

            // 3. Tailscale & P2P ネットワークカード
            egui::Frame::none()
                .fill(Color32::from_rgb(24, 33, 49))
                .rounding(Rounding::same(4.0))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(39, 52, 75)))
                .inner_margin(8.0)
                .show(ui, |ui| {
                    // Tailscaleステータス
                    if let Some(ref telem) = self.cached_telemetry {
                        if let Some(ref ts) = telem.tailscale {
                            ui.horizontal(|ui| {
                                match ts.state {
                                    TailscaleState::Running => {
                                        ui.label(RichText::new("●").color(Color32::from_rgb(34, 197, 94)).size(10.0));
                                        ui.label(RichText::new("Tailscale:").size(11.0).color(Color32::from_rgb(148, 163, 184)));
                                        ui.label(RichText::new(format!("接続中 ({})", ts.self_ip.as_deref().unwrap_or("100.x"))).size(11.0).color(Color32::WHITE));
                                    }
                                    TailscaleState::NeedsLogin => {
                                        ui.label(RichText::new("▲").color(Color32::from_rgb(245, 158, 11)).size(10.0));
                                        ui.label(RichText::new("Tailscale: ログイン必要").color(Color32::from_rgb(245, 158, 11)).size(11.0));
                                        if let Some(ref url) = ts.auth_url {
                                            if ui.small_button("認証を開く ↗").clicked() {
                                                open_browser(url);
                                            }
                                        }
                                    }
                                    TailscaleState::NotInstalled => {
                                        ui.label(RichText::new("✕").color(Color32::from_rgb(239, 68, 68)).size(10.0));
                                        ui.label(RichText::new("Tailscale: 未インストール").color(Color32::from_rgb(239, 68, 68)).size(11.0));
                                        if ui.small_button("公式DL ↗").clicked() {
                                            open_browser("https://tailscale.com/download");
                                        }
                                    }
                                    _ => {
                                        ui.label(RichText::new("○").color(Color32::GRAY).size(10.0));
                                        ui.label(RichText::new("Tailscale: 停止中").size(11.0).color(Color32::GRAY));
                                    }
                                }
                            });
                        }
                    }

                    ui.separator();

                    // ピア一覧ヘッダー
                    let peers = self.cached_telemetry.as_ref().and_then(|t| t.peers.as_ref());
                    let online_count = peers.map(|p| p.iter().filter(|n| n.status == "online").count()).unwrap_or(0);
                    let total_count = peers.map(|p| p.len()).unwrap_or(0);

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("P2P ピア一覧").strong().size(11.5).color(Color32::WHITE));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(format!("{}/{} 台オンライン", online_count, total_count)).size(11.0).color(Color32::from_rgb(148, 163, 184)));
                        });
                    });

                    ui.add_space(2.0);

                    // ピアリストスクロール領域
                    egui::ScrollArea::vertical()
                        .max_height(110.0)
                        .show(ui, |ui| {
                            if let Some(peer_list) = peers {
                                if peer_list.is_empty() {
                                    ui.label(RichText::new("探索中...").italics().size(11.0).color(Color32::from_rgb(100, 116, 139)));
                                } else {
                                    for p in peer_list {
                                        let is_online = p.status == "online";
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new("●").size(9.0).color(if is_online {
                                                Color32::from_rgb(34, 197, 94)
                                            } else {
                                                Color32::from_rgb(100, 116, 139)
                                            }));
                                            ui.label(RichText::new(&p.name).strong().size(11.5).color(if is_online {
                                                Color32::WHITE
                                            } else {
                                                Color32::from_rgb(148, 163, 184)
                                            }));

                                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                if is_online {
                                                    if let Some(score) = p.surplus.allocatable_cpu_score {
                                                        ui.label(RichText::new(format!("{} pt", score)).strong().size(11.5).color(Color32::from_rgb(34, 197, 94)));
                                                    }
                                                } else {
                                                    ui.label(RichText::new("offline").size(10.5).color(Color32::from_rgb(100, 116, 139)));
                                                }
                                            });
                                        });
                                    }
                                }
                            }
                        });
                });

            // 4. アクションコントロールバー（下部）
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                // 一時停止 / 再開ボタン
                let pause_text = if is_paused { "▶ 共有を再開" } else { "⏸ 一時停止" };
                if ui.button(pause_text).clicked() {
                    self.sharing_paused.store(!is_paused, Ordering::SeqCst);
                }

                // ブラウザ詳細画面を開くボタン
                if ui.button("🌐 Web詳細 ↗").clicked() {
                    open_browser("http://localhost:5173");
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("終了").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        });

        // アイドル時のCPU負荷を抑えるため、500ms（毎秒2回）の再描画をスケジュール
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}
