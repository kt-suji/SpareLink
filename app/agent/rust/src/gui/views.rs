use std::sync::atomic::Ordering;
use eframe::egui::{self, Align, Color32, Layout, RichText, Rounding, Stroke, Vec2};

use crate::discovery::tailscale::open_browser;
use crate::models::{LocalAgentTelemetryMessage, TailscaleState};
use super::theme::ThemeColors;
use super::widgets::{format_thousands, show_resource_bar, truncate_name};
use super::SpareLinkMonitorApp;

/// 1. 独自カスタムウィンドウバー (ドラッグ・最小化・ピン・テーマ切替・閉じる)
pub fn render_titlebar(app: &mut SpareLinkMonitorApp, ui: &mut egui::Ui, ctx: &egui::Context, theme: ThemeColors) {
    egui::Frame::none()
        .fill(theme.title_bg)
        .rounding(Rounding::same(4.0))
        .inner_margin(egui::Margin::symmetric(6.0, 4.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);

                // 左側: ブランドロゴ + タイトル (クリック・ドラッグでウィンドウ移動)
                let left_resp = ui.horizontal(|ui| {
                    ui.label(RichText::new("■").color(theme.accent).size(13.5));
                    ui.label(RichText::new("SpareLink").strong().size(13.0).color(theme.text_main));
                    ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).monospace().strong().size(10.5).color(theme.accent));
                }).response;

                if left_resp.dragged() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }

                // 中央の余白（ドラッグ領域として機能）
                let remaining_w = (ui.available_width() - 116.0).max(10.0);
                let drag_spacer = ui.allocate_response(Vec2::new(remaining_w, 22.0), egui::Sense::drag());
                if drag_spacer.dragged() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }

                // 右側: ウィンドウ操作ボタングループ
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // ✕ 閉じるボタン
                    let close_btn = egui::Button::new(RichText::new("✕").size(12.0).color(theme.text_main))
                        .frame(false);
                    if ui.add(close_btn).on_hover_text("終了").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }

                    // ― 最小化ボタン
                    let min_btn = egui::Button::new(RichText::new("―").size(11.0).color(theme.text_main))
                        .frame(false);
                    if ui.add(min_btn).on_hover_text("最小化").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }

                    // 📌 最前面ピン留め
                    let pin_color = if app.always_on_top { theme.accent } else { theme.text_muted };
                    let pin_btn = egui::Button::new(RichText::new("📌").size(11.5).color(pin_color))
                        .frame(false);
                    if ui.add(pin_btn).on_hover_text(if app.always_on_top { "最前面固定: ON" } else { "最前面固定: OFF" }).clicked() {
                        app.always_on_top = !app.always_on_top;
                        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                            if app.always_on_top {
                                egui::WindowLevel::AlwaysOnTop
                            } else {
                                egui::WindowLevel::Normal
                            },
                        ));
                    }

                    // 🌐 Webダッシュボード・設定を開くクイックボタン
                    let web_title_btn = egui::Button::new(RichText::new("🌐").size(12.0).color(theme.text_main)).frame(false);
                    if ui.add(web_title_btn).on_hover_text("ブラウザでWebダッシュボード・設定を開く (http://localhost:5180)").clicked() {
                        open_browser("http://localhost:5180");
                    }

                    // ☀ / 🌙 ダーク・ライトモード切替
                    let mode_icon = if app.is_dark_mode { "☀" } else { "🌙" };
                    let mode_btn = egui::Button::new(RichText::new(mode_icon).size(12.0).color(theme.text_main))
                        .frame(false);
                    if ui.add(mode_btn).on_hover_text(if app.is_dark_mode { "ライトモードに切替" } else { "ダークモードに切替" }).clicked() {
                        app.is_dark_mode = !app.is_dark_mode;
                        crate::gui::theme::configure_theme(ctx, app.is_dark_mode);
                    }
                });
            });
        });
}

