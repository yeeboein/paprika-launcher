//! Paprika's look: warm dark theme, red/orange accents and the pepper logo.

use eframe::egui::{self, vec2, Color32, CornerRadius, Pos2, Rect, Stroke, Ui, Visuals};

pub const ACCENT: Color32 = Color32::from_rgb(232, 72, 52);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(246, 98, 70);
pub const ORANGE: Color32 = Color32::from_rgb(247, 140, 54);
pub const GREEN: Color32 = Color32::from_rgb(92, 184, 92);
pub const BG: Color32 = Color32::from_rgb(20, 17, 19);
pub const PANEL: Color32 = Color32::from_rgb(28, 24, 27);
pub const CARD: Color32 = Color32::from_rgb(37, 32, 36);
pub const CARD_HOVER: Color32 = Color32::from_rgb(47, 40, 45);
pub const TEXT_DIM: Color32 = Color32::from_rgb(165, 152, 158);

pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(14, 12, 13);
    v.faint_bg_color = CARD;
    v.selection.bg_fill = ACCENT.linear_multiply(0.75);
    v.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.hyperlink_color = ORANGE;
    v.window_corner_radius = CornerRadius::same(12);
    v.window_stroke = Stroke::new(1.0_f32, Color32::from_rgb(60, 50, 56));
    let r = CornerRadius::same(7);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = r;
    }
    v.widgets.inactive.bg_fill = CARD;
    v.widgets.inactive.weak_bg_fill = CARD;
    v.widgets.hovered.bg_fill = CARD_HOVER;
    v.widgets.hovered.weak_bg_fill = CARD_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT.linear_multiply(0.6));
    v.widgets.active.bg_fill = ACCENT;
    v.widgets.active.weak_bg_fill = ACCENT;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(52, 44, 49));
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 8.0);
        s.spacing.button_padding = vec2(12.0, 6.0);
        s.spacing.interact_size.y = 28.0;
    });
}

const LOGO_PNG: &[u8] = include_bytes!("../assets/paprika.png");

/// Paint the Paprika Launcher logo centered on `c`, `size` points across.
pub fn paint_logo(ui: &Ui, c: Pos2, size: f32) {
    egui::Image::new(egui::ImageSource::Bytes {
        uri: "bytes://paprika-logo.png".into(),
        bytes: LOGO_PNG.into(),
    })
    .paint_at(ui, Rect::from_center_size(c, vec2(size, size)));
}

/// The logo as the window/taskbar icon.
pub fn window_icon() -> egui::IconData {
    let img = image::load_from_memory(LOGO_PNG)
        .expect("logo is a valid PNG")
        .to_rgba8();
    let (width, height) = img.dimensions();
    egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    }
}

pub fn fmt_count(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{:.1}K", n as f64 / 1e3),
        _ => format!("{:.1}M", n as f64 / 1e6),
    }
}
