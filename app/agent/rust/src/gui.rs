use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use eframe::egui::{self, Align, Color32, Layout, RichText, Rounding, Stroke, Vec2};

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

/// 3桁区切りのカンマ挿入 (例: 32298 -> "32,298")
pub fn format_thousands(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let len = s.len();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result
}

/// 長いデバイス名を最大文字数で省略 (例: "DESKTOP-SNC5OE2" -> "DESKTOP-SN…")
pub fn truncate_name(name: &str, max_chars: usize) -> String {
    let char_count = name.chars().count();
    if char_count > max_chars {
        let mut s: String = name.chars().take(max_chars.saturating_sub(1)).collect();
        s.push('…');
        s
    } else {
        name.to_string()
    }
}

/// |||||||| 方式のリソースバー用セグメント数を計算
/// (使用分バー数 [左], 余剰分バー数 [右])
pub fn calculate_resource_bar_counts(
    used_ratio: f32,
    total_bars: usize,
) -> (usize, usize) {
    let u_ratio = used_ratio.clamp(0.0, 1.0);
    let used_bars = if u_ratio > 0.005 {
        ((u_ratio * total_bars as f32).round() as usize).clamp(1, total_bars)
    } else {
        0
    };
    let surplus_bars = total_bars.saturating_sub(used_bars);
    (used_bars, surplus_bars)
}

/// |||||||| 方式のリソースバーを描画
/// [使用分 (左: used_color)] + [余剰分 (右: surplus_color)]
pub fn show_resource_bar(
    ui: &mut egui::Ui,
    used_ratio: f32,
    total_bars: usize,
    used_color: Color32,
    surplus_color: Color32,
) {
    let (used_bars, surplus_bars) = calculate_resource_bar_counts(used_ratio, total_bars);

    let old_spacing = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing.x = 0.0;

    let is_rtl = ui.layout().prefer_right_to_left();

    // right_to_left の場合は先に追加した要素が右に配置されるため、
    // [左: 使用分 (赤)] [右: 余剰分 (緑)] の順序を厳格に保持する
    if is_rtl {
        // 先に右側（余剰分）を追加 -> 右側に配置される
        if surplus_bars > 0 {
            ui.label(
                RichText::new("|".repeat(surplus_bars))
                    .color(surplus_color)
                    .monospace()
                    .strong()
                    .size(13.0),
            );
        }
        // 次に左側（使用分）を追加 -> 左側に配置される
        if used_bars > 0 {
            ui.label(
                RichText::new("|".repeat(used_bars))
                    .color(used_color)
                    .monospace()
                    .strong()
                    .size(13.0),
            );
        }
    } else {
        // left_to_right では通常順序で追加
        if used_bars > 0 {
            ui.label(
                RichText::new("|".repeat(used_bars))
                    .color(used_color)
                    .monospace()
                    .strong()
                    .size(13.0),
            );
        }
        if surplus_bars > 0 {
            ui.label(
                RichText::new("|".repeat(surplus_bars))
                    .color(surplus_color)
                    .monospace()
                    .strong()
                    .size(13.0),
            );
        }
    }

    ui.spacing_mut().item_spacing = old_spacing;
}

/// |||||||| 方式の単一リソースバーを描画 (点灯セグメント + 未点灯セグメント)
pub fn show_segmented_bar(
    ui: &mut egui::Ui,
    ratio: f32,
    total_bars: usize,
    bar_color: Color32,
    empty_color: Color32,
) {
    let filled = ((ratio.clamp(0.0, 1.0) * total_bars as f32).round() as usize).min(total_bars);
    let empty = total_bars.saturating_sub(filled);

    let old_spacing = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing.x = 0.0;

    let is_rtl = ui.layout().prefer_right_to_left();
    if is_rtl {
        if empty > 0 {
            ui.label(
                RichText::new("|".repeat(empty))
                    .color(empty_color)
                    .monospace()
                    .size(13.0),
            );
        }
        if filled > 0 {
            ui.label(
                RichText::new("|".repeat(filled))
                    .color(bar_color)
                    .monospace()
                    .strong()
                    .size(13.0),
            );
        }
    } else {
        if filled > 0 {
            ui.label(
                RichText::new("|".repeat(filled))
                    .color(bar_color)
                    .monospace()
                    .strong()
                    .size(13.0),
            );
        }
        if empty > 0 {
            ui.label(
                RichText::new("|".repeat(empty))
                    .color(empty_color)
                    .monospace()
                    .size(13.0),
            );
        }
    }

    ui.spacing_mut().item_spacing = old_spacing;
}

