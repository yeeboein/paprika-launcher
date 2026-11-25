//! Paprika's look: warm dark theme, red/orange accents and the pepper logo.

use eframe::egui::{self, pos2, vec2, Color32, CornerRadius, Painter, Pos2, Stroke, Visuals};

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

const STEM: Color32 = Color32::from_rgb(76, 156, 72);
const CAP: Color32 = Color32::from_rgb(98, 182, 86);

/// The pepper as circles in draw order (unit size, roughly centered on 0,0),
/// plus the stem as a line segment. Shared by the UI logo and window icon.
type Circle = (f32, f32, f32, Color32);

fn pepper_shapes() -> (Vec<Circle>, [(f32, f32); 2], f32) {
    let (p0, p1, p2) = (
        (-0.17f32, -0.13f32),
        (0.45f32, -0.07f32),
        (0.15f32, 0.47f32),
    );
    let n = 30;
    let mut circles: Vec<Circle> = (0..n)
        .map(|i| {
            let t = i as f32 / (n - 1) as f32;
            let u = 1.0 - t;
            let x = u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0;
            let y = u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1;
            let r = 0.21 * (1.0 - t).powf(0.75) + 0.025;
            let col = lerp(
                Color32::from_rgb(238, 66, 46),
                Color32::from_rgb(176, 26, 30),
                t,
            );
            (x, y, r, col)
        })
        .collect();
    // Green cap where the stem meets the pepper.
    for (dx, dy) in [(-0.06f32, 0.04f32), (0.0, 0.0), (0.05, -0.05)] {
        circles.push((-0.31 + dx, -0.27 + dy, 0.085, CAP));
    }
    (circles, [(-0.33, -0.29), (-0.42, -0.47)], 0.075)
}

pub fn draw_pepper(p: &Painter, c: Pos2, size: f32) {
    let (circles, stem, width) = pepper_shapes();
    let at = |(x, y): (f32, f32)| pos2(c.x + x * size, c.y + y * size);
    p.line_segment([at(stem[0]), at(stem[1])], Stroke::new(size * width, STEM));
    for (x, y, r, col) in circles {
        p.circle_filled(at((x, y)), r * size, col);
    }
}

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

/// Rasterise the pepper for the window/taskbar icon.
pub fn window_icon() -> egui::IconData {
    icon_rgba(64)
}

pub fn icon_rgba(n: usize) -> egui::IconData {
    let mut rgba = vec![0u8; n * n * 4];
    let (circles, stem, width) = pepper_shapes();
    let s = n as f32 * 0.98;
    let (cx, cy) = (n as f32 * 0.5, n as f32 * 0.52);
    for py in 0..n {
        for px in 0..n {
            let (fx, fy) = ((px as f32 + 0.5 - cx) / s, (py as f32 + 0.5 - cy) / s);
            let mut col = None;
            if seg_dist(fx, fy, stem[0].0, stem[0].1, stem[1].0, stem[1].1) < width / 2.0 {
                col = Some(STEM);
            }
            for &(x, y, r, c) in &circles {
                if (fx - x).powi(2) + (fy - y).powi(2) < r * r {
                    col = Some(c);
                }
            }
            if let Some(c) = col {
                let i = (py * n + px) * 4;
                rgba[i..i + 4].copy_from_slice(&[c.r(), c.g(), c.b(), 255]);
            }
        }
    }
    egui::IconData {
        rgba,
        width: n as u32,
        height: n as u32,
    }
}

fn seg_dist(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (vx, vy) = (bx - ax, by - ay);
    let t = (((px - ax) * vx + (py - ay) * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
    ((px - ax - t * vx).powi(2) + (py - ay - t * vy).powi(2)).sqrt()
}

pub fn fmt_count(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{:.1}K", n as f64 / 1e3),
        _ => format!("{:.1}M", n as f64 / 1e6),
    }
}

#[cfg(test)]
mod tests {
    /// `cargo test write_icon -- --ignored` regenerates assets/paprika.png.
    #[test]
    #[ignore]
    fn write_icon() {
        let i = super::icon_rgba(256);
        image::save_buffer(
            "assets/paprika.png",
            &i.rgba,
            i.width,
            i.height,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    }
}
