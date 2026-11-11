use super::widgets::*;
use super::*;
use crate::accounts::member_since;

impl WaveletApp {
    pub(super) fn view_content(&mut self, ui: &mut Ui) {
        match self.view.clone() {
            View::Home => self.home_view(ui),
            View::Search => self.search_view(ui),
            View::Library => self.library_view(ui),
            View::Liked => {
                let name = self.user.as_ref().map(|u| u.display_name.clone()).unwrap_or_default();
                let ids = self.data.liked.clone();
                let sub = format!("{name} · {} songs", ids.len());
                self.collection_view(ui, "Playlist", "Liked Songs".into(), sub, String::new(), Art::Liked, ids, TableKind::Plain, None);
            }
            View::Playlist(i) => {
                let Some(pl) = self.data.playlists.get(i).cloned() else {
                    ui.label("This playlist no longer exists.");
                    return;
                };
                let name = self.user.as_ref().map(|u| u.display_name.clone()).unwrap_or_default();
                let total: f32 = pl.tracks.iter().filter_map(|id| self.track(id)).map(|t| t.duration).sum();
                let sub = format!("{name} · {} songs, {}", pl.tracks.len(), long_time(total));
                self.collection_view(ui, "Playlist", pl.name.clone(), sub, pl.description.clone(), Art::Cover(pl.hue, pl.name.clone()), pl.tracks.clone(), TableKind::Playlist(i), Some(i));
            }
            View::Album(a) => {
                let Some(al) = self.albums.iter().find(|x| x.name == a).cloned() else {
                    ui.label("Album not found.");
                    return;
                };
                let total: f32 = al.tracks.iter().filter_map(|id| self.track(id)).map(|t| t.duration).sum();
                let sub = format!("{} · {} songs, {}", al.artist, al.tracks.len(), long_time(total));
                let kind = if al.local { "Album · On this computer" } else { "Album" };
                self.collection_view(ui, kind, al.name.clone(), sub, String::new(), Art::Cover(al.hue, al.name.clone()), al.tracks.clone(), TableKind::Plain, None);
            }
            View::Mix(i) => {
                let Some(m) = self.mixes.get(i) else { return };
                let (name, desc, hue, ids) = (m.name.clone(), m.desc.clone(), m.hue, m.ids.clone());
                let sub = format!("Made for you · {} songs", ids.len());
                self.collection_view(ui, "Mix", name.clone(), sub, desc, Art::Cover(hue, name), ids, TableKind::Plain, None);
            }
            View::Local => self.local_view(ui),
            View::Queue => self.queue_view(ui),
            View::Account => self.account_view(ui),
        }
    }