/// ダークモード / ライトモード対応の配色パレット
#[derive(Clone, Copy)]
pub struct ThemeColors {
    pub bg: Color32,
    pub title_bg: Color32,
    pub card_bg: Color32,
    pub card_border: Color32,
    pub text_main: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub offline: Color32,
    pub ping: Color32,
    #[allow(dead_code)]
    pub meter_base: Color32,
    pub used_bar: Color32,
    pub unlit_bar: Color32,
    pub window_border: Color32,
}

impl ThemeColors {
    pub fn dark() -> Self {
        Self {
            bg: Color32::from_rgb(10, 14, 21),                // #0a0e15 (深黒ネイビー)
            title_bg: Color32::from_rgb(14, 19, 27),          // #0e131b (タイトルバー)
            card_bg: Color32::from_rgb(19, 25, 36),           // #131924 (カード)
            card_border: Color32::from_rgb(42, 52, 68),       // #2a3444 (境界線)
            text_main: Color32::from_rgb(255, 255, 255),      // #ffffff (純白・最高コントラスト)
            text_secondary: Color32::from_rgb(209, 213, 219), // #d1d5db
            text_muted: Color32::from_rgb(148, 163, 184),     // #94a3b8
            accent: Color32::from_rgb(240, 100, 79),          // #f0644f (SpareLink 赤)
            accent_hover: Color32::from_rgb(255, 120, 101),
            ok: Color32::from_rgb(34, 197, 94),               // #22c55e (高視認性グリーン)
            warn: Color32::from_rgb(234, 179, 8),             // #eab308 (アンバー警告)
            offline: Color32::from_rgb(107, 114, 128),        // #6b7280 (オフライン灰)
            ping: Color32::from_rgb(56, 189, 248),            // #38bdf8 (高視認性スカイブルー)
            meter_base: Color32::from_rgb(148, 163, 184),     // #94a3b8
            used_bar: Color32::from_rgb(240, 100, 79),        // #f0644f (使用中: コーラルレッド)
            unlit_bar: Color32::from_rgb(45, 55, 72),         // #2d3748 (未点灯バー)
            window_border: Color32::from_rgb(51, 65, 85),     // #334155 (外枠線)
        }
    }

    pub fn light() -> Self {
        Self {
            bg: Color32::from_rgb(241, 245, 249),             // #f1f5f9 (スレート100)
            title_bg: Color32::from_rgb(226, 232, 240),       // #e2e8f0 (タイトルバー)
            card_bg: Color32::from_rgb(255, 255, 255),        // #ffffff (白カード)
            card_border: Color32::from_rgb(203, 213, 225),    // #cbd5e1 (スレート300)
            text_main: Color32::from_rgb(15, 23, 42),         // #0f172a (最高コントラスト濃黒)
            text_secondary: Color32::from_rgb(51, 65, 85),    // #334155
            text_muted: Color32::from_rgb(100, 116, 139),     // #64748b
            accent: Color32::from_rgb(225, 29, 72),           // #e11d48 (白背景で映えるクリムゾンレッド)
            accent_hover: Color32::from_rgb(190, 18, 60),
            ok: Color32::from_rgb(22, 163, 74),               // #16a34a (高コントラスト濃緑)
            warn: Color32::from_rgb(202, 138, 4),             // #ca8a04
            offline: Color32::from_rgb(148, 163, 184),        // #94a3b8
            ping: Color32::from_rgb(2, 132, 199),             // #0284c7 (高コントラスト青)
            meter_base: Color32::from_rgb(100, 116, 139),     // #64748b
            used_bar: Color32::from_rgb(225, 29, 72),         // #e11d48 (使用中: クリムゾンレッド)
            unlit_bar: Color32::from_rgb(226, 232, 240),      // #e2e8f0 (未点灯バー)
            window_border: Color32::from_rgb(148, 163, 184),  // #94a3b8 (外枠線)
        }
    }
}

