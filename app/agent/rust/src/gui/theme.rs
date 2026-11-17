use eframe::egui::{self, Color32, Stroke};

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
            used_bar: Color32::from_rgb(225, 29, 72),         // #e11d48 (使用中: クリムゾンレッド)
            unlit_bar: Color32::from_rgb(226, 232, 240),      // #e2e8f0 (未点灯バー)
            window_border: Color32::from_rgb(148, 163, 184),  // #94a3b8 (外枠線)
        }
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