/// 2. 自PC情報（コンパクトな |||||||| 方式・朱と緑で統一）
pub fn render_local_card(
    ui: &mut egui::Ui,
    telem: Option<&LocalAgentTelemetryMessage>,
    is_paused: bool,
    theme: ThemeColors,
) {
    if let Some(telem) = telem {
        egui::Frame::none()
            .fill(theme.card_bg)
            .rounding(Rounding::same(4.0))
            .stroke(Stroke::new(1.0_f32, theme.card_border))
            .inner_margin(8.0)
            .show(ui, |ui| {
                let surplus_col = if is_paused { theme.unlit_bar } else { theme.ok };

                // 行1: 自PC名 (最大10文字切り詰め) + 余剰スコア & |||||||| バー
                ui.horizontal(|ui| {
                    let dot_color = if is_paused { theme.warn } else { theme.ok };
                    ui.label(RichText::new("●").size(9.0).color(dot_color));
                    ui.add_space(4.0);
                    ui.label(RichText::new(truncate_name(&telem.pc_name, 10)).strong().size(12.5).color(theme.text_main));
                    ui.add_space(3.0);
                    ui.label(RichText::new("(自PC)").size(11.0).color(theme.text_muted));

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if is_paused {
                            ui.label(RichText::new("⏸ 共有停止中").strong().size(12.0).color(theme.warn));
                        } else {
                            let surplus_pt = telem.surplus.allocatable_cpu_score.unwrap_or(0);
                            let used_ratio = (telem.cpu_percent / 100.0).clamp(0.0, 1.0) as f32;

                            ui.label(RichText::new(format!("{} pt", format_thousands(surplus_pt))).strong().size(12.0).color(theme.ok));
                            ui.add_space(6.0);
                            show_resource_bar(ui, used_ratio, 8, theme.used_bar, surplus_col);
                        }
                    });
                });

                ui.add_space(4.0);

                // 行2: 実測CPUと実測MEM (グレー廃止・朱と緑の8セグメントバー + 数値間隔確保)
                ui.horizontal(|ui| {
                    ui.label(RichText::new("CPU:").size(11.0).color(theme.text_muted));
                    ui.add_space(4.0);
                    let cpu_ratio = (telem.cpu_percent / 100.0).clamp(0.0, 1.0) as f32;
                    show_resource_bar(ui, cpu_ratio, 8, theme.used_bar, surplus_col);
                    ui.add_space(6.0);
                    ui.label(RichText::new(format!("{:.1}%", telem.cpu_percent)).monospace().size(11.0).color(theme.text_secondary));

                    ui.add_space(10.0);

                    ui.label(RichText::new("MEM:").size(11.0).color(theme.text_muted));
                    ui.add_space(4.0);
                    let mem_ratio = (telem.memory_percent / 100.0).clamp(0.0, 1.0) as f32;
                    let used_gb = (telem.memory_total_gb - telem.memory_available_gb).max(0.0);
                    show_resource_bar(ui, mem_ratio, 8, theme.used_bar, surplus_col);
                    ui.add_space(6.0);
                    ui.label(RichText::new(format!("{:.0}% ({:.1}G)", telem.memory_percent, used_gb)).monospace().size(11.0).color(theme.text_secondary));
                });
            });
    } else {
        ui.label(RichText::new("システム情報を取得中...").italics().color(theme.text_muted));
    }
}

/// 3. Tailscale ステータス
pub fn render_tailscale_card(
    ui: &mut egui::Ui,
    telem: Option<&LocalAgentTelemetryMessage>,
    theme: ThemeColors,
) {
    if let Some(telem) = telem {
        if let Some(ref ts) = telem.tailscale {
            egui::Frame::none()
                .fill(theme.card_bg)
                .rounding(Rounding::same(4.0))
                .stroke(Stroke::new(1.0_f32, theme.card_border))
                .inner_margin(6.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        match ts.state {
                            TailscaleState::Running => {
                                ui.label(RichText::new("●").color(theme.ok).size(9.0));
                                ui.add_space(4.0);
                                ui.label(RichText::new("Tailscale:").size(11.0).color(theme.text_muted));
                                ui.add_space(4.0);
                                ui.label(RichText::new(format!("接続中 ({})", ts.self_ip.as_deref().unwrap_or("100.x"))).size(11.5).strong().color(theme.text_main));
                            }
                            TailscaleState::NeedsLogin => {
                                ui.label(RichText::new("▲").color(theme.warn).size(9.0));
                                ui.add_space(4.0);
                                ui.label(RichText::new("Tailscale: ログイン必要").color(theme.warn).size(11.0));
                                ui.add_space(6.0);
                                if let Some(ref url) = ts.auth_url {
                                    if ui.small_button("認証を開く ↗").clicked() {
                                        open_browser(url);
                                    }
                                }
                            }
                            TailscaleState::NotInstalled => {
                                ui.label(RichText::new("✕").color(theme.accent).size(9.0));
                                ui.add_space(4.0);
                                ui.label(RichText::new("Tailscale: 未インストール").color(theme.accent).size(11.0));
                                ui.add_space(6.0);
                                if ui.small_button("公式DL ↗").clicked() {
                                    open_browser("https://tailscale.com/download");
                                }
                            }
                            _ => {
                                ui.label(RichText::new("○").color(theme.offline).size(9.0));
                                ui.add_space(4.0);
                                ui.label(RichText::new("Tailscale: 停止中").size(11.0).color(theme.offline));
                            }
                        }
                    });
                });
        }
    }
}

