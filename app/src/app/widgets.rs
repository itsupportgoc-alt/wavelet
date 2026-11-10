use super::Art;
use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Frame, Id, Key, Margin, Painter, Pos2, Rect, Sense, Shape, Stroke, TextEdit, Ui};

pub fn draw_art(p: &Painter, r: Rect, art: &Art) {
    match art {
        Art::Cover(h, seed) => cover(p, r, *h, seed),
        Art::Liked => liked_cover(p, r),
        Art::Local => local_cover(p, r),
        Art::Tex(id) => {
            p.image(*id, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
    }
}

pub fn fit_text(p: &Painter, pos: Pos2, anchor: Align2, s: &str, font: FontId, col: Color32, max_w: f32) -> Rect {
    let mut g = p.layout_no_wrap(s.to_string(), font.clone(), col);
    let mut n = s.chars().count();
    while g.size().x > max_w && n > 1 {
        let est = ((n as f32) * max_w / g.size().x).floor() as usize;
        n = est.min(n - 1).max(1);
        g = p.layout_no_wrap(elide(s, n), font.clone(), col);
    }
    let rect = anchor.anchor_size(pos, g.size());
    p.galley(rect.min, g, col);
    rect
}

pub fn rounded_top_gradient(p: &Painter, r: Rect, rad: f32, top: Color32, bottom: Color32) {
    let mut m = egui::Mesh::default();
    let mut pts: Vec<Pos2> = vec![];
    let n = 8;
    for i in 0..=n {
        let a = std::f32::consts::PI + std::f32::consts::FRAC_PI_2 * i as f32 / n as f32;
        pts.push(pos2(r.left() + rad + rad * a.cos(), r.top() + rad + rad * a.sin()));
    }
    for i in 0..=n {
        let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::FRAC_PI_2 * i as f32 / n as f32;
        pts.push(pos2(r.right() - rad + rad * a.cos(), r.top() + rad + rad * a.sin()));
    }
    let top_n = pts.len() as u32;
    for q in &pts {
        m.colored_vertex(*q, top);
    }
    m.colored_vertex(r.right_bottom(), bottom);
    m.colored_vertex(r.left_bottom(), bottom);
    let br = top_n;
    let bl = top_n + 1;
    for i in 0..top_n - 1 {
        m.add_triangle(bl, i, i + 1);
    }
    m.add_triangle(bl, top_n - 1, br);
    p.add(Shape::mesh(m));
}

pub fn nav_item(ui: &mut Ui, label: &str, sel: bool, icon: impl Fn(&Painter, Pos2, Color32)) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    let p = ui.painter().clone();
    let col = if sel || resp.hovered() { TEXT } else { SUBTLE };
    icon(&p, pos2(r.left() + 16.0, r.center().y), col);
    p.text(pos2(r.left() + 44.0, r.center().y), Align2::LEFT_CENTER, label, bold(15.5), col);
    resp
}

pub fn lib_row(ui: &mut Ui, title: &str, sub: &str, art: Art, sel: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 60.0), Sense::click());
    let p = ui.painter().clone();
    if sel {
        p.rect_filled(r, cr(6), Color32::from_white_alpha(22));
    } else if resp.hovered() {
        p.rect_filled(r, cr(6), Color32::from_white_alpha(12));
    }
    let cv = Rect::from_min_size(pos2(r.left() + 8.0, r.center().y - 22.0), vec2(44.0, 44.0));
    draw_art(&p, cv, &art);
    let tw = r.width() - 72.0;
    fit_text(&p, pos2(cv.right() + 12.0, r.center().y - 1.0), Align2::LEFT_BOTTOM, title, reg(15.0), if sel { ACCENT } else { TEXT }, tw);
    fit_text(&p, pos2(cv.right() + 12.0, r.center().y + 2.0), Align2::LEFT_TOP, sub, reg(12.5), SUBTLE, tw);
    resp
}