/// SpareLink 右下常駐ネイティブモニターGUI
pub struct SpareLinkMonitorApp {
    pub telemetry_holder: Arc<RwLock<Option<LocalAgentTelemetryMessage>>>,
    pub sharing_paused: Arc<AtomicBool>,
    pub always_on_top: bool,
    pub is_dark_mode: bool,
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
            is_dark_mode: true,
            cached_telemetry: None,
        }
    }

    /// テーマおよび日本語フォント・視認性設定
    pub fn configure_theme(ctx: &egui::Context, is_dark: bool) {
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

        // 2. 文字サイズ（視認性向上）
        let mut style = (*ctx.style()).clone();
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(12.5, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(12.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(14.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::new(12.0, egui::FontFamily::Monospace),
        );
        ctx.set_style(style);

        // 3. ダーク/ライトモードビジュアル設定
        let theme = if is_dark { ThemeColors::dark() } else { ThemeColors::light() };
        let mut visuals = if is_dark { egui::Visuals::dark() } else { egui::Visuals::light() };

        visuals.panel_fill = theme.bg;
        visuals.window_fill = theme.bg;

        visuals.widgets.noninteractive.bg_fill = theme.card_bg;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, theme.card_border);

        visuals.widgets.inactive.bg_fill = theme.card_bg;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, theme.card_border);

        visuals.widgets.hovered.bg_fill = if is_dark { Color32::from_rgb(38, 48, 64) } else { Color32::from_rgb(226, 232, 240) };
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, theme.accent);

        visuals.widgets.active.bg_fill = theme.accent;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, theme.accent_hover);

        visuals.selection.bg_fill = theme.accent;
        visuals.selection.stroke = Stroke::new(1.0_f32, theme.accent_hover);

        ctx.set_visuals(visuals);
    }
}