    fn home_view(&mut self, ui: &mut Ui) {
        let first = self.user.as_ref().map(|u| u.display_name.split_whitespace().next().unwrap_or("").to_string()).unwrap_or_default();
        ui.add_space(4.0);
        ui.label(RichText::new(format!("Welcome back, {first}")).font(bold(30.0)));
        ui.add_space(14.0);

        let mut tiles: Vec<CardItem> = vec![CardItem { title: "Liked Songs".into(), subtitle: String::new(), art: Art::Liked, view: View::Liked }];
        for (i, p) in self.data.playlists.iter().enumerate().take(3) {
            tiles.push(CardItem { title: p.name.clone(), subtitle: String::new(), art: Art::Cover(p.hue, p.name.clone()), view: View::Playlist(i) });
        }
        for (i, m) in self.mixes.iter().enumerate() {
            if tiles.len() >= 6 {
                break;
            }
            tiles.push(CardItem { title: m.name.clone(), subtitle: String::new(), art: Art::Cover(m.hue, m.name.clone()), view: View::Mix(i) });
        }
        for a in self.albums.iter().filter(|a| !a.local) {
            if tiles.len() >= 6 {
                break;
            }
            tiles.push(CardItem { title: a.name.clone(), subtitle: String::new(), art: Art::Cover(a.hue, a.name.clone()), view: View::Album(a.name.clone()) });
        }
        let w = ui.available_width();
        let gap = 12.0;
        let cols = if w > 900.0 { 3 } else { 2 };
        let tw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
        for row in tiles.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for t in row {
                    self.quick_tile(ui, t, tw);
                }
            });
            ui.add_space(4.0);
        }

        ui.add_space(20.0);
        self.home_trending(ui);
        self.home_hits(ui);
        self.home_real_music(ui);
        let mixes: Vec<CardItem> = self
            .mixes
            .iter()
            .enumerate()
            .map(|(i, m)| CardItem { title: m.name.clone(), subtitle: m.desc.clone(), art: Art::Cover(m.hue, m.name.clone()), view: View::Mix(i) })
            .collect();
        let name = self.user.as_ref().map(|u| u.display_name.clone()).unwrap_or_default();
        self.shelf(ui, &format!("Made for {name}"), mixes, false);

        let mut recent_albums: Vec<String> = vec![];
        for id in &self.data.recent {
            if let Some(t) = self.track(id) {
                if !recent_albums.contains(&t.album) {
                    recent_albums.push(t.album.clone());
                }
            }
        }
        if !recent_albums.is_empty() {
            let items: Vec<CardItem> = recent_albums
                .iter()
                .filter_map(|n| self.albums.iter().find(|a| &a.name == n))
                .map(|a| CardItem { title: a.name.clone(), subtitle: a.artist.clone(), art: Art::Cover(a.hue, a.name.clone()), view: View::Album(a.name.clone()) })
                .collect();
            self.shelf(ui, "Recently played", items, false);
        }

        let items: Vec<CardItem> = self
            .albums
            .iter()
            .filter(|a| !a.local)
            .map(|a| CardItem { title: a.name.clone(), subtitle: a.artist.clone(), art: Art::Cover(a.hue, a.name.clone()), view: View::Album(a.name.clone()) })
            .collect();
        self.shelf(ui, "Albums to explore", items, true);

        let local: Vec<CardItem> = self
            .albums
            .iter()
            .filter(|a| a.local)
            .map(|a| CardItem { title: a.name.clone(), subtitle: a.artist.clone(), art: Art::Cover(a.hue, a.name.clone()), view: View::Album(a.name.clone()) })
            .collect();
        if !local.is_empty() {
            self.shelf(ui, "From your computer", local, true);
        }
    }

    fn quick_tile(&mut self, ui: &mut Ui, t: &CardItem, w: f32) {
        let (r, resp) = ui.allocate_exact_size(vec2(w, 64.0), Sense::click());
        let p = ui.painter().clone();
        p.rect_filled(r, cr(6), if resp.hovered() { Color32::from_white_alpha(40) } else { Color32::from_white_alpha(18) });
        let cvr = Rect::from_min_size(r.min, vec2(64.0, 64.0));
        draw_art(&p, cvr, &t.art);
        fit_text(&p, pos2(cvr.right() + 14.0, r.center().y), Align2::LEFT_CENTER, &t.title, bold(15.0), TEXT, w - 64.0 - if resp.hovered() { 76.0 } else { 24.0 });
        let ids = self.ids_for(&t.view);
        let playing_this = self.playing && !ids.is_empty() && self.context == ids;
        if resp.hovered() || playing_this {
            let c = pos2(r.right() - 32.0, r.center().y);
            let pr = Rect::from_center_size(c, vec2(40.0, 40.0));
            let presp = ui.interact(pr, Id::new(("qt_play", &t.title)), Sense::click());
            p.circle_filled(c, if presp.hovered() { 21.0 } else { 20.0 }, if presp.hovered() { ACCENT_HI } else { ACCENT });
            if playing_this {
                pause(&p, c, 14.0, BG);
            } else {
                play(&p, pos2(c.x + 1.0, c.y), 14.0, BG);
            }
            if presp.clicked() {
                self.pending.push(if playing_this { Action::TogglePlay } else { Action::PlayList(ids, 0) });
                return;
            }
        }
        if resp.clicked() {
            self.pending.push(Action::Go(t.view.clone()));
        }
    }

    fn shelf(&mut self, ui: &mut Ui, title: &str, items: Vec<CardItem>, wrap: bool) {
        if items.is_empty() {
            return;
        }
        ui.add_space(8.0);
        ui.label(RichText::new(title).font(bold(22.0)));
        ui.add_space(8.0);
        let w = ui.available_width();
        let gap = 12.0;
        let cols = (((w + gap) / (180.0 + gap)).floor() as usize).max(2);
        let cw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
        let take = if wrap { items.len() } else { cols.min(items.len()) };
        for row in items[..take].chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for it in row {
                    self.card(ui, it, cw);
                }
            });
            ui.add_space(6.0);
        }
        ui.add_space(14.0);
    }

    fn card(&mut self, ui: &mut Ui, it: &CardItem, w: f32) {
        let h = w + 50.0;
        let (r, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
        let p = ui.painter().clone();
        if resp.hovered() {
            p.rect_filled(r, cr(8), Color32::from_white_alpha(14));
        }
        let pad = 10.0;
        let cvr = Rect::from_min_size(r.min + vec2(pad, pad), vec2(w - 2.0 * pad, w - 2.0 * pad));
        p.rect_filled(cvr.translate(vec2(0.0, 4.0)), cr(6), Color32::from_black_alpha(60));
        draw_art(&p, cvr, &it.art);
        fit_text(&p, pos2(cvr.left(), cvr.bottom() + 10.0), Align2::LEFT_TOP, &it.title, bold(15.0), TEXT, cvr.width());
        fit_text(&p, pos2(cvr.left(), cvr.bottom() + 31.0), Align2::LEFT_TOP, &it.subtitle, reg(13.0), SUBTLE, cvr.width());
        if resp.hovered() {
            let c = pos2(cvr.right() - 28.0, cvr.bottom() - 28.0);
            let pr = Rect::from_center_size(c, vec2(46.0, 46.0));
            let presp = ui.interact(pr, Id::new(("card_play", &it.title)), Sense::click());
            p.circle_filled(c + vec2(0.0, 2.0), 23.0, Color32::from_black_alpha(90));
            p.circle_filled(c, 23.0, if presp.hovered() { ACCENT_HI } else { ACCENT });
            play(&p, pos2(c.x + 1.5, c.y), 16.0, BG);
            if presp.clicked() {
                let ids = self.ids_for(&it.view);
                self.pending.push(Action::PlayList(ids, 0));
                return;
            }
        }
        if resp.clicked() {
            self.pending.push(Action::Go(it.view.clone()));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn collection_view(&mut self, ui: &mut Ui, kind: &str, title: String, sub: String, desc: String, art: Art, ids: Vec<String>, table: TableKind, playlist: Option<usize>) {
        let w = ui.available_width();
        if w < 560.0 {
            self.collection_header_narrow(ui, kind, &title, &sub, &desc, &art, playlist);
        } else {
        let (hr, _) = ui.allocate_exact_size(vec2(w, 220.0), Sense::hover());
        let p = ui.painter().clone();
        let cv = Rect::from_min_size(pos2(hr.left(), hr.top() + 10.0), vec2(200.0, 200.0));
        p.rect_filled(cv.translate(vec2(0.0, 6.0)), cr(4), Color32::from_black_alpha(90));
        draw_art(&p, cv, &art);
        let tx = cv.right() + 24.0;
        let tw = hr.right() - tx;
        p.text(pos2(tx, cv.top() + 64.0), Align2::LEFT_BOTTOM, kind, bold(13.0), TEXT);
        let size = if title.chars().count() > 18 { 40.0 } else { 64.0 };
        fit_text(&p, pos2(tx - 2.0, cv.top() + 138.0), Align2::LEFT_BOTTOM, &title, bold(size), TEXT, tw);
        if !desc.is_empty() {
            fit_text(&p, pos2(tx, cv.top() + 168.0), Align2::LEFT_BOTTOM, &desc, reg(14.0), SUBTLE, tw);
        }
        fit_text(&p, pos2(tx, cv.bottom()), Align2::LEFT_BOTTOM, &sub, reg(14.0), TEXT, tw);
        if let Some(i) = playlist {
            let tr = Rect::from_min_max(pos2(tx, cv.top() + 70.0), pos2(hr.right(), cv.top() + 140.0));
            if ui.interact(tr, Id::new("pl_title"), Sense::click()).on_hover_text("Edit details").clicked() {
                self.pending.push(Action::EditPlaylist(i));
            }
        }
        }

        ui.add_space(16.0);
        let p = ui.painter().clone();
        let (ar, _) = ui.allocate_exact_size(vec2(w, 64.0), Sense::hover());
        let c = pos2(ar.left() + 30.0, ar.center().y);
        let presp = ui.interact(Rect::from_center_size(c, vec2(58.0, 58.0)), Id::new("big_play"), Sense::click());
        let this_ctx = !ids.is_empty() && self.context == ids && self.current.is_some();
        p.circle_filled(c, if presp.hovered() { 30.0 } else { 28.0 }, if ids.is_empty() { FAINT } else if presp.hovered() { ACCENT_HI } else { ACCENT });
        if this_ctx && self.playing {
            pause(&p, c, 20.0, BG);
        } else {
            play(&p, pos2(c.x + 2.0, c.y), 20.0, BG);
        }
        if presp.clicked() && !ids.is_empty() {
            self.pending.push(if this_ctx { Action::TogglePlay } else { Action::PlayList(ids.clone(), 0) });
        }
        let sc = pos2(ar.left() + 90.0, ar.center().y);
        let sresp = ui.interact(Rect::from_center_size(sc, vec2(36.0, 36.0)), Id::new("big_shuffle"), Sense::click()).on_hover_text("Shuffle");
        shuffle(&p, sc, 26.0, if self.shuffle { ACCENT } else if sresp.hovered() { TEXT } else { SUBTLE });
        if self.shuffle {
            p.circle_filled(pos2(sc.x, sc.y + 18.0), 2.0, ACCENT);
        }
        if sresp.clicked() {
            self.pending.push(Action::ToggleShuffle);
        }
        if let Some(i) = playlist {
            let dc = pos2(ar.left() + 140.0, ar.center().y);
            let dresp = ui.interact(Rect::from_center_size(dc, vec2(36.0, 36.0)), Id::new("pl_more"), Sense::click()).on_hover_text("More options");
            dots(&p, dc, 26.0, if dresp.hovered() { TEXT } else { SUBTLE });
            let pid = Id::new("pl_more_popup");
            if dresp.clicked() {
                ui.memory_mut(|m| m.toggle_popup(pid));
            }
            egui::popup::popup_below_widget(ui, pid, &dresp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                ui.set_min_width(170.0);
                if ui.button("Edit details").clicked() {
                    self.pending.push(Action::EditPlaylist(i));
                }
                if ui.button("Add songs").clicked() {
                    self.pending.push(Action::Go(View::Search));
                }
                if ui.button("Delete playlist").clicked() {
                    self.pending.push(Action::DeletePlaylist(i));
                }
            });
        }
        ui.add_space(8.0);

        if ids.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Nothing here yet").font(bold(22.0)));
                ui.label(RichText::new("Find songs you love and add them here. Right-click any song for options.").color(SUBTLE));
                ui.add_space(10.0);
                if pill_button(ui, "Find songs", 140.0, 40.0, TEXT, BG).clicked() {
                    self.pending.push(Action::Go(View::Search));
                }
            });
            return;
        }
        self.track_table(ui, &ids, table, true);
    }

    fn collection_header_narrow(&mut self, ui: &mut Ui, kind: &str, title: &str, sub: &str, desc: &str, art: &Art, playlist: Option<usize>) {
        let w = ui.available_width();
        let side = 180.0f32.min(w - 40.0);
        let h = side + 20.0 + 26.0 + 44.0 + if desc.is_empty() { 0.0 } else { 22.0 } + 22.0;
        let (hr, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
        let p = ui.painter().clone();
        let cv = Rect::from_min_size(pos2(hr.center().x - side / 2.0, hr.top() + 6.0), vec2(side, side));
        p.rect_filled(cv.translate(vec2(0.0, 6.0)), cr(4), Color32::from_black_alpha(90));
        draw_art(&p, cv, art);
        let mut y = cv.bottom() + 20.0;
        p.text(pos2(hr.left(), y), Align2::LEFT_TOP, kind, bold(12.0), SUBTLE);
        y += 18.0;
        let tr = fit_text(&p, pos2(hr.left(), y), Align2::LEFT_TOP, title, bold(28.0), TEXT, w);
        y += 40.0;
        if !desc.is_empty() {
            fit_text(&p, pos2(hr.left(), y), Align2::LEFT_TOP, desc, reg(13.5), SUBTLE, w);
            y += 22.0;
        }
        fit_text(&p, pos2(hr.left(), y), Align2::LEFT_TOP, sub, reg(13.5), TEXT, w);
        if let Some(i) = playlist {
            if ui.interact(tr, Id::new("pl_title"), Sense::click()).clicked() {
                self.pending.push(Action::EditPlaylist(i));
            }
        }
    }

    /// Song table showing `shown`, where playing a row continues through all of `full`.
    fn track_table_in(&mut self, ui: &mut Ui, shown: &[String], full: &[String]) {
        self.table_ctx = Some(full.to_vec());
        self.track_table(ui, shown, TableKind::Plain, true);
        self.table_ctx = None;
    }

    fn track_table(&mut self, ui: &mut Ui, ids: &[String], kind: TableKind, show_album: bool) {
        let play_ids: Vec<String> = self.table_ctx.take().unwrap_or_else(|| ids.to_vec());
        let w = ui.available_width();
        let p = ui.painter().clone();
        let show_album = show_album && w >= 600.0;
        let narrow = w < 560.0;
        let album_x = w * 0.52;
        let (hr, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
        p.text(pos2(hr.left() + 22.0, hr.center().y), Align2::CENTER_CENTER, "#", reg(13.0), SUBTLE);
        p.text(pos2(hr.left() + 52.0, hr.center().y), Align2::LEFT_CENTER, "Title", reg(13.0), SUBTLE);
        if show_album {
            p.text(pos2(hr.left() + album_x, hr.center().y), Align2::LEFT_CENTER, "Album", reg(13.0), SUBTLE);
        }
        p.text(pos2(hr.right() - 30.0, hr.center().y), Align2::RIGHT_CENTER, "Time", reg(13.0), SUBTLE);
        p.line_segment([pos2(hr.left(), hr.bottom()), pos2(hr.right(), hr.bottom())], Stroke::new(1.0_f32, Color32::from_white_alpha(20)));
        ui.add_space(6.0);
        let time = ui.input(|i| i.time);
        let playlists: Vec<String> = self.data.playlists.iter().map(|p| p.name.clone()).collect();

        for (row, id) in ids.iter().enumerate() {
            let Some(t) = self.track(id).cloned() else { continue };
            let (r, resp) = ui.allocate_exact_size(vec2(w, 56.0), Sense::click());
            if !ui.is_rect_visible(r) {
                continue;
            }
            let is_cur = self.current.as_deref() == Some(id.as_str());
            let hovered = resp.hovered() || resp.context_menu_opened();
            if hovered {
                p.rect_filled(r, cr(6), Color32::from_white_alpha(16));
            }
            let nc = pos2(r.left() + 22.0, r.center().y);
            let num_resp = ui.interact(Rect::from_center_size(nc, vec2(32.0, 32.0)), Id::new(("row_play", row, id)), Sense::click());
            if hovered || num_resp.hovered() {
                if is_cur && self.playing {
                    pause(&p, nc, 14.0, TEXT);
                } else {
                    play(&p, nc, 14.0, TEXT);
                }
            } else if is_cur && self.playing {
                for k in 0..3 {
                    let hh = 4.0 + 10.0 * (0.5 + 0.5 * ((time * 6.0 + k as f64 * 1.7).sin() as f32));
                    p.rect_filled(Rect::from_center_size(pos2(nc.x - 5.0 + k as f32 * 5.0, nc.y + 7.0 - hh / 2.0), vec2(3.0, hh)), cr(1), ACCENT);
                }
            } else {
                p.text(nc, Align2::CENTER_CENTER, format!("{}", row + 1), reg(15.0), if is_cur { ACCENT } else { SUBTLE });
            }
            if num_resp.clicked() {
                self.pending.push(if is_cur { Action::TogglePlay } else { Action::PlayList(play_ids.clone(), row) });
            }
            let cvr = Rect::from_min_size(pos2(r.left() + 48.0, r.center().y - 20.0), vec2(40.0, 40.0));
            let art = self.art_for(&t);
            draw_art(&p, cvr, &art);
            let title_w = if show_album { album_x - 110.0 } else { w - 230.0 };
            fit_text(&p, pos2(cvr.right() + 12.0, r.center().y - 1.0), Align2::LEFT_BOTTOM, &t.title, reg(15.5), if is_cur { ACCENT } else { TEXT }, title_w);
            let mut ax = cvr.right() + 12.0;
            if t.preview {
                let g = p.layout_no_wrap("PREVIEW".into(), bold(9.5), BG);
                let br = Rect::from_min_size(pos2(ax, r.center().y + 4.0), vec2(g.size().x + 8.0, 14.0));
                p.rect_filled(br, cr(3), SUBTLE);
                p.galley(pos2(br.left() + 4.0, br.center().y - g.size().y / 2.0), g, BG);
                ax = br.right() + 6.0;
            }
            fit_text(&p, pos2(ax, r.center().y + 2.0), Align2::LEFT_TOP, &t.artist, reg(13.5), SUBTLE, title_w - (ax - cvr.right() - 12.0));
            if show_album {
                fit_text(&p, pos2(r.left() + album_x, r.center().y), Align2::LEFT_CENTER, &t.album, reg(14.0), SUBTLE, w - album_x - 140.0);
            }
            let liked = self.data.liked.contains(id);
            let hc = pos2(r.right() - 100.0, r.center().y);
            let hresp = ui.interact(Rect::from_center_size(hc, vec2(28.0, 28.0)), Id::new(("row_like", row, id)), Sense::click());
            if liked {
                heart(&p, hc, 15.0, ACCENT, true);
            } else if hovered || hresp.hovered() {
                heart(&p, hc, 15.0, if hresp.hovered() { TEXT } else { SUBTLE }, false);
            }
            if hresp.clicked() {
                self.pending.push(Action::ToggleLike(id.clone()));
            }
            p.text(pos2(r.right() - 30.0, r.center().y), Align2::RIGHT_CENTER, fmt_time(t.duration), reg(14.0), SUBTLE);
            if resp.double_clicked() || (narrow && resp.clicked()) {
                self.pending.push(Action::PlayList(play_ids.clone(), row));
            }
            resp.context_menu(|ui| {
                ui.set_min_width(210.0);
                if ui.button("Play").clicked() {
                    self.pending.push(Action::PlayList(play_ids.clone(), row));
                    ui.close_menu();
                }
                if ui.button("Add to queue").clicked() {
                    self.pending.push(Action::Queue(id.clone()));
                    ui.close_menu();
                }
                if t.preview && ui.button("Play full song on Spotify").clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(crate::online::spotify_link(&t.title, &t.artist)));
                    ui.close_menu();
                }
                let like_label = if liked { "Remove from Liked Songs" } else { "Save to Liked Songs" };
                if ui.button(like_label).clicked() {
                    self.pending.push(Action::ToggleLike(id.clone()));
                    ui.close_menu();
                }
                ui.menu_button("Add to playlist", |ui| {
                    if ui.button("+ New playlist").clicked() {
                        self.pending.push(Action::NewPlaylist(Some(id.clone())));
                        ui.close_menu();
                    }
                    if !playlists.is_empty() {
                        ui.separator();
                    }
                    for (pi, name) in playlists.iter().enumerate() {
                        if ui.button(name).clicked() {
                            self.pending.push(Action::AddToPlaylist(id.clone(), pi));
                            ui.close_menu();
                        }
                    }
                });
                if let TableKind::Playlist(pi) = kind {
                    if ui.button("Remove from this playlist").clicked() {
                        self.pending.push(Action::RemoveFromPlaylist(pi, row));
                        ui.close_menu();
                    }
                }
                if self.albums.iter().any(|a| a.name == t.album) {
                    ui.separator();
                    if ui.button("Go to album").clicked() {
                        self.pending.push(Action::Go(View::Album(t.album.clone())));
                        ui.close_menu();
                    }
                }
            });
        }
    }

    fn search_view(&mut self, ui: &mut Ui) {
        let q = self.search.trim().to_lowercase();
        if q.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new("Browse all").font(bold(24.0)));
            ui.add_space(12.0);
            let genres: [(&str, f32); 7] = [("Electronic", 0.74), ("Chill", 0.11), ("Lo-fi", 0.53), ("Ambient", 0.03), ("Dance", 0.9), ("Made for you", 0.35), ("Local files", 0.48)];
            let w = ui.available_width();
            let gap = 16.0;
            let cols = (((w + gap) / (220.0 + gap)).floor() as usize).max(2);
            let tw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
            for row in genres.chunks(cols) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (g, hue) in row {
                        let (r, resp) = ui.allocate_exact_size(vec2(tw, tw * 0.55), Sense::click());
                        let p = ui.painter().with_clip_rect(r);
                        p.rect_filled(r, cr(8), hsv(*hue, 0.6, if resp.hovered() { 0.75 } else { 0.62 }));
                        let art = Rect::from_min_size(pos2(r.right() - tw * 0.3, r.bottom() - tw * 0.26), vec2(tw * 0.32, tw * 0.32));
                        cover(&p, art, *hue + 0.05, g);
                        p.text(r.left_top() + vec2(16.0, 14.0), Align2::LEFT_TOP, *g, bold(22.0), TEXT);
                        if resp.clicked() {
                            if *g == "Made for you" {
                                self.pending.push(Action::Go(View::Mix(0)));
                            } else if *g == "Local files" {
                                self.pending.push(if self.data.local.is_empty() { Action::AddFolder } else { Action::Go(View::Local) });
                            } else {
                                self.search = g.to_string();
                            }
                        }
                    }
                });
                ui.add_space(gap - 6.0);
            }
            return;
        }

        let songs: Vec<String> = self
            .tracks
            .iter()
            .filter(|t| t.title.to_lowercase().contains(&q) || t.artist.to_lowercase().contains(&q) || t.album.to_lowercase().contains(&q) || t.genre.to_lowercase().contains(&q))
            .map(|t| t.id.clone())
            .collect();
        let albums: Vec<CardItem> = self
            .albums
            .iter()
            .filter(|a| a.name.to_lowercase().contains(&q) || a.artist.to_lowercase().contains(&q) || a.tracks.iter().any(|id| songs.contains(id)))
            .map(|a| CardItem { title: a.name.clone(), subtitle: a.artist.clone(), art: Art::Cover(a.hue, a.name.clone()), view: View::Album(a.name.clone()) })
            .collect();
        let pls: Vec<CardItem> = self
            .data
            .playlists
            .iter()
            .enumerate()
            .filter(|(_, p)| p.name.to_lowercase().contains(&q))
            .map(|(i, p)| CardItem { title: p.name.clone(), subtitle: format!("{} songs", p.tracks.len()), art: Art::Cover(p.hue, p.name.clone()), view: View::Playlist(i) })
            .collect();
        let has_key = self.has_key();
        self.full_songs_section(ui);
        self.catalog_section(ui);
        if has_key {
            self.online_results_section(ui);
        }
        if songs.is_empty() && albums.is_empty() && pls.is_empty() {
            if self.cat_results.is_empty() && !self.cat_busy && self.search.trim() == self.cat_sent {
                ui.add_space(60.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(format!("No results found for \"{}\"", self.search.trim())).font(bold(22.0)));
                    ui.label(RichText::new("Check the spelling, or try different words.").color(SUBTLE));
                });
            }
            return;
        }
        if !songs.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new(if has_key { "In your library" } else { "Songs" }).font(bold(22.0)));
            ui.add_space(4.0);
            let shown: Vec<String> = songs.into_iter().take(40).collect();
            self.track_table(ui, &shown, TableKind::Plain, true);
            ui.add_space(16.0);
        }
        self.shelf(ui, "Albums", albums, true);
        self.shelf(ui, "Your playlists", pls, true);
    }

    fn online_results_section(&mut self, ui: &mut Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Full songs").font(bold(22.0)));
            ui.label(RichText::new("· independent artists on Jamendo").color(SUBTLE));
        });
        ui.add_space(4.0);
        let q = self.search.trim().to_string();
        if self.online_busy || q != self.online_sent {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Searching…").color(SUBTLE));
            });
        } else if let Some(e) = self.online_err.clone() {
            ui.label(RichText::new(e).color(Color32::from_rgb(255, 110, 110)));
        } else if self.online_results.is_empty() {
            ui.label(RichText::new(format!("No songs found online for \"{q}\".")).color(SUBTLE));
        } else {
            let ids = self.online_results.clone();
            self.track_table(ui, &ids, TableKind::Plain, true);
        }
        ui.add_space(20.0);
    }

    fn catalog_section(&mut self, ui: &mut Ui) {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Chart songs").font(bold(22.0)));
            ui.label(RichText::new(online::chart_note()).color(TEXT));
        });
        ui.add_space(4.0);
        let q = self.search.trim().to_string();
        if self.cat_busy || q != self.cat_sent {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Searching every song…").color(SUBTLE));
            });
        } else if let Some(e) = self.cat_err.clone() {
            ui.label(RichText::new(e).color(Color32::from_rgb(255, 110, 110)));
            if ui.small_button("Try again").clicked() {
                self.cat_sent.clear();
            }
        } else if self.cat_results.is_empty() {
            ui.label(RichText::new(format!("No songs found for \"{q}\". Try just the song name or the artist.")).color(SUBTLE));
        } else {
            let cleaned = crate::online::clean_query(&q);
            if cleaned != q.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ") {
                ui.label(RichText::new(format!("Showing songs for \"{cleaned}\"")).color(TEXT));
            }
            let ids = self.cat_results.clone();
            self.track_table(ui, &ids, TableKind::Plain, true);
        }
        ui.add_space(20.0);
    }

    /// Section title with a "Play all" button.
    fn section_title(&mut self, ui: &mut Ui, title: &str, note: &str, ids: &[String]) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(title).font(bold(22.0)));
            ui.label(RichText::new(note).color(TEXT));
            if !ids.is_empty() {
                ui.add_space(8.0);
                if pill_button(ui, "Play all", 92.0, 30.0, ACCENT, BG).clicked() {
                    self.pending.push(Action::PlayList(ids.to_vec(), 0));
                }
            }
        });
    }

    fn full_songs_section(&mut self, ui: &mut Ui) {
        let ids = if self.full_busy { vec![] } else { self.full_results.clone() };
        ui.add_space(4.0);
        self.section_title(ui, "Full songs", "· complete tracks from Audius", &ids);
        ui.add_space(4.0);
        let q = self.search.trim().to_string();
        if self.full_busy || q != self.full_sent {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Searching full songs…").color(SUBTLE));
            });
        } else if let Some(e) = self.full_err.clone() {
            ui.label(RichText::new(e).color(Color32::from_rgb(255, 110, 110)));
            if ui.small_button("Try again").clicked() {
                self.full_sent.clear();
            }
        } else if ids.is_empty() {
            ui.label(RichText::new(format!("No full songs found for \"{q}\". Chart songs are below.")).color(SUBTLE));
        } else {
            let shown: Vec<String> = ids.into_iter().take(10).collect();
            self.track_table(ui, &shown, TableKind::Plain, true);
        }
        ui.add_space(20.0);
    }

    fn home_trending(&mut self, ui: &mut Ui) {
        let ids: Vec<String> = self.trend.clone();
        self.section_title(ui, "Trending now", "· full songs from Audius", &ids);
        ui.add_space(4.0);
        if self.trend_state < 2 {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Loading trending songs…").color(SUBTLE));
            });
        } else if let Some(e) = self.trend_err.clone() {
            ui.label(RichText::new(e).color(Color32::from_rgb(255, 110, 110)));
            if ui.small_button("Try again").clicked() {
                self.trend_state = 0;
            }
        } else {
            let shown: Vec<String> = ids.iter().take(10).cloned().collect();
            self.track_table_in(ui, &shown, &ids);
        }
        ui.add_space(24.0);
    }

    fn home_hits(&mut self, ui: &mut Ui) {
        let ids = self.hits.clone();
        self.section_title(ui, "Top chart hits", &online::chart_note(), &ids);
        ui.add_space(4.0);
        if self.hits_state < 2 {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Loading the charts…").color(SUBTLE));
            });
        } else if let Some(e) = self.hits_err.clone() {
            ui.label(RichText::new(e).color(Color32::from_rgb(255, 110, 110)));
            if ui.small_button("Try again").clicked() {
                self.hits_state = 0;
            }
        } else {
            let shown: Vec<String> = ids.iter().take(10).cloned().collect();
            self.track_table_in(ui, &shown, &ids);
        }
        ui.add_space(24.0);
    }

    fn home_real_music(&mut self, ui: &mut Ui) {
        // Only shown once a key is set: full songs already play without one.
        if !self.has_key() {
            return;
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Popular this week").font(bold(22.0)));
            ui.label(RichText::new("· real music from Jamendo").color(SUBTLE));
        });
        ui.add_space(4.0);
        if self.popular_state < 2 {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Loading songs…").color(SUBTLE));
            });
        } else if let Some(e) = self.popular_err.clone() {
            ui.label(RichText::new(e).color(Color32::from_rgb(255, 110, 110)));
            if ui.small_button("Try again").clicked() {
                self.popular_state = 0;
            }
        } else {
            let ids: Vec<String> = self.popular.iter().take(8).cloned().collect();
            self.track_table(ui, &ids, TableKind::Plain, true);
        }
        ui.add_space(24.0);
    }

    fn real_music_box(&mut self, ui: &mut Ui, w: f32) {
        let connected = self.has_key();
        section_box(ui, w, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Jamendo (optional)").font(bold(18.0)));
                if connected {
                    ui.label(RichText::new("● Connected").color(ACCENT));
                }
            });
            ui.add_space(4.0);
            ui.label(RichText::new("Full songs already play with no key. A free key of your own adds Jamendo's catalogue of independent artists (Creative Commons) on top. Keys copied from public lists don't work: Jamendo switches off apps it didn't issue to you.").color(SUBTLE));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("1. Make a free account at").color(SUBTLE));
                ui.hyperlink_to("devportal.jamendo.com", online::KEY_SITE);
            });
            ui.label(RichText::new("2. Choose \"Create an app\" (any name works).").color(SUBTLE));
            ui.label(RichText::new("3. Copy its Client ID and paste it below.").color(SUBTLE));
            field_label(ui, "Your key (Client ID)");
            ui.add(text_field(&mut self.key_buf, "Paste your Client ID here", false, (w - 40.0).min(420.0)));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let label = if self.key_testing { "Checking…" } else { "Save & connect" };
                if pill_button(ui, label, 160.0, 36.0, ACCENT, BG).clicked() {
                    self.pending.push(Action::SaveKey(self.key_buf.clone()));
                }
                if connected && ui.add(egui::Button::new(RichText::new("Turn off").color(SUBTLE)).frame(false)).clicked() {
                    self.key_buf.clear();
                    self.pending.push(Action::SaveKey(String::new()));
                }
            });
            if let Some((m, ok)) = &self.key_msg {
                ui.add_space(6.0);
                ui.label(RichText::new(m).color(if *ok { ACCENT } else { Color32::from_rgb(255, 110, 110) }));
            }
        });
    }

    /// Whole songs instead of 30-second previews: one click sets up the helper program.
    fn full_songs_box(&mut self, ui: &mut Ui, w: f32) {
        let on = online::full_songs_available();
        section_box(ui, w, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Full-length songs").font(bold(18.0)));
                if on {
                    ui.label(RichText::new("● On").color(ACCENT));
                }
            });
            ui.add_space(4.0);
            if on {
                ui.label(RichText::new("Every song plays its whole length, like the original. Songs you pick from the charts are fetched in full, so they no longer stop after 30 seconds.").color(SUBTLE));
            } else if online::phone() {
                ui.label(RichText::new("On the phone, chart songs play their whole length whenever an independent artist has a whole version, and a 30-second preview otherwise. The computer app can fetch every chart song in full.").color(SUBTLE));
            } else if online::full_songs_missing() == "ffmpeg" {
                ui.label(RichText::new("The full-song helper is here, but ffmpeg is needed to turn songs into something the player can play. Install it, then come back.").color(SUBTLE));
                ui.add_space(4.0);
                ui.hyperlink_to("ffmpeg.org/download.html", online::FFMPEG_SITE);
            } else {
                ui.label(RichText::new("Songs from the charts play a 30-second preview, because the catalogue that lists them only shares that much of each song. Turn this on and the app fetches the whole song instead.").color(SUBTLE));
                ui.add_space(8.0);
                let label = if self.full_tool_busy { "Getting full-length songs…" } else { "Turn on full-length songs" };
                if pill_button(ui, label, 250.0, 36.0, ACCENT, BG).clicked() {
                    self.pending.push(Action::GetFullSongs);
                }
            }
            if let Some((m, ok)) = &self.full_tool_msg {
                ui.add_space(6.0);
                ui.label(RichText::new(m).color(if *ok { ACCENT } else { Color32::from_rgb(255, 110, 110) }));
            }
        });
    }

    fn library_view(&mut self, ui: &mut Ui) {
        ui.add_space(4.0);
        ui.label(RichText::new("Your Library").font(bold(30.0)));
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            for (f, label) in [(LibFilter::All, "All"), (LibFilter::Playlists, "Playlists"), (LibFilter::Albums, "Albums"), (LibFilter::Local, "Local files")] {
                if chip(ui, label, self.lib_filter == f).clicked() {
                    self.lib_filter = f;
                }
            }
            ui.add_space(12.0);
            if chip(ui, "+ New playlist", false).clicked() {
                self.pending.push(Action::NewPlaylist(None));
            }
            if chip(ui, "+ Add music folder", false).clicked() {
                self.pending.push(Action::AddFolder);
            }
        });
        ui.add_space(14.0);

        let mut items: Vec<CardItem> = vec![];
        let f = self.lib_filter;
        if f == LibFilter::All || f == LibFilter::Playlists {
            items.push(CardItem { title: "Liked Songs".into(), subtitle: format!("{} songs", self.data.liked.len()), art: Art::Liked, view: View::Liked });
            for (i, p) in self.data.playlists.iter().enumerate() {
                items.push(CardItem { title: p.name.clone(), subtitle: format!("Playlist · {} songs", p.tracks.len()), art: Art::Cover(p.hue, p.name.clone()), view: View::Playlist(i) });
            }
        }
        if f == LibFilter::All || f == LibFilter::Local {
            if !self.data.local.is_empty() {
                items.push(CardItem { title: "Local files".into(), subtitle: format!("{} songs", self.data.local.len()), art: Art::Local, view: View::Local });
            }
            for a in self.albums.iter().filter(|a| a.local) {
                items.push(CardItem { title: a.name.clone(), subtitle: format!("Album · {}", a.artist), art: Art::Cover(a.hue, a.name.clone()), view: View::Album(a.name.clone()) });
            }
        }
        if f == LibFilter::All || f == LibFilter::Albums {
            for a in self.albums.iter().filter(|a| !a.local) {
                items.push(CardItem { title: a.name.clone(), subtitle: format!("Album · {}", a.artist), art: Art::Cover(a.hue, a.name.clone()), view: View::Album(a.name.clone()) });
            }
        }
        if f == LibFilter::Local && self.data.local.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Play music from your computer").font(bold(22.0)));
                ui.label(RichText::new("Pick a folder with MP3, FLAC, WAV, OGG or M4A files and it will show up here.").color(SUBTLE));
                ui.add_space(10.0);
                if pill_button(ui, "Choose a folder", 170.0, 40.0, TEXT, BG).clicked() {
                    self.pending.push(Action::AddFolder);
                }
            });
            return;
        }
        let w = ui.available_width();
        let gap = 12.0;
        let cols = (((w + gap) / (180.0 + gap)).floor() as usize).max(2);
        let cw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
        for row in items.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for it in row {
                    self.card(ui, it, cw);
                }
            });
            ui.add_space(6.0);
        }
    }

    fn local_view(&mut self, ui: &mut Ui) {
        let ids = self.ids_for(&View::Local);
        let sub = format!("{} songs on this computer", ids.len());
        self.collection_view(ui, "Folder", "Local files".into(), sub, String::new(), Art::Local, ids, TableKind::Plain, None);
        ui.add_space(20.0);
        ui.label(RichText::new("Music folders").font(bold(18.0)));
        let folders = self.data.folders.clone();
        for (i, f) in folders.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(f.display().to_string()).color(SUBTLE));
                if ui.small_button("Rescan").clicked() {
                    self.pending.push(Action::RescanFolder(f.clone()));
                }
                if ui.small_button("Remove").clicked() {
                    self.pending.push(Action::RemoveFolder(i));
                }
            });
        }
        ui.add_space(6.0);
        if chip(ui, "+ Add another folder", false).clicked() {
            self.pending.push(Action::AddFolder);
        }
    }

    fn queue_view(&mut self, ui: &mut Ui) {
        ui.add_space(4.0);
        ui.label(RichText::new("Queue").font(bold(30.0)));
        ui.add_space(12.0);
        let Some(cur) = self.current.clone() else {
            ui.label(RichText::new("Nothing is playing yet. Pick a song to get started.").color(SUBTLE));
            return;
        };
        if ui.available_width() < 560.0 {
            self.now_playing_card(ui);
        } else {
            ui.label(RichText::new("Now playing").font(bold(17.0)).color(SUBTLE));
            self.track_table(ui, &[cur], TableKind::Plain, true);
        }
        if !self.queue.is_empty() {
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Next in queue").font(bold(17.0)).color(SUBTLE));
                if ui.small_button("Clear").clicked() {
                    self.queue.clear();
                }
            });
            let q: Vec<String> = self.queue.iter().cloned().collect();
            self.track_table(ui, &q, TableKind::Plain, true);
        }
        if !self.context.is_empty() {
            ui.add_space(16.0);
            ui.label(RichText::new(if self.shuffle { "Next up (shuffle is on)" } else { "Next up" }).font(bold(17.0)).color(SUBTLE));
            let upcoming: Vec<String> = self.context.iter().skip(self.ctx_pos + 1).take(30).cloned().collect();
            if upcoming.is_empty() {
                ui.label(RichText::new("End of the list.").color(SUBTLE));
            } else {
                self.track_table(ui, &upcoming, TableKind::Plain, true);
            }
        }
    }

    fn account_view(&mut self, ui: &mut Ui) {
        let Some(user) = self.user.clone() else { return };
        let w = ui.available_width();
        let narrow = w < 560.0;
        let (hr, _) = ui.allocate_exact_size(vec2(w, if narrow { 300.0 } else { 210.0 }), Sense::hover());
        let p = ui.painter().clone();
        let rad = if narrow { 64.0 } else { 92.0 };
        let ac = if narrow { pos2(hr.center().x, hr.top() + 80.0) } else { pos2(hr.left() + 96.0, hr.center().y) };
        p.circle_filled(ac + vec2(0.0, 6.0), rad, Color32::from_black_alpha(80));
        p.circle_filled(ac, rad, hsv(library::hue_of(&user.username), 0.55, 0.9));
        let initial = user.display_name.chars().next().unwrap_or('?').to_uppercase().to_string();
        p.text(ac, Align2::CENTER_CENTER, initial, bold(rad * 0.85), BG);
        let meta = format!("{} playlists · {} liked songs · Listening since {}", self.data.playlists.len(), self.data.liked.len(), member_since(user.created));
        if narrow {
            let y = ac.y + rad + 20.0;
            p.text(pos2(hr.center().x, y), Align2::CENTER_TOP, "Profile", bold(13.0), TEXT);
            fit_text(&p, pos2(hr.center().x, y + 20.0), Align2::CENTER_TOP, &user.display_name, bold(36.0), TEXT, w);
            fit_text(&p, pos2(hr.center().x, y + 68.0), Align2::CENTER_TOP, &meta, reg(13.0), SUBTLE, w);
        } else {
            let tx = hr.left() + 220.0;
            p.text(pos2(tx, hr.center().y - 44.0), Align2::LEFT_BOTTOM, "Profile", bold(13.0), TEXT);
            fit_text(&p, pos2(tx, hr.center().y + 26.0), Align2::LEFT_BOTTOM, &user.display_name, bold(60.0), TEXT, hr.right() - tx);
            fit_text(&p, pos2(tx, hr.center().y + 54.0), Align2::LEFT_BOTTOM, &meta, reg(14.0), SUBTLE, hr.right() - tx);
        }
        ui.add_space(20.0);

        let box_w = if narrow { w } else { (w - 16.0) / 2.0 };
        let plan = |ui: &mut Ui| {
            section_box(ui, box_w, |ui| {
                ui.label(RichText::new("Your plan").font(bold(18.0)));
                ui.add_space(4.0);
                ui.label(RichText::new("Free forever · Ad-free").font(bold(24.0)).color(ACCENT));
                ui.add_space(4.0);
                for line in ["No ads between songs, ever", "No sign-in needed", "Unlimited skips", "Play your own music files", "Unlimited playlists"] {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("✓").color(ACCENT));
                        ui.label(RichText::new(line).color(SUBTLE));
                    });
                }
            });
        };
        let mut save_name = false;
        let mut edit = |ui: &mut Ui, name: &mut String| {
            section_box(ui, box_w, |ui| {
                ui.label(RichText::new("Your name").font(bold(18.0)));
                ui.add_space(4.0);
                ui.label(RichText::new("Shown on your playlists and home page.").color(SUBTLE));
                field_label(ui, "Display name");
                ui.add(text_field(name, "Your name", false, box_w - 40.0));
                ui.add_space(8.0);
                if pill_button(ui, "Save name", 130.0, 36.0, TEXT, BG).clicked() {
                    save_name = true;
                }
            });
        };
        let mut name = self.acct_name.clone();
        if narrow {
            plan(ui);
            ui.add_space(12.0);
            edit(ui, &mut name);
        } else {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                plan(ui);
                edit(ui, &mut name);
            });
        }
        self.acct_name = name;
        if save_name {
            let n = self.acct_name.trim().to_string();
            if n.is_empty() {
                self.acct_msg = Some(("Name can't be empty.".into(), false));
            } else {
                self.accounts.rename(&user.username, &n);
                self.save_accounts();
                if let Some(u) = &mut self.user {
                    u.display_name = n;
                }
                self.acct_msg = Some(("Name saved.".into(), true));
            }
        }
        ui.add_space(16.0);
        self.full_songs_box(ui, w);
        ui.add_space(16.0);
        self.real_music_box(ui, w);
        if let Some((m, ok)) = &self.acct_msg {
            ui.add_space(10.0);
            ui.label(RichText::new(m).color(if *ok { ACCENT } else { Color32::from_rgb(255, 110, 110) }));
        }
    }
}
