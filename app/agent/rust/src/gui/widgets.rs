use eframe::egui::{self, Color32, RichText};

#[cfg(target_os = "windows")]
unsafe extern "system" {
    fn GetSystemMetrics(nIndex: i32) -> i32;
}

#[cfg(target_os = "windows")]
const SM_CXSCREEN: i32 = 0;
#[cfg(target_os = "windows")]
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
#[allow(dead_code)]
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