pub fn pill_button(ui: &mut Ui, text: &str, w: f32, h: f32, fill: Color32, text_col: Color32) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    let p = ui.painter();
    let (r2, f) = if resp.hovered() { (r.expand(1.0), fill.gamma_multiply(1.1)) } else { (r, fill) };
    p.rect_filled(r2, cr((h / 2.0) as u8), f);
    p.text(r.center(), Align2::CENTER_CENTER, text, bold(15.0), text_col);
    resp
}

pub fn outline_button(ui: &mut Ui, text: &str, w: f32, h: f32) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    let p = ui.painter();
    let col = if resp.hovered() { TEXT } else { SUBTLE };
    p.rect_stroke(r, cr((h / 2.0) as u8), Stroke::new(1.0_f32, col), egui::StrokeKind::Inside);
    p.text(r.center(), Align2::CENTER_CENTER, text, bold(13.5), col);
    resp
}

pub fn chip(ui: &mut Ui, text: &str, sel: bool) -> egui::Response {
    let g = ui.painter().layout_no_wrap(text.to_string(), reg(14.0), TEXT);
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 28.0, 32.0), Sense::click());
    let p = ui.painter();
    let fill = if sel { TEXT } else if resp.hovered() { Color32::from_white_alpha(40) } else { Color32::from_white_alpha(22) };
    p.rect_filled(r, cr(16), fill);
    p.text(r.center(), Align2::CENTER_CENTER, text, reg(14.0), if sel { BG } else { TEXT });
    resp
}

pub fn field_label(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text).font(bold(13.0)).color(TEXT));
}

pub fn text_field<'a>(s: &'a mut String, hint: &'a str, password: bool, w: f32) -> TextEdit<'a> {
    TextEdit::singleline(s).hint_text(hint).password(password).desired_width(w).margin(Margin::symmetric(12, 10)).font(reg(15.0))
}

pub fn enter_pressed(ui: &Ui, r: &egui::Response) -> bool {
    r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
}

pub fn section_box(ui: &mut Ui, w: f32, add: impl FnOnce(&mut Ui)) {
    Frame::new().fill(Color32::from_white_alpha(10)).corner_radius(cr(10)).inner_margin(Margin::same(20)).show(ui, |ui| {
        ui.set_width(w - 40.0);
        ui.set_min_height(180.0);
        ui.vertical(|ui| add(ui));
    });
}

pub enum SliderEv {
    Drag(f32),
    Done(f32),
}

pub fn slider_bar(ui: &mut Ui, p: &Painter, bar: Rect, id: &str, frac: f32, enabled: bool) -> Option<SliderEv> {
    let resp = ui.interact(bar.expand2(vec2(6.0, 8.0)), Id::new(("slider", id)), Sense::click_and_drag());
    let hot = enabled && (resp.hovered() || resp.dragged());
    p.rect_filled(bar, cr(2), Color32::from_white_alpha(50));
    let fill = Rect::from_min_max(bar.min, pos2(bar.left() + bar.width() * frac, bar.bottom()));
    p.rect_filled(fill, cr(2), if hot { ACCENT } else { TEXT });
    if hot {
        p.circle_filled(pos2(fill.right(), bar.center().y), 6.0, TEXT);
    }
    if !enabled {
        return None;
    }
    let at = |x: f32| ((x - bar.left()) / bar.width()).clamp(0.0, 1.0);
    if resp.drag_stopped() {
        return resp.interact_pointer_pos().map(|q| SliderEv::Done(at(q.x))).or(Some(SliderEv::Done(frac)));
    }
    if resp.dragged() {
        return resp.interact_pointer_pos().map(|q| SliderEv::Drag(at(q.x)));
    }
    if resp.clicked() {
        return resp.interact_pointer_pos().map(|q| SliderEv::Done(at(q.x)));
    }
    None
}

pub fn long_time(secs: f32) -> String {
    let s = secs.round() as u32;
    if s >= 3600 {
        format!("{} hr {} min", s / 3600, (s % 3600) / 60)
    } else {
        format!("{} min {} sec", s / 60, s % 60)
    }
}