/// 4. P2P メッシュピア (常時一覧表示: デバイス名 + |||||||| バー + pt + Ping)
pub fn render_peers_card(
    ui: &mut egui::Ui,
    telem: Option<&LocalAgentTelemetryMessage>,
    theme: ThemeColors,
) {
    let peers = telem.and_then(|t| t.peers.as_ref());
    let online_count = peers.map(|p| p.iter().filter(|n| n.status == "online").count()).unwrap_or(0);
    let total_count = peers.map(|p| p.len()).unwrap_or(0);

    egui::Frame::none()
        .fill(theme.card_bg)
        .rounding(Rounding::same(4.0))
        .stroke(Stroke::new(1.0_f32, theme.card_border))
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("P2P メッシュピア").strong().size(12.5).color(theme.text_main));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(format!("{}/{} 台オンライン", online_count, total_count)).size(11.0).color(theme.text_muted));
                });
            });

            ui.separator();

            egui::ScrollArea::vertical()
                .max_height(130.0)
                .show(ui, |ui| {
                    if let Some(peer_list) = peers {
                        if peer_list.is_empty() {
                            ui.label(RichText::new("SpareLink 稼働中の他PCはいません").italics().size(11.5).color(theme.text_muted));
                        } else {
                            for p in peer_list {
                                let is_online = p.status == "online";
                                ui.horizontal(|ui| {
                                    // 1. 状態ドット
                                    let dot_color = if is_online { theme.ok } else { theme.offline };
                                    ui.label(RichText::new("●").size(9.0).color(dot_color));
                                    ui.add_space(4.0);

                                    // 2. 切り詰めデバイス名 (最大10文字)
                                    let name_color = if is_online { theme.text_main } else { theme.text_muted };
                                    ui.label(RichText::new(truncate_name(&p.name, 10)).strong().size(12.0).color(name_color));

                                    // 3. 右側要素: Ping (ms / Offline) + 余剰pt + |||||||| バー
                                    // 固定幅 (Ping: 46px, 余剰スコア: 70px) を確保することで、
                                    // 桁数の変化やオフライン表示によるバーの位置ズレ（ガタつき）を完全に防止
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        // 最右端: ping (ms または Offline) - 固定幅 46px
                                        if is_online {
                                            let ping_str = match p.latency_ms {
                                                Some(0) => "<1 ms".to_string(),
                                                Some(ms) => format!("{} ms", ms),
                                                None => "-- ms".to_string(),
                                            };
                                            ui.add_sized(
                                                [46.0, 16.0],
                                                egui::Label::new(
                                                    RichText::new(ping_str)
                                                        .monospace()
                                                        .strong()
                                                        .size(11.5)
                                                        .color(theme.ping),
                                                ),
                                            );
                                        } else {
                                            ui.add_sized(
                                                [46.0, 16.0],
                                                egui::Label::new(
                                                    RichText::new("Offline")
                                                        .size(11.0)
                                                        .color(theme.offline),
                                                ),
                                            );
                                        }

                                        ui.add_space(6.0);

                                        // 余剰資源 pt - 固定幅 70px で右揃え配置
                                        if is_online {
                                            let score = p.surplus.allocatable_cpu_score.unwrap_or(0);
                                            ui.add_sized(
                                                [70.0, 16.0],
                                                egui::Label::new(
                                                    RichText::new(format!("{} pt", format_thousands(score)))
                                                        .strong()
                                                        .size(11.5)
                                                        .color(theme.ok),
                                                ),
                                            );
                                        } else {
                                            ui.add_sized(
                                                [70.0, 16.0],
                                                egui::Label::new(
                                                    RichText::new("--- pt")
                                                        .monospace()
                                                        .size(11.5)
                                                        .color(theme.offline),
                                                ),
                                            );
                                        }

                                        ui.add_space(8.0);

                                        // |||||||| バー: 左に使用分(赤)、右に余剰分(緑)
                                        if is_online {
                                            let u_ratio = (p.cpu_percent.unwrap_or(0.0) / 100.0).clamp(0.0, 1.0) as f32;
                                            show_resource_bar(ui, u_ratio, 8, theme.used_bar, theme.ok);
                                        } else {
                                            // オフライン時は全8セグメント未点灯
                                            show_resource_bar(ui, 0.0, 8, theme.unlit_bar, theme.unlit_bar);
                                        }
                                    });
                                });
                            }
                        }
                    }
                });
        });
}

/// 5. アクションコントロールバー (下部)
pub fn render_action_bar(
    app: &SpareLinkMonitorApp,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    is_paused: bool,
    theme: ThemeColors,
) {
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        // 一時停止 / 再開ボタン
        let pause_text = if is_paused { "▶ 共有を再開" } else { "⏸ 一時停止" };
        let pause_btn = if is_paused {
            egui::Button::new(RichText::new(pause_text).color(Color32::WHITE).strong())
                .fill(theme.accent)
        } else {
            egui::Button::new(RichText::new(pause_text).color(theme.text_main))
        };
        if ui.add(pause_btn).clicked() {
            app.sharing_paused.store(!is_paused, Ordering::SeqCst);
        }

        ui.add_space(6.0);

        // ブラウザ詳細画面・設定を開くボタン
        let web_btn = egui::Button::new(RichText::new("🌐 Web設定・詳細 ↗").color(theme.text_main).strong());
        if ui.add(web_btn).on_hover_text("ブラウザで詳細設定画面（http://localhost:5180）を開きます").clicked() {
            open_browser("http://localhost:5180");
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("終了").clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    });
}
