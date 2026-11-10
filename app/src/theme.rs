use eframe::egui::{
    self, ecolor::Hsva, pos2, vec2, Color32, CornerRadius, FontFamily, FontId, Mesh, Painter, Pos2, Rect, Shape, Stroke,
};
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(10, 10, 14);
pub const PANEL: Color32 = Color32::from_rgb(20, 20, 27);
pub const CARD: Color32 = Color32::from_rgb(30, 30, 40);
pub const CARD_HOVER: Color32 = Color32::from_rgb(40, 40, 54);
pub const TEXT: Color32 = Color32::from_rgb(242, 242, 247);
pub const SUBTLE: Color32 = Color32::from_rgb(158, 158, 176);
pub const FAINT: Color32 = Color32::from_rgb(90, 90, 108);
pub const ACCENT: Color32 = Color32::from_rgb(255, 122, 89);
pub const ACCENT_HI: Color32 = Color32::from_rgb(255, 150, 120);
pub const ACCENT2: Color32 = Color32::from_rgb(167, 139, 250);

pub fn cr(n: u8) -> CornerRadius {
    CornerRadius::same(n)
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

pub fn reg(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn hsv(h: f32, s: f32, v: f32) -> Color32 {
    Hsva::new(h.rem_euclid(1.0), s, v, 1.0).into()
}

pub fn setup(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let base: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut prop = base.clone();
    let mut boldf = base.clone();

    // First font file that exists: Windows system font, else the Android system font.
    let read = |paths: &[&str]| paths.iter().find_map(|p| std::fs::read(p).ok());
    if let Some(b) = read(&["C:\\Windows\\Fonts\\segoeui.ttf", "/system/fonts/Roboto-Regular.ttf"]) {
        fonts.font_data.insert("segoe".into(), Arc::new(egui::FontData::from_owned(b)));
        prop.insert(0, "segoe".into());
    }
    if let Some(b) = read(&["C:\\Windows\\Fonts\\segoeuib.ttf", "/system/fonts/Roboto-Bold.ttf"]) {
        fonts.font_data.insert("segoe_bold".into(), Arc::new(egui::FontData::from_owned(b)));
        boldf.insert(0, "segoe_bold".into());
    } else if prop.first().map(|s| s == "segoe").unwrap_or(false) {
        boldf.insert(0, "segoe".into());
    }
    if let Some(b) = read(&["C:\\Windows\\Fonts\\seguisym.ttf"]) {
        fonts.font_data.insert("symbols".into(), Arc::new(egui::FontData::from_owned(b)));
        prop.push("symbols".into());
        boldf.push("symbols".into());
    }
    fonts.families.insert(FontFamily::Proportional, prop);
    fonts.families.insert(FontFamily::Name("bold".into()), boldf);
    ctx.set_fonts(fonts);

    let mut style = (*ctx.style()).clone();
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.extreme_bg_color = CARD;
    v.faint_bg_color = PANEL;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT.linear_multiply(0.6);
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    v.hyperlink_color = ACCENT;
    v.window_corner_radius = cr(10);
    v.menu_corner_radius = cr(8);
    v.window_stroke = Stroke::new(1.0_f32, CARD_HOVER);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, CARD_HOVER);
    v.widgets.inactive.bg_fill = CARD;
    v.widgets.inactive.weak_bg_fill = CARD;
    v.widgets.inactive.corner_radius = cr(8);
    v.widgets.hovered.bg_fill = CARD_HOVER;
    v.widgets.hovered.weak_bg_fill = CARD_HOVER;
    v.widgets.hovered.corner_radius = cr(8);
    v.widgets.active.bg_fill = CARD_HOVER;
    v.widgets.active.weak_bg_fill = CARD_HOVER;
    v.widgets.active.corner_radius = cr(8);
    v.widgets.open.corner_radius = cr(8);
    style.visuals = v;
    style.spacing.item_spacing = vec2(8.0, 6.0);
    style.spacing.button_padding = vec2(12.0, 6.0);
    style.spacing.menu_margin = egui::Margin::same(6);
    style.text_styles.insert(egui::TextStyle::Body, reg(15.0));
    style.text_styles.insert(egui::TextStyle::Button, reg(15.0));
    style.text_styles.insert(egui::TextStyle::Small, reg(12.0));
    style.text_styles.insert(egui::TextStyle::Heading, bold(26.0));
    ctx.set_style(style);
}

/// Rectangle with a colour per corner.
pub fn gradient4(p: &Painter, r: Rect, tl: Color32, tr: Color32, bl: Color32, br: Color32) {
    let mut m = Mesh::default();
    m.colored_vertex(r.left_top(), tl);
    m.colored_vertex(r.right_top(), tr);
    m.colored_vertex(r.left_bottom(), bl);
    m.colored_vertex(r.right_bottom(), br);
    m.add_triangle(0, 1, 2);
    m.add_triangle(1, 3, 2);
    p.add(Shape::mesh(m));
}

pub fn vgradient(p: &Painter, r: Rect, top: Color32, bottom: Color32) {
    gradient4(p, r, top, top, bottom, bottom);
}

/// Placeholder cover art: a gradient with soft shapes, unique per seed.
pub fn cover(p: &Painter, r: Rect, hue: f32, seed: &str) {
    let p = p.with_clip_rect(r.intersect(p.clip_rect()));
    let h = crate::library::hue_of(seed);
    let c1 = hsv(hue, 0.6, 0.85);
    let c2 = hsv(hue + 0.08, 0.75, 0.5);
    let c3 = hsv(hue - 0.06, 0.75, 0.28);
    gradient4(&p, r, c1, c2, c2, c3);
    let s = r.width();
    let light = |a: u8| Color32::from_rgba_unmultiplied(255, 255, 255, a);
    let dark = |a: u8| Color32::from_rgba_unmultiplied(0, 0, 0, a);
    let style = ((h * 1000.0) as u32) % 4;
    match style {
        0 => {
            // sun over waves
            let c = pos2(r.left() + s * (0.3 + 0.4 * h), r.top() + s * 0.42);
            p.circle_filled(c, s * 0.24, light(70));
            for k in 0..3 {
                let n = 24;
                let pts: Vec<Pos2> = (0..=n)
                    .map(|i| {
                        let t = i as f32 / n as f32;
                        let y = r.top() + s * (0.66 + 0.1 * k as f32) + (t * 8.0 + h * 6.0 + k as f32).sin() * s * 0.04;
                        pos2(r.left() + t * s, y)
                    })
                    .collect();
                p.add(Shape::line(pts, Stroke::new((s * 0.025).max(1.0), light(110 - 30 * k as u8))));
            }
        }
        1 => {
            // concentric rings
            let c = pos2(r.left() + s * (0.55 + 0.3 * h), r.top() + s * (0.6 - 0.2 * h));
            for k in 1..6 {
                p.circle_stroke(c, s * 0.1 * k as f32, Stroke::new((s * 0.02).max(1.0), light(120 - 18 * k as u8)));
            }
            p.circle_filled(c, s * 0.05, light(160));
        }
        2 => {
            // diagonal bands
            for k in 0..4 {
                let o = s * (0.25 * k as f32 - 0.1 + 0.2 * h);
                let pts = vec![pos2(r.left() + o, r.bottom()), pos2(r.left() + o + s * 0.12, r.bottom()), pos2(r.left() + o + s * 0.7, r.top()), pos2(r.left() + o + s * 0.58, r.top())];
                p.add(Shape::convex_polygon(pts, if k % 2 == 0 { light(40) } else { dark(50) }, Stroke::NONE));
            }
        }
        _ => {
            // stacked circles
            p.circle_filled(pos2(r.left() + s * 0.35, r.top() + s * 0.4), s * 0.28, light(55));
            p.circle_filled(pos2(r.left() + s * 0.62, r.top() + s * 0.58), s * 0.26, dark(55));
            p.circle_filled(pos2(r.left() + s * (0.5 + 0.2 * h), r.top() + s * 0.3), s * 0.08, light(150));
        }
    }
}

/// Liked-songs style cover.
pub fn liked_cover(p: &Painter, r: Rect) {
    gradient4(p, r, Color32::from_rgb(120, 70, 240), Color32::from_rgb(200, 90, 200), Color32::from_rgb(90, 60, 200), ACCENT);
    heart(p, r.center(), r.width() * 0.32, TEXT, true);
}

pub fn local_cover(p: &Painter, r: Rect) {
    gradient4(p, r, Color32::from_rgb(40, 120, 110), Color32::from_rgb(40, 90, 140), Color32::from_rgb(30, 70, 90), Color32::from_rgb(20, 40, 70));
    folder(p, r.center(), r.width() * 0.4, TEXT);
}

// ---------- icons ----------

pub fn play(p: &Painter, c: Pos2, s: f32, col: Color32) {
    let pts = vec![pos2(c.x - s * 0.35, c.y - s * 0.45), pos2(c.x + s * 0.5, c.y), pos2(c.x - s * 0.35, c.y + s * 0.45)];
    p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
}

pub fn pause(p: &Painter, c: Pos2, s: f32, col: Color32) {
    let w = s * 0.24;
    let h = s * 0.9;
    p.rect_filled(Rect::from_center_size(pos2(c.x - s * 0.22, c.y), vec2(w, h)), cr(1), col);
    p.rect_filled(Rect::from_center_size(pos2(c.x + s * 0.22, c.y), vec2(w, h)), cr(1), col);
}

pub fn skip(p: &Painter, c: Pos2, s: f32, col: Color32, forward: bool) {
    let d = if forward { 1.0 } else { -1.0 };
    let pts = vec![pos2(c.x - d * s * 0.4, c.y - s * 0.4), pos2(c.x + d * s * 0.3, c.y), pos2(c.x - d * s * 0.4, c.y + s * 0.4)];
    p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
    p.rect_filled(Rect::from_center_size(pos2(c.x + d * s * 0.38, c.y), vec2(s * 0.14, s * 0.8)), cr(1), col);
}

pub fn heart(p: &Painter, c: Pos2, s: f32, col: Color32, filled: bool) {
    let n = 40;
    let k = s / 17.0;
    let pts: Vec<Pos2> = (0..n)
        .map(|i| {
            let t = i as f32 / n as f32 * std::f32::consts::TAU;
            let x = 16.0 * t.sin().powi(3);
            let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
            pos2(c.x + x * k, c.y - y * k - k)
        })
        .collect();
    if filled {
        let mut m = Mesh::default();
        m.colored_vertex(pos2(c.x, c.y), col);
        for q in &pts {
            m.colored_vertex(*q, col);
        }
        for i in 0..n as u32 {
            m.add_triangle(0, 1 + i, 1 + (i + 1) % n as u32);
        }
        p.add(Shape::mesh(m));
    } else {
        p.add(Shape::closed_line(pts, Stroke::new(1.6_f32, col)));
    }
}

pub fn shuffle(p: &Painter, c: Pos2, s: f32, col: Color32) {
    let st = Stroke::new(1.7_f32, col);
    let l = c.x - s * 0.45;
    let r = c.x + s * 0.45;
    let t = c.y - s * 0.3;
    let b = c.y + s * 0.3;
    p.line_segment([pos2(l, b), pos2(r - s * 0.1, t)], st);
    p.line_segment([pos2(l, t), pos2(r - s * 0.1, b)], st);
    for y in [t, b] {
        let pts = vec![pos2(r, y), pos2(r - s * 0.22, y - s * 0.15), pos2(r - s * 0.22, y + s * 0.15)];
        p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
    }
}

pub fn repeat(p: &Painter, c: Pos2, s: f32, col: Color32, one: bool) {
    let st = Stroke::new(1.7_f32, col);
    let r = Rect::from_center_size(c, vec2(s * 0.9, s * 0.6));
    p.rect_stroke(r, cr(4), st, egui::StrokeKind::Middle);
    let pts = vec![pos2(r.center().x + s * 0.12, r.top()), pos2(r.center().x - s * 0.06, r.top() - s * 0.14), pos2(r.center().x - s * 0.06, r.top() + s * 0.14)];
    p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
    if one {
        p.text(c, egui::Align2::CENTER_CENTER, "1", bold(s * 0.5), col);
    }
}

pub fn volume(p: &Painter, c: Pos2, s: f32, col: Color32, level: f32) {
    let x = c.x - s * 0.35;
    let pts = vec![
        pos2(x - s * 0.15, c.y - s * 0.15),
        pos2(x + s * 0.05, c.y - s * 0.15),
        pos2(x + s * 0.3, c.y - s * 0.4),
        pos2(x + s * 0.3, c.y + s * 0.4),
        pos2(x + s * 0.05, c.y + s * 0.15),
        pos2(x - s * 0.15, c.y + s * 0.15),
    ];
    p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
    if level <= 0.001 {
        let st = Stroke::new(1.6_f32, col);
        p.line_segment([pos2(c.x + s * 0.15, c.y - s * 0.18), pos2(c.x + s * 0.5, c.y + s * 0.18)], st);
        p.line_segment([pos2(c.x + s * 0.15, c.y + s * 0.18), pos2(c.x + s * 0.5, c.y - s * 0.18)], st);
        return;
    }
    let waves = if level > 0.66 { 3 } else if level > 0.33 { 2 } else { 1 };
    for i in 0..waves {
        let rad = s * (0.2 + 0.16 * i as f32);
        arc(p, pos2(x + s * 0.3, c.y), rad, -0.8, 0.8, Stroke::new(1.5_f32, col));
    }
}

pub fn arc(p: &Painter, c: Pos2, r: f32, a0: f32, a1: f32, st: Stroke) {
    let n = 12;
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            pos2(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect();
    p.add(Shape::line(pts, st));
}

pub fn home(p: &Painter, c: Pos2, s: f32, col: Color32, filled: bool) {
    let pts = vec![
        pos2(c.x, c.y - s * 0.48),
        pos2(c.x + s * 0.45, c.y - s * 0.08),
        pos2(c.x + s * 0.45, c.y + s * 0.45),
        pos2(c.x - s * 0.45, c.y + s * 0.45),
        pos2(c.x - s * 0.45, c.y - s * 0.08),
    ];
    if filled {
        p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
    } else {
        p.add(Shape::closed_line(pts, Stroke::new(1.8_f32, col)));
    }
}

pub fn search(p: &Painter, c: Pos2, s: f32, col: Color32, thick: bool) {
    let w: f32 = if thick { 2.6 } else { 1.8 };
    p.circle_stroke(pos2(c.x - s * 0.08, c.y - s * 0.08), s * 0.32, Stroke::new(w, col));
    p.line_segment([pos2(c.x + s * 0.16, c.y + s * 0.16), pos2(c.x + s * 0.45, c.y + s * 0.45)], Stroke::new(w + 0.4, col));
}

pub fn library(p: &Painter, c: Pos2, s: f32, col: Color32) {
    let st = Stroke::new(2.0_f32, col);
    p.line_segment([pos2(c.x - s * 0.4, c.y - s * 0.42), pos2(c.x - s * 0.4, c.y + s * 0.42)], st);
    p.line_segment([pos2(c.x - s * 0.12, c.y - s * 0.42), pos2(c.x - s * 0.12, c.y + s * 0.42)], st);
    p.line_segment([pos2(c.x + s * 0.12, c.y - s * 0.38), pos2(c.x + s * 0.42, c.y + s * 0.42)], st);
}

pub fn plus(p: &Painter, c: Pos2, s: f32, col: Color32) {
    let st = Stroke::new(2.0_f32, col);
    p.line_segment([pos2(c.x - s * 0.4, c.y), pos2(c.x + s * 0.4, c.y)], st);
    p.line_segment([pos2(c.x, c.y - s * 0.4), pos2(c.x, c.y + s * 0.4)], st);
}

pub fn queue(p: &Painter, c: Pos2, s: f32, col: Color32) {
    let st = Stroke::new(1.7_f32, col);
    for (i, w) in [0.9f32, 0.9, 0.55].iter().enumerate() {
        let y = c.y - s * 0.3 + i as f32 * s * 0.3;
        p.line_segment([pos2(c.x - s * 0.45, y), pos2(c.x - s * 0.45 + s * w, y)], st);
    }
    play(p, pos2(c.x + s * 0.35, c.y + s * 0.3), s * 0.35, col);
}

pub fn folder(p: &Painter, c: Pos2, s: f32, col: Color32) {
    let st = Stroke::new(1.8_f32, col);
    let r = Rect::from_center_size(pos2(c.x, c.y + s * 0.06), vec2(s, s * 0.7));
    p.rect_stroke(r, cr(3), st, egui::StrokeKind::Middle);
    p.line_segment([pos2(r.left() + 2.0, r.top()), pos2(r.left() + s * 0.12, r.top() - s * 0.14)], st);
    p.line_segment([pos2(r.left() + s * 0.12, r.top() - s * 0.14), pos2(r.left() + s * 0.4, r.top() - s * 0.14)], st);
    p.line_segment([pos2(r.left() + s * 0.4, r.top() - s * 0.14), pos2(r.left() + s * 0.48, r.top())], st);
}

pub fn chevron(p: &Painter, c: Pos2, s: f32, col: Color32, left: bool) {
    let d = if left { -1.0 } else { 1.0 };
    let st = Stroke::new(2.0_f32, col);
    p.line_segment([pos2(c.x - d * s * 0.15, c.y - s * 0.3), pos2(c.x + d * s * 0.15, c.y)], st);
    p.line_segment([pos2(c.x + d * s * 0.15, c.y), pos2(c.x - d * s * 0.15, c.y + s * 0.3)], st);
}

pub fn dots(p: &Painter, c: Pos2, s: f32, col: Color32) {
    for i in -1..=1 {
        p.circle_filled(pos2(c.x + i as f32 * s * 0.3, c.y), s * 0.08, col);
    }
}

/// App logo: a round badge with sound-wave bars.
pub fn logo(p: &Painter, c: Pos2, r: f32) {
    p.circle_filled(c, r, ACCENT);
    p.circle_filled(pos2(c.x + r * 0.25, c.y - r * 0.3), r * 0.55, ACCENT_HI.linear_multiply(0.6));
    let hs = [0.35f32, 0.7, 1.0, 0.6, 0.3];
    for (i, h) in hs.iter().enumerate() {
        let x = c.x + (i as f32 - 2.0) * r * 0.28;
        let hh = r * 0.9 * h;
        p.rect_filled(Rect::from_center_size(pos2(x, c.y), vec2(r * 0.14, hh)), cr(2), BG);
    }
}

pub fn fmt_time(secs: f32) -> String {
    if !secs.is_finite() || secs <= 0.0 {
        return "0:00".into();
    }
    let s = secs.round() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn elide(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", t.trim_end())
    }
}