impl eframe::App for SpareLinkMonitorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 定期的にテレメトリホルダーから最新実測値を取得
        if let Ok(lock) = self.telemetry_holder.try_read() {
            if let Some(ref telem) = *lock {
                self.cached_telemetry = Some(telem.clone());
            }
        }

        let is_paused = self.sharing_paused.load(Ordering::SeqCst);
        let theme = if self.is_dark_mode {
            ThemeColors::dark()
        } else {
            ThemeColors::light()
        };

        // 外枠と背景パネル（独自ウィンドウバー対応）
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(theme.bg)
                    .stroke(Stroke::new(1.0_f32, theme.window_border))
                    .rounding(Rounding::same(6.0))
                    .inner_margin(8.0),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 7.0);

                // -------------------------------------------------------------
                // 1. 独自カスタムウィンドウバー (ドラッグ・最小化・ピン・テーマ切替・閉じる)
                // -------------------------------------------------------------
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
                                ui.label(RichText::new("Core").monospace().size(10.5).color(theme.text_muted));
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
                                let pin_color = if self.always_on_top { theme.accent } else { theme.text_muted };
                                let pin_btn = egui::Button::new(RichText::new("📌").size(11.5).color(pin_color))
                                    .frame(false);
                                if ui.add(pin_btn).on_hover_text(if self.always_on_top { "最前面固定: ON" } else { "最前面固定: OFF" }).clicked() {
                                    self.always_on_top = !self.always_on_top;
                                    ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                                        if self.always_on_top {
                                            egui::WindowLevel::AlwaysOnTop
                                        } else {
                                            egui::WindowLevel::Normal
                                        },
                                    ));
                                }

                                // ☀ / 🌙 ダーク・ライトモード切替
                                let mode_icon = if self.is_dark_mode { "☀" } else { "🌙" };
                                let mode_btn = egui::Button::new(RichText::new(mode_icon).size(12.0).color(theme.text_main))
                                    .frame(false);
                                if ui.add(mode_btn).on_hover_text(if self.is_dark_mode { "ライトモードに切替" } else { "ダークモードに切替" }).clicked() {
                                    self.is_dark_mode = !self.is_dark_mode;
                                    Self::configure_theme(ctx, self.is_dark_mode);
                                }
                            });
                        });
                    });

                ui.separator();

                // -------------------------------------------------------------
                // 2. 自PC情報（コンパクトな |||||| 方式）
                // -------------------------------------------------------------
                if let Some(ref telem) = self.cached_telemetry {
                    egui::Frame::none()
                        .fill(theme.card_bg)
                        .rounding(Rounding::same(4.0))
                        .stroke(Stroke::new(1.0_f32, theme.card_border))
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            // 行1: 自PC名 (最大10文字切り詰め) + 余剰スコア & |||||||| バー
                            ui.horizontal(|ui| {
                                let dot_color = if is_paused { theme.warn } else { theme.ok };
                                ui.label(RichText::new("●").size(9.0).color(dot_color));
                                ui.label(RichText::new(truncate_name(&telem.pc_name, 10)).strong().size(12.5).color(theme.text_main));
                                ui.label(RichText::new("(自PC)").size(11.0).color(theme.text_muted));

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if is_paused {
                                        ui.label(RichText::new("⏸ 共有停止中").strong().size(12.0).color(theme.warn));
                                    } else {
                                        let surplus_pt = telem.surplus.allocatable_cpu_score.unwrap_or(0);
                                        let used_ratio = (telem.cpu_percent / 100.0).clamp(0.0, 1.0) as f32;

                                        ui.label(RichText::new(format!("{} pt", format_thousands(surplus_pt))).strong().size(12.0).color(theme.ok));
                                        ui.add_space(4.0);
                                        let surplus_col = if is_paused { theme.unlit_bar } else { theme.ok };
                                        show_resource_bar(ui, used_ratio, 8, theme.used_bar, surplus_col);
                                    }
                                });
                            });

                            ui.add_space(3.0);

                            // 行2: 実測CPUと実測MEM (超コンパクト |||||| 方式)
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("CPU:").size(11.0).color(theme.text_muted));
                                let cpu_ratio = (telem.cpu_percent / 100.0).clamp(0.0, 1.0) as f32;
                                let cpu_bar_color = if telem.cpu_percent > 85.0 { theme.warn } else { theme.used_bar };
                                show_segmented_bar(ui, cpu_ratio, 6, cpu_bar_color, theme.unlit_bar);
                                ui.label(RichText::new(format!("{:>4.1}%", telem.cpu_percent)).monospace().size(11.0).color(theme.text_secondary));

                                ui.add_space(8.0);

                                ui.label(RichText::new("MEM:").size(11.0).color(theme.text_muted));
                                let mem_ratio = (telem.memory_percent / 100.0).clamp(0.0, 1.0) as f32;
                                let used_gb = (telem.memory_total_gb - telem.memory_available_gb).max(0.0);
                                show_segmented_bar(ui, mem_ratio, 6, theme.used_bar, theme.unlit_bar);
                                ui.label(RichText::new(format!("{:.0}% ({:.1}G)", telem.memory_percent, used_gb)).monospace().size(11.0).color(theme.text_secondary));
                            });
                        });
                } else {
                    ui.label(RichText::new("システム情報を取得中...").italics().color(theme.text_muted));
                }

                // -------------------------------------------------------------
                // 3. Tailscale ステータス
                // -------------------------------------------------------------
                if let Some(ref telem) = self.cached_telemetry {
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
                                            ui.label(RichText::new("Tailscale:").size(11.0).color(theme.text_muted));
                                            ui.label(RichText::new(format!("接続中 ({})", ts.self_ip.as_deref().unwrap_or("100.x"))).size(11.5).strong().color(theme.text_main));
                                        }
                                        TailscaleState::NeedsLogin => {
                                            ui.label(RichText::new("▲").color(theme.warn).size(9.0));
                                            ui.label(RichText::new("Tailscale: ログイン必要").color(theme.warn).size(11.0));
                                            if let Some(ref url) = ts.auth_url {
                                                if ui.small_button("認証を開く ↗").clicked() {
                                                    open_browser(url);
                                                }
                                            }
                                        }
                                        TailscaleState::NotInstalled => {
                                            ui.label(RichText::new("✕").color(theme.accent).size(9.0));
                                            ui.label(RichText::new("Tailscale: 未インストール").color(theme.accent).size(11.0));
                                            if ui.small_button("公式DL ↗").clicked() {
                                                open_browser("https://tailscale.com/download");
                                            }
                                        }
                                        _ => {
                                            ui.label(RichText::new("○").color(theme.offline).size(9.0));
                                            ui.label(RichText::new("Tailscale: 停止中").size(11.0).color(theme.offline));
                                        }
                                    }
                                });
                            });
                    }
                }

                // -------------------------------------------------------------
                // 4. P2P ピア一覧 (デバイス名省略 + |||||||| 3層バー + pt + Ping)
                // -------------------------------------------------------------
                let peers = self.cached_telemetry.as_ref().and_then(|t| t.peers.as_ref());
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
                                        ui.label(RichText::new("探索中...").italics().size(11.5).color(theme.text_muted));
                                    } else {
                                        for p in peer_list {
                                            let is_online = p.status == "online";
                                            ui.horizontal(|ui| {
                                                // 1. 状態ドット
                                                let dot_color = if is_online { theme.ok } else { theme.offline };
                                                ui.label(RichText::new("●").size(9.0).color(dot_color));

                                                // 2. 切り詰めデバイス名 (最大10文字)
                                                let name_color = if is_online { theme.text_main } else { theme.text_muted };
                                                ui.label(RichText::new(truncate_name(&p.name, 10)).strong().size(12.0).color(name_color));

                                                // 3. 右側要素: Ping (ms / Offline) + 余剰pt + |||||||| バー
                                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                    // 最右端: ping (ms または Offline)
                                                    if is_online {
                                                        let ping_str = match p.latency_ms {
                                                            Some(0) => "<1 ms".to_string(),
                                                            Some(ms) => format!("{} ms", ms),
                                                            None => "1 ms".to_string(),
                                                        };
                                                        ui.label(RichText::new(ping_str).monospace().strong().size(11.5).color(theme.ping));
                                                    } else {
                                                        ui.label(RichText::new("Offline").size(11.0).color(theme.offline));
                                                    }

                                                    ui.add_space(8.0);

                                                    // 余剰資源 pt
                                                    if is_online {
                                                        let score = p.surplus.allocatable_cpu_score.unwrap_or(0);
                                                        ui.label(RichText::new(format!("{} pt", format_thousands(score))).strong().size(11.5).color(theme.ok));
                                                    } else {
                                                        ui.label(RichText::new("--- pt").monospace().size(11.5).color(theme.offline));
                                                    }

                                                    ui.add_space(4.0);

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

                // -------------------------------------------------------------
                // 5. アクションコントロールバー (下部)
                // -------------------------------------------------------------
                ui.add_space(1.0);
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
                        self.sharing_paused.store(!is_paused, Ordering::SeqCst);
                    }

                    // ブラウザ詳細画面を開くボタン
                    if ui.button("🌐 Web詳細 ↗").clicked() {
                        open_browser("http://localhost:5180");
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("終了").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
            });

        // 500ms（毎秒2回）の再描画
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_thousands() {
        assert_eq!(format_thousands(0), "0");
        assert_eq!(format_thousands(999), "999");
        assert_eq!(format_thousands(1000), "1,000");
        assert_eq!(format_thousands(32298), "32,298");
        assert_eq!(format_thousands(1000000), "1,000,000");
    }

    #[test]
    fn test_truncate_name() {
        assert_eq!(truncate_name("WINSERVER", 10), "WINSERVER");
        assert_eq!(truncate_name("DESKTOP-SNC5OE2", 10), "DESKTOP-S…");
    }

    #[test]
    fn test_calculate_resource_bar_counts() {
        // 0% used on 8 bars -> 0 used, 8 surplus
        assert_eq!(calculate_resource_bar_counts(0.0, 8), (0, 8));

        // 12% used on 8 bars -> 1 used, 7 surplus
        assert_eq!(calculate_resource_bar_counts(0.12, 8), (1, 7));

        // 25% used on 8 bars -> 2 used, 6 surplus
        assert_eq!(calculate_resource_bar_counts(0.25, 8), (2, 6));

        // 50% used on 8 bars -> 4 used, 4 surplus
        assert_eq!(calculate_resource_bar_counts(0.50, 8), (4, 4));

        // 85% used on 8 bars -> 7 used, 1 surplus
        assert_eq!(calculate_resource_bar_counts(0.85, 8), (7, 1));

        // 100% used on 8 bars -> 8 used, 0 surplus
        assert_eq!(calculate_resource_bar_counts(1.0, 8), (8, 0));
    }

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
