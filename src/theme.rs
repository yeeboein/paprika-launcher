//! Paprika's look: warm dark theme, red/orange accents and the pepper logo.

use eframe::egui::{self, vec2, Color32, CornerRadius, Pos2, Rect, Stroke, Ui, Visuals};

pub const ACCENT: Color32 = Color32::from_rgb(232, 72, 52);
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
        s.animation_time = 0.15;
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

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgba_unmultiplied(
        f(a.r(), b.r()),
        f(a.g(), b.g()),
        f(a.b(), b.b()),
        f(a.a(), b.a()),
    )
}

/// A button that glows and grows on hover and squishes when pressed.
pub fn anim_button(
    ui: &mut Ui,
    text: &str,
    fill: Color32,
    min: egui::Vec2,
    font_size: f32,
    enabled: bool,
) -> egui::Response {
    let text_color = if enabled {
        Color32::WHITE
    } else {
        Color32::from_gray(150)
    };
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(font_size),
        text_color,
    );
    let size = vec2(
        min.x.max(galley.size().x + 30.0),
        min.y.max(galley.size().y + 14.0),
    );
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let ctx = ui.ctx().clone();
    let hover = ctx.animate_bool_with_time(resp.id.with("hover"), enabled && resp.hovered(), 0.18);
    let press = ctx.animate_bool_with_time(
        resp.id.with("press"),
        enabled && resp.is_pointer_button_down_on(),
        0.07,
    );
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        // One flat colour; hover grows it a touch, pressing shrinks it.
        let r = rect.expand(2.0 * hover - 2.5 * press);
        let base = if enabled {
            fill
        } else {
            fill.gamma_multiply(0.35)
        };
        p.rect_filled(r, 8.0, base);
        let pos = r.center() - galley.size() / 2.0;
        p.galley(pos, galley, text_color);
    }
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Sidebar item with a sliding highlight and accent bar.
pub fn nav_item(ui: &mut Ui, icon: &str, label: &str, selected: bool) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), 40.0), egui::Sense::click());
    let ctx = ui.ctx().clone();
    let hover = ctx.animate_bool_with_time(resp.id.with("hover"), resp.hovered(), 0.15);
    let sel = ctx.animate_bool_with_time(resp.id.with("sel"), selected, 0.22);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        if hover > 0.01 && !selected {
            p.rect_filled(
                rect.shrink(2.0 * (1.0 - hover)),
                8.0,
                CARD_HOVER.gamma_multiply(hover),
            );
        }
        if sel > 0.01 {
            p.rect_filled(rect, 8.0, ACCENT.gamma_multiply(sel));
        }
        let color = mix(TEXT_DIM, Color32::WHITE, sel.max(hover * 0.6));
        let x = rect.min.x + 16.0 + 2.0 * hover;
        p.text(
            egui::pos2(x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            icon,
            egui::FontId::proportional(15.0),
            color,
        );
        p.text(
            egui::pos2(x + 26.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(15.0),
            color,
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
