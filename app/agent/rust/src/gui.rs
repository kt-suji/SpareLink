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

    /// SpareLink 公式テーマ（黒ネイビー #0a0e15 × 赤アクセント #f0644f）および日本語フォントの設定
    pub fn configure_theme(ctx: &egui::Context) {
        // 1. 日本語フォント（Windows Meiryo / Yu Gothic / MS Gothic）の自動設定
        let font_candidates = [
            "C:\\Windows\\Fonts\\meiryo.ttc",
            "C:\\Windows\\Fonts\\YuGothM.ttc",
            "C:\\Windows\\Fonts\\YuGothR.ttc",
            "C:\\Windows\\Fonts\\msgothic.ttc",
            "C:\\Windows\\Fonts\\BIZ-UDGothicR.ttc",
        ];

        for path in &font_candidates {
            if let Ok(data) = std::fs::read(path) {
                let mut fonts = egui::FontDefinitions::default();
                fonts.font_data.insert("windows_jp".to_owned(), egui::FontData::from_owned(data));
                if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                    family.insert(0, "windows_jp".to_owned());
                }
                if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                    family.push("windows_jp".to_owned());
                }
                ctx.set_fonts(fonts);
                break;
            }
        }

        // 2. SpareLink 標準ビジュアルテーマ（赤アクセント #f0644f）
        let mut visuals = egui::Visuals::dark();

        // 背景色（--bg: #0a0e15）
        visuals.panel_fill = Color32::from_rgb(10, 14, 21);
        visuals.window_fill = Color32::from_rgb(10, 14, 21);

        // 各種ウィジェット背景（--panel: #151a22, --panel-raised: #1b212b）
        visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(21, 26, 34);
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(42, 49, 60)); // --line: #2a313c

        visuals.widgets.inactive.bg_fill = Color32::from_rgb(27, 33, 43);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(42, 49, 60));

        visuals.widgets.hovered.bg_fill = Color32::from_rgb(38, 45, 56);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(240, 100, 79)); // 赤ボーダー

        visuals.widgets.active.bg_fill = Color32::from_rgb(240, 100, 79); // --accent: #f0644f
        visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(255, 120, 101));

        visuals.selection.bg_fill = Color32::from_rgb(240, 100, 79);
        visuals.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(255, 120, 101));

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

        // SpareLink テーマカラー定数
        let accent_red = Color32::from_rgb(240, 100, 79);      // #f0644f (赤アクセント)
        let text_main = Color32::from_rgb(230, 233, 239);       // #e6e9ef
        let text_muted = Color32::from_rgb(138, 147, 162);      // #8a93a2
        let card_bg = Color32::from_rgb(21, 26, 34);            // #151a22
        let card_border = Color32::from_rgb(42, 49, 60);        // #2a313c
        let status_ok = Color32::from_rgb(79, 179, 132);        // #4fb384 (緑)
        let status_warn = Color32::from_rgb(224, 164, 59);      // #e0a43b (黄)
        let meter_base = Color32::from_rgb(163, 174, 191);      // #a3aebf (グレー青)

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 8.0);

            // 1. ヘッダーバー (SpareLink Core + 最前面ピン)
            ui.horizontal(|ui| {
                // 赤のブランドマーク ■
                ui.label(RichText::new("■").color(accent_red).size(13.0));
                ui.label(RichText::new("SpareLink").strong().size(14.0).color(text_main));
                ui.label(RichText::new("Core v0.1.0").monospace().size(10.5).color(text_muted));

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
                    .fill(card_bg)
                    .rounding(Rounding::same(4.0))
                    .stroke(Stroke::new(1.0_f32, card_border))
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("💻 自PC:").size(11.5).color(text_muted));
                            ui.label(RichText::new(&telem.pc_name).strong().color(text_main));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if let Some(score) = telem.cpu_benchmark_score {
                                    ui.label(RichText::new(format!("Base: {} pt", score)).size(10.5).color(text_muted));
                                }
                            });
                        });

                        ui.add_space(2.0);

                        // CPU使用率 (赤ベースプログレスバー)
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("CPU").monospace().size(11.0).color(text_muted));
                            let cpu_frac = (telem.cpu_percent / 100.0).clamp(0.0, 1.0) as f32;
                            let cpu_bar_color = if telem.cpu_percent > 85.0 {
                                status_warn // 高負荷注意
                            } else {
                                accent_red  // SpareLink 赤アクセント
                            };
                            let bar = ProgressBar::new(cpu_frac)
                                .text(format!("{:>4.1}%", telem.cpu_percent))
                                .fill(cpu_bar_color);
                            ui.add(bar);
                        });

                        // メモリ使用率
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("MEM").monospace().size(11.0).color(text_muted));
                            let mem_frac = (telem.memory_percent / 100.0).clamp(0.0, 1.0) as f32;
                            let used_gb = (telem.memory_total_gb - telem.memory_available_gb).max(0.0);
                            let bar = ProgressBar::new(mem_frac)
                                .text(format!("{:.1}/{:.1}GB ({:.0}%)", used_gb, telem.memory_total_gb, telem.memory_percent))
                                .fill(meter_base);
                            ui.add(bar);
                        });

                        ui.add_space(4.0);

                        // 余剰ポイントハイライトボックス
                        egui::Frame::none()
                            .fill(if is_paused {
                                Color32::from_rgba_premultiplied(224, 164, 59, 25)
                            } else {
                                Color32::from_rgba_premultiplied(79, 179, 132, 25)
                            })
                            .rounding(Rounding::same(3.0))
                            .inner_margin(6.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if is_paused {
                                        ui.label(RichText::new("⏸ 共有一時停止中").strong().size(12.0).color(status_warn));
                                    } else {
                                        let surplus_pt = telem.surplus.allocatable_cpu_score.unwrap_or(0);
                                        let surplus_mem = telem.surplus.allocatable_memory_gb.unwrap_or(0.0);
                                        ui.label(RichText::new("余剰CPU:").size(11.0).color(text_muted));
                                        ui.label(RichText::new(format!("{} pt", surplus_pt)).strong().size(14.0).color(status_ok));
                                        ui.label(RichText::new(format!("(RAM: {:.1}GB)", surplus_mem)).size(11.0).color(text_muted));
                                    }
                                });
                            });
                    });
            } else {
                ui.label(RichText::new("システム情報を取得中...").italics().color(text_muted));
            }

            // 3. Tailscale & P2P ネットワークカード
            egui::Frame::none()
                .fill(card_bg)
                .rounding(Rounding::same(4.0))
                .stroke(Stroke::new(1.0_f32, card_border))
                .inner_margin(8.0)
                .show(ui, |ui| {
                    // Tailscaleステータス
                    if let Some(ref telem) = self.cached_telemetry {
                        if let Some(ref ts) = telem.tailscale {
                            ui.horizontal(|ui| {
                                match ts.state {
                                    TailscaleState::Running => {
                                        ui.label(RichText::new("●").color(status_ok).size(10.0));
                                        ui.label(RichText::new("Tailscale:").size(11.0).color(text_muted));
                                        ui.label(RichText::new(format!("接続中 ({})", ts.self_ip.as_deref().unwrap_or("100.x"))).size(11.0).color(text_main));
                                    }
                                    TailscaleState::NeedsLogin => {
                                        ui.label(RichText::new("▲").color(status_warn).size(10.0));
                                        ui.label(RichText::new("Tailscale: ログイン必要").color(status_warn).size(11.0));
                                        if let Some(ref url) = ts.auth_url {
                                            if ui.small_button("認証を開く ↗").clicked() {
                                                open_browser(url);
                                            }
                                        }
                                    }
                                    TailscaleState::NotInstalled => {
                                        ui.label(RichText::new("✕").color(accent_red).size(10.0));
                                        ui.label(RichText::new("Tailscale: 未インストール").color(accent_red).size(11.0));
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
                        ui.label(RichText::new("P2P ピア一覧").strong().size(11.5).color(text_main));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(format!("{}/{} 台オンライン", online_count, total_count)).size(11.0).color(text_muted));
                        });
                    });

                    ui.add_space(2.0);

                    // ピアリストスクロール領域
                    egui::ScrollArea::vertical()
                        .max_height(110.0)
                        .show(ui, |ui| {
                            if let Some(peer_list) = peers {
                                if peer_list.is_empty() {
                                    ui.label(RichText::new("探索中...").italics().size(11.0).color(text_muted));
                                } else {
                                    for p in peer_list {
                                        let is_online = p.status == "online";
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new("●").size(9.0).color(if is_online {
                                                status_ok
                                            } else {
                                                Color32::from_rgb(107, 115, 128)
                                            }));
                                            ui.label(RichText::new(&p.name).strong().size(11.5).color(if is_online {
                                                text_main
                                            } else {
                                                text_muted
                                            }));

                                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                if is_online {
                                                    if let Some(score) = p.surplus.allocatable_cpu_score {
                                                        ui.label(RichText::new(format!("{} pt", score)).strong().size(11.5).color(status_ok));
                                                    }
                                                } else {
                                                    ui.label(RichText::new("offline").size(10.5).color(Color32::from_rgb(107, 115, 128)));
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
                let pause_btn = if is_paused {
                    egui::Button::new(RichText::new(pause_text).color(Color32::WHITE).strong())
                        .fill(accent_red)
                } else {
                    egui::Button::new(RichText::new(pause_text).color(text_main))
                };
                if ui.add(pause_btn).clicked() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_japanese_font_load() {
        let candidates = [
            "C:\\Windows\\Fonts\\meiryo.ttc",
            "C:\\Windows\\Fonts\\msgothic.ttc",
            "C:\\Windows\\Fonts\\YuGothM.ttc",
        ];
        let mut loaded = false;
        for path in &candidates {
            if let Ok(data) = std::fs::read(path) {
                let font_data = egui::FontData::from_owned(data);
                let mut fonts = egui::FontDefinitions::default();
                fonts.font_data.insert("jp_font".to_owned(), font_data);
                fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "jp_font".to_owned());
                fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().push("jp_font".to_owned());
                let ctx = egui::Context::default();
                ctx.set_fonts(fonts);
                loaded = true;
                break;
            }
        }
        assert!(loaded, "Failed to load any Windows Japanese font");
    }
}
