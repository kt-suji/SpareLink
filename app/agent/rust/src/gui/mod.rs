pub mod theme;
pub mod views;
pub mod widgets;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use eframe::egui::{self, Rounding, Stroke, Vec2};

pub use theme::ThemeColors;
pub use widgets::*;

use crate::models::LocalAgentTelemetryMessage;

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

    /// テーマおよびフォント初期設定の委譲ヘルパー
    pub fn configure_theme(ctx: &egui::Context, is_dark: bool) {
        theme::configure_theme(ctx, is_dark);
    }
}

impl eframe::App for SpareLinkMonitorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 定期的にテレメトリホルダーから最新実測値を取得
        if let Ok(lock) = self.telemetry_holder.try_read()
            && let Some(ref telem) = *lock
        {
            self.cached_telemetry = Some(telem.clone());
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

                // 1. タイトルバー
                views::render_titlebar(self, ui, ctx, theme);

                ui.separator();

                // 2. 自PC情報カード
                views::render_local_card(ui, self.cached_telemetry.as_ref(), is_paused, theme);

                // 3. Tailscale ステータス
                views::render_tailscale_card(ui, self.cached_telemetry.as_ref(), theme);

                // 4. P2P メッシュピア
                views::render_peers_card(ui, self.cached_telemetry.as_ref(), theme);

                // 5. アクションコントロールバー (下部)
                views::render_action_bar(self, ui, ctx, is_paused, theme);
            });

        // 500ms（毎秒2回）の再描画
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Align, Color32, Layout};

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
        assert_eq!(calculate_resource_bar_counts(0.0, 8), (0, 8));
        assert_eq!(calculate_resource_bar_counts(0.12, 8), (1, 7));
        assert_eq!(calculate_resource_bar_counts(0.25, 8), (2, 6));
        assert_eq!(calculate_resource_bar_counts(0.50, 8), (4, 4));
        assert_eq!(calculate_resource_bar_counts(0.85, 8), (7, 1));
        assert_eq!(calculate_resource_bar_counts(1.0, 8), (8, 0));
    }

    #[test]
    fn test_bar_alignment_across_digits() {
        let ctx = egui::Context::default();
        let mut bar_x1 = 0.0;
        let mut bar_x2 = 0.0;
        let mut score_rect1 = egui::Rect::NOTHING;
        let mut score_rect2 = egui::Rect::NOTHING;

        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                // Row 1: GORONEPC (0 pt, 2 ms)
                ui.horizontal(|ui| {
                    ui.label("GORONEPC");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let _ = ui.add_sized([46.0, 16.0], egui::Label::new("2 ms"));
                        ui.add_space(6.0);
                        let resp = ui.add_sized([70.0, 16.0], egui::Label::new("0 pt"));
                        score_rect1 = resp.rect;
                        ui.add_space(8.0);
                        let bar_resp = ui.scope(|ui| {
                            show_resource_bar(ui, 0.1, 8, Color32::RED, Color32::GREEN);
                        });
                        bar_x1 = bar_resp.response.rect.min.x;
                    });
                });

                // Row 2: WINSERVER (25,070 pt, 1 ms)
                ui.horizontal(|ui| {
                    ui.label("WINSERVER");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let _ = ui.add_sized([46.0, 16.0], egui::Label::new("1 ms"));
                        ui.add_space(6.0);
                        let resp = ui.add_sized([70.0, 16.0], egui::Label::new("25,070 pt"));
                        score_rect2 = resp.rect;
                        ui.add_space(8.0);
                        let bar_resp = ui.scope(|ui| {
                            show_resource_bar(ui, 0.1, 8, Color32::RED, Color32::GREEN);
                        });
                        bar_x2 = bar_resp.response.rect.min.x;
                    });
                });
            });
        });

        assert_eq!(score_rect1.width(), 70.0);
        assert_eq!(score_rect2.width(), 70.0);
        assert_eq!(score_rect1.min.x, score_rect2.min.x);
        assert_eq!(score_rect1.max.x, score_rect2.max.x);
        assert!(
            (bar_x1 - bar_x2).abs() < 0.1,
            "Bars must align perfectly! x1={}, x2={}",
            bar_x1,
            bar_x2
        );
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
