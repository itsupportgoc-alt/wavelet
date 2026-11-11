mod views;
mod widgets;

use crate::accounts::{Account, AccountStore};
use crate::library::{self, AlbumInfo, LocalTrack, Playlist, Track, UserData};
use crate::player::Player;
use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Align2, Color32, Context, Frame, Id, Key, Margin, Pos2, Rect, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui, UiBuilder};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use crate::online::{self, OnlineTrack};
use std::sync::mpsc::{channel, Receiver, Sender};
use widgets::*;

static DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

thread_local! {
    /// Shows (true) or hides (false) the phone's on-screen keyboard. Set by the Android app.
    static KEYBOARD: std::cell::RefCell<Option<Box<dyn Fn(bool)>>> = std::cell::RefCell::new(None);
}

/// Lets the Android app open the on-screen keyboard when a text box (like Search) is tapped.
pub fn set_keyboard_hook(f: Box<dyn Fn(bool)>) {
    KEYBOARD.with(|k| *k.borrow_mut() = Some(f));
}

fn show_keyboard(show: bool) {
    KEYBOARD.with(|k| {
        if let Some(f) = k.borrow().as_ref() {
            f(show);
        }
    });
}

/// Where profiles and playlists are saved (Android passes the app's private folder).
pub fn set_data_dir(dir: PathBuf) {
    let _ = DATA_DIR.set(dir);
}

#[cfg(not(target_os = "android"))]
fn pick_music_folder() -> Option<PathBuf> {
    rfd::FileDialog::new().set_title("Choose a folder with your music").pick_folder()
}

/// On a phone there is no folder picker: look in the usual music folders.
#[cfg(target_os = "android")]
fn pick_music_folder() -> Option<PathBuf> {
    ["/storage/emulated/0/Music", "/storage/emulated/0/Download", "/sdcard/Music"]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_dir())
}

#[derive(Clone, Debug, PartialEq)]
pub enum View {
    Home,
    Search,
    Library,
    Liked,
    Playlist(usize),
    Album(String),
    Mix(usize),
    Local,
    Queue,
    Account,
}

#[derive(Clone, Copy, PartialEq)]
enum Repeat {
    Off,
    All,
    One,
}

#[derive(Clone, Copy, PartialEq)]
enum LibFilter {
    All,
    Playlists,
    Albums,
    Local,
}

#[derive(Clone)]
pub enum Art {
    Cover(f32, String),
    /// Real cover art that has been downloaded.
    Tex(egui::TextureId),
    Liked,
    Local,
}

#[derive(Clone, Copy, PartialEq)]
enum TableKind {
    Plain,
    Playlist(usize),
}

enum NetMsg {
    Search(String, Result<Vec<OnlineTrack>, String>),
    Popular(Result<Vec<OnlineTrack>, String>),
    KeyTest(String, Result<Vec<OnlineTrack>, String>),
    Downloaded(String, Result<(), String>),
    Catalog(String, Result<Vec<OnlineTrack>, String>),
    Hits(Result<Vec<OnlineTrack>, String>),

    Cover(String, Result<egui::ColorImage, String>),
    FullTool(Result<(), String>),
}

struct Mix {
    name: String,
    desc: String,
    hue: f32,
    ids: Vec<String>,
}

struct CardItem {
    title: String,
    subtitle: String,
    art: Art,
    view: View,
}

enum Action {
    PlayList(Vec<String>, usize),
    TogglePlay,
    Next,
    Prev,
    Seek(f32),
    Volume(f32),
    ToggleLike(String),
    AddToPlaylist(String, usize),
    NewPlaylist(Option<String>),
    RemoveFromPlaylist(usize, usize),
    DeletePlaylist(usize),
    EditPlaylist(usize),
    Queue(String),
    Go(View),
    Back,
    Forward,
    ToggleShuffle,
    CycleRepeat,
    AddFolder,
    RescanFolder(PathBuf),
    RemoveFolder(usize),
    SaveKey(String),
    GetFullSongs,
}

pub struct WaveletApp {
    data_dir: PathBuf,
    ephemeral: bool,
    accounts: AccountStore,
    user: Option<Account>,
    data: UserData,

    tracks: Vec<Track>,
    index: HashMap<String, usize>,
    albums: Vec<AlbumInfo>,
    mixes: Vec<Mix>,

    view: View,
    back: Vec<View>,
    forward: Vec<View>,
    search: String,
    lib_filter: LibFilter,

    player: Player,
    current: Option<String>,
    context: Vec<String>,
    ctx_pos: usize,
    queue: VecDeque<String>,
    shuffle: bool,
    repeat: Repeat,
    playing: bool,
    seek_drag: Option<f32>,

    pending: Vec<Action>,
    toast: Option<(String, f64)>,
    scan_rx: Option<Receiver<(PathBuf, Vec<LocalTrack>)>>,
    editing: Option<(usize, String, String)>,
    acct_name: String,
    acct_msg: Option<(String, bool)>,
    focus_search: bool,
    /// When set, playing a row in the next song table plays this whole list.
    table_ctx: Option<Vec<String>>,

    net_tx: Sender<NetMsg>,
    net_rx: Receiver<NetMsg>,
    online_tracks: HashMap<String, Track>,
    online_meta: HashMap<String, OnlineTrack>,
    online_results: Vec<String>,
    online_sent: String,
    online_busy: bool,
    online_err: Option<String>,
    search_seen: String,
    search_changed: f64,
    popular: Vec<String>,
    popular_state: u8,
    popular_err: Option<String>,
    downloading: Option<String>,
    /// Internet songs downloading (they start playing before the download ends).
    streams: HashMap<String, std::sync::Arc<online::StreamState>>,
    /// Turning on full-length songs: the helper download and its message.
    full_tool_busy: bool,
    full_tool_msg: Option<(String, bool)>,
    keyboard_shown: bool,
    saw_text_input: bool,
    key_buf: String,
    key_msg: Option<(String, bool)>,
    key_testing: bool,
    covers: HashMap<String, Option<egui::TextureHandle>>,
    cat_results: Vec<String>,
    cat_sent: String,
    cat_busy: bool,
    cat_err: Option<String>,
    hits: Vec<String>,
    hits_state: u8,
    hits_err: Option<String>,
    /// When each online request started, so a hung one can't leave the screen spinning forever.
    cat_since: f64,
    hits_since: f64,

    screenshot: Option<PathBuf>,
    frames: u32,
    shot_requested: bool,
}

impl WaveletApp {
    pub fn new(screenshot: Option<PathBuf>) -> Self {
        let data_dir = DATA_DIR.get().cloned().unwrap_or_else(|| dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("Wavelet"));
        // Helper programs (the full-song fetcher) live in the app's own folder.
        online::set_tools_dir(data_dir.join("bin"));
        let ephemeral = screenshot.is_some();
        let accounts = if ephemeral { AccountStore::default() } else { AccountStore::load(&data_dir) };
        let (net_tx, net_rx) = channel();
        let mut app = Self {
            data_dir,
            ephemeral,
            accounts,
            user: None,
            data: UserData::default(),
            tracks: vec![],
            index: HashMap::new(),
            albums: vec![],
            mixes: vec![],
            view: View::Home,
            back: vec![],
            forward: vec![],
            search: String::new(),
            lib_filter: LibFilter::All,
            player: Player::new(!ephemeral),
            current: None,
            context: vec![],
            ctx_pos: 0,
            queue: VecDeque::new(),
            shuffle: false,
            repeat: Repeat::Off,
            playing: false,
            seek_drag: None,
            pending: vec![],
            toast: None,
            scan_rx: None,
            editing: None,
            acct_name: String::new(),
            acct_msg: None,
            focus_search: false,
            table_ctx: None,
            net_tx,
            net_rx,
            online_tracks: HashMap::new(),
            online_meta: HashMap::new(),
            online_results: vec![],
            online_sent: String::new(),
            online_busy: false,
            online_err: None,
            search_seen: String::new(),
            search_changed: 0.0,
            popular: vec![],
            popular_state: 0,
            popular_err: None,
            downloading: None,
            streams: HashMap::new(),
            full_tool_busy: false,
            full_tool_msg: None,
            keyboard_shown: false,
            saw_text_input: false,
            key_buf: String::new(),
            key_msg: None,
            key_testing: false,
            covers: HashMap::new(),
            cat_results: vec![],
            cat_sent: String::new(),
            cat_busy: false,
            cat_err: None,
            hits: vec![],
            hits_state: 0,
            hits_err: None,
            cat_since: 0.0,
            hits_since: 0.0,
            screenshot,
            frames: 0,
            shot_requested: false,
        };
        app.rebuild_index();
        if app.ephemeral {
            app.demo_session();
        } else {
            // No sign-in: open the listener's profile straight away.
            let acc = app.accounts.profile();
            app.save_accounts();
            app.enter(acc);
            if !app.player.has_device() {
                app.show_toast("No speakers or headphones found, so there will be no sound.");
            }
        }
        app
    }

    // ---------------------------------------------------------------- data

    fn rebuild_index(&mut self) {
        let mut tracks = library::demo_catalog();
        for l in &self.data.local {
            tracks.push(library::local_to_track(l));
        }
        self.index = tracks.iter().enumerate().map(|(i, t)| (t.id.clone(), i)).collect();
        let mut albums: Vec<AlbumInfo> = vec![];
        for t in &tracks {
            let local = matches!(t.source, library::TrackSource::File(_));
            if let Some(a) = albums.iter_mut().find(|a| a.name == t.album && a.local == local) {
                a.tracks.push(t.id.clone());
            } else {
                albums.push(AlbumInfo { name: t.album.clone(), artist: t.artist.clone(), hue: t.hue, tracks: vec![t.id.clone()], local });
            }
        }
        let defs: [(&str, &[&str], f32); 3] = [
            ("Daily Mix 1", &["Electronic", "Dance"], 0.8),
            ("Daily Mix 2", &["Chill", "Lo-fi"], 0.45),
            ("Calm Mix", &["Ambient", "Lo-fi", "Chill"], 0.06),
        ];
        let mut mixes = vec![];
        for (mi, (name, genres, hue)) in defs.iter().enumerate() {
            let mut picked: Vec<&Track> = tracks.iter().filter(|t| genres.contains(&t.genre.as_str())).collect();
            let mut rng = fastrand::Rng::with_seed(mi as u64 + 11);
            rng.shuffle(&mut picked);
            let mut artists: Vec<String> = vec![];
            for t in &picked {
                if !artists.contains(&t.artist) {
                    artists.push(t.artist.clone());
                }
            }
            let desc = format!("{} and more", artists.iter().take(2).cloned().collect::<Vec<_>>().join(", "));
            mixes.push(Mix { name: name.to_string(), desc, hue: *hue, ids: picked.iter().map(|t| t.id.clone()).collect() });
        }
        self.tracks = tracks;
        self.albums = albums;
        self.mixes = mixes;
    }

    fn track(&self, id: &str) -> Option<&Track> {
        self.index.get(id).map(|&i| &self.tracks[i]).or_else(|| self.online_tracks.get(id))
    }

    fn cache_dir(&self) -> PathBuf {
        if self.ephemeral {
            std::env::temp_dir().join("wavelet-cache")
        } else {
            self.data_dir.join("cache")
        }
    }

    /// Make internet songs known to the app; returns their ids.
    fn add_online(&mut self, list: &[OnlineTrack]) -> Vec<String> {
        let dir = self.cache_dir();
        list.iter()
            .map(|o| {
                let id = o.track_id();
                self.online_tracks.insert(id.clone(), library::online_to_track(o, &dir));
                self.online_meta.insert(id.clone(), o.clone());
                id
            })
            .collect()
    }

    /// Full-length songs just became available: forget the 30-second clips, rebuild the
    /// internet songs so they show their real length, and restart the current song.
    fn enable_full_songs(&mut self) {
        online::refresh_tools();
        let dir = self.cache_dir();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().starts_with("dz_") {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        let metas: Vec<OnlineTrack> = self.online_meta.values().cloned().collect();
        self.streams.clear();
        self.downloading = None;
        self.add_online(&metas);
        let chart_song = self.current.clone().map_or(false, |c| self.online_meta.get(&c).map_or(false, |o| o.is_catalogue()));
        self.show_toast("Full-length songs are on");
        if chart_song {
            if let Some(cur) = self.current.clone() {
                self.play_id(&cur);
            }
        }
    }

    /// Keep an internet song in the saved profile (so likes and playlists survive restarts).
    fn remember(&mut self, id: &str) {
        if let Some(o) = self.online_meta.get(id) {
            if !self.data.online.iter().any(|x| x.id == o.id) {
                self.data.online.push(o.clone());
            }
        }
    }

    fn spawn_net(&self, f: impl FnOnce() -> NetMsg + Send + 'static) {
        let tx = self.net_tx.clone();
        std::thread::spawn(move || {
            // A damaged reply must never close the app (the result is simply dropped).
            if let Ok(msg) = crate::safe::catch(f) {
                let _ = tx.send(msg);
            }
        });
    }

    /// Cover picture for a song: real cover art when it has one (downloaded in the background).
    fn art_for(&mut self, t: &Track) -> Art {
        if let Some(url) = &t.cover {
            match self.covers.get(url) {
                Some(Some(tex)) => return Art::Tex(tex.id()),
                Some(None) => {}
                None => {
                    self.covers.insert(url.clone(), None);
                    let u = url.clone();
                    self.spawn_net(move || {
                        // Some cover pictures are damaged; reading them must not close the app.
                        let r = online::http_get(&u).and_then(|b| {
                            crate::safe::catch_res(|| {
                                let img = image::load_from_memory(&b).map_err(|e| e.to_string())?;
                                // Covers are shown small: shrink big pictures (saves memory, phones limit picture size).
                                let img = if img.width() > 300 || img.height() > 300 { img.thumbnail(300, 300) } else { img };
                                let rgba = img.to_rgba8();
                                if rgba.width() == 0 || rgba.height() == 0 {
                                    return Err("empty picture".to_string());
                                }
                                Ok(egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
                            })
                        });
                        NetMsg::Cover(u, r)
                    });
                }
            }
        }
        Art::Cover(t.hue, t.album.clone())
    }

    fn has_key(&self) -> bool {
        !self.data.jamendo_key.trim().is_empty()
    }

    /// A music key supplied without typing it in: the WAVELET_MUSIC_KEY variable, or a
    /// music-key.txt file in the app's data folder. Empty when there is none.
    fn music_key_from_outside(&self) -> String {
        if let Ok(k) = std::env::var("WAVELET_MUSIC_KEY") {
            let k = k.trim();
            if !k.is_empty() {
                return k.to_string();
            }
        }
        std::fs::read_to_string(self.data_dir.join("music-key.txt")).map(|s| s.trim().to_string()).unwrap_or_default()
    }

    fn save(&self) {
        if self.ephemeral {
            return;
        }
        if let Some(u) = &self.user {
            self.data.save(&self.data_dir, &u.username);
        }
    }

    fn save_accounts(&self) {
        if !self.ephemeral {
            self.accounts.save(&self.data_dir);
        }
    }

    fn enter(&mut self, acc: Account) {
        self.data = if self.ephemeral { UserData::default() } else { UserData::load(&self.data_dir, &acc.username) };
        if !self.ephemeral && self.data.jamendo_key.trim().is_empty() {
            self.data.jamendo_key = self.music_key_from_outside();
        }
        self.shuffle = self.data.shuffle;
        self.player.set_volume(self.data.volume);
        self.acct_name = acc.display_name.clone();
        self.user = Some(acc);
        self.view = View::Home;
        self.back.clear();
        self.forward.clear();
        self.rebuild_index();
        self.online_tracks.clear();
        self.online_meta.clear();
        let saved = self.data.online.clone();
        self.add_online(&saved);
        self.key_buf = self.data.jamendo_key.clone();
        self.popular_state = 0;
        self.popular.clear();
    }

    fn demo_session(&mut self) {
        let acc = Account { username: "alex".into(), display_name: "Alex Rivera".into(), created: 1_760_000_000 };
        self.enter(acc);
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        self.data.liked = ids(&["demo:0:0", "demo:2:1", "demo:1:2", "demo:4:0", "demo:3:1"]);
        self.data.playlists = vec![
            Playlist { name: "Road Trip".into(), description: "Windows down, volume up.".into(), tracks: ids(&["demo:0:0", "demo:4:0", "demo:0:1", "demo:4:2", "demo:1:1", "demo:0:2"]), hue: 0.95 },
            Playlist { name: "Focus Flow".into(), description: "Calm beats for deep work.".into(), tracks: ids(&["demo:2:0", "demo:2:2", "demo:3:0", "demo:1:0"]), hue: 0.5 },
            Playlist { name: "Late Night".into(), description: String::new(), tracks: ids(&["demo:3:2", "demo:0:1", "demo:2:1", "demo:3:1", "demo:4:1"]), hue: 0.68 },
        ];
        self.data.recent = ids(&["demo:0:0", "demo:2:1", "demo:3:0", "demo:1:2"]);
        self.context = self.data.playlists[0].tracks.clone();
        self.ctx_pos = 0;
        if let Some(t) = self.track("demo:0:0").cloned() {
            let _ = self.player.load(&t);
            self.player.seek(47.0);
            self.current = Some(t.id.clone());
        }
        // Optional: pick which screen the screenshot shows (used for testing).
        match std::env::var("WAVELET_SHOT_VIEW").unwrap_or_default().as_str() {
            "playlist" => self.view = View::Playlist(0),
            "album" => self.view = View::Album("Low Tide".into()),
            "search" => {
                self.view = View::Search;
                self.search = std::env::var("WAVELET_SHOT_SEARCH").unwrap_or_else(|_| "night".into());
            }
            "browse" => self.view = View::Search,
            "library" => self.view = View::Library,
            "account" => self.view = View::Account,
            "queue" => self.view = View::Queue,
            _ => {}
        }
    }

    // ---------------------------------------------------------------- playback
    /// Download the next internet song in the list ahead of time, so songs follow each other without a wait.
    fn prefetch_next(&mut self) {
        let next = self.queue.front().cloned().or_else(|| self.context.get(self.ctx_pos + 1).cloned());
        let Some(id) = next else { return };
        let Some(t) = self.track(&id).cloned() else { return };
        if let library::TrackSource::Online { url, cache } = t.source {
            if !cache.exists() {
                self.stream_for(&id, &url, &cache);
            }
        }
    }

    /// The download for an internet song, starting it if needed.
    fn stream_for(&mut self, id: &str, url: &str, cache: &std::path::Path) -> &std::sync::Arc<online::StreamState> {
        // Drop finished or failed downloads of other songs (keeps memory and temp files small).
        let current = self.current.clone();
        self.streams.retain(|k, st| {
            let keep = Some(k.as_str()) == current.as_deref() || k == id || !st.done.load(std::sync::atomic::Ordering::SeqCst);
            if !keep {
                let _ = std::fs::remove_file(&st.part);
            }
            keep
        });
        let dz = id.strip_prefix("dz:").map(|s| s.to_string());
        // Chart songs play in full when the helper is there.
        let full = self.online_meta.get(id).filter(|o| o.is_catalogue()).map(|o| format!("{} {} official audio", o.title, o.artist));
        self.streams
            .entry(id.to_string())
            .or_insert_with(|| online::start_stream(url.to_string(), dz, full, cache.to_path_buf()))
    }

    fn start(&mut self, ids: Vec<String>, pos: usize) {
        if ids.is_empty() {
            return;
        }
        let pos = if self.shuffle && pos == 0 && ids.len() > 1 { fastrand::usize(..ids.len()) } else { pos.min(ids.len() - 1) };
        self.context = ids;
        self.ctx_pos = pos;
        let id = self.context[pos].clone();
        self.play_id(&id);
    }

    fn next(&mut self, auto: bool) {
        if auto && self.repeat == Repeat::One {
            if let Some(id) = self.current.clone() {
                self.play_id(&id);
            }
            return;
        }
        if let Some(id) = self.queue.pop_front() {
            self.play_id(&id);
            return;
        }
        let n = self.context.len();
        if n == 0 {
            self.player.stop();
            self.playing = false;
            return;
        }
        let np = if self.shuffle && n > 1 {
            loop {
                let r = fastrand::usize(..n);
                if r != self.ctx_pos {
                    break r;
                }
            }
        } else if self.ctx_pos + 1 < n {
            self.ctx_pos + 1
        } else if self.repeat == Repeat::All || !auto {
            0
        } else {
            // End of the list: rewind to the first song and stop.
            self.ctx_pos = 0;
            let id = self.context[0].clone();
            self.play_id(&id);
            self.player.pause();
            self.playing = false;
            return;
        };
        self.ctx_pos = np;
        let id = self.context[np].clone();
        self.play_id(&id);
    }

    fn play_id(&mut self, id: &str) {
        let Some(t) = self.track(id).cloned() else { return };
        self.current = Some(t.id.clone());
        let mut stream = None;
        if let library::TrackSource::Online { url, cache } = &t.source {
            if !cache.exists() {
                // Start (or keep) the download; play as soon as the first part has arrived.
                if self.streams.get(id).map_or(false, |st| st.failed().is_some()) {
                    self.streams.remove(id); // try the download again
                }
                let st = self.stream_for(id, url, cache).clone();
                if let Some(e) = st.failed() {
                    self.streams.remove(id);
                    self.downloading = None;
                    self.player.stop();
                    self.playing = false;
                    self.show_toast(e);
                    return;
                }
                if !st.ready() {
                    if self.downloading.as_deref() != Some(id) {
                        self.show_toast(format!("Loading \"{}\"…", t.title));
                    }
                    self.downloading = Some(id.to_string());
                    self.player.stop();
                    self.playing = false;
                    return;
                }
                stream = Some(st);
            }
            self.downloading = None;
            self.remember(id);
        }
        let loaded = match &stream {
            Some(st) => self.player.load_stream(&t, st),
            None => self.player.load(&t),
        };
        match loaded {
            Ok(()) => {
                self.player.play();
                self.playing = true;
                self.prefetch_next();
                self.data.recent.retain(|r| r != &t.id);
                self.data.recent.insert(0, t.id.clone());
                self.data.recent.truncate(30);
                self.save();
            }
            Err(e) => {
                self.playing = false;
                self.show_toast(e);
            }
        }
    }

    fn prev(&mut self) {
        if self.player.position() > 3.0 || self.context.is_empty() {
            self.player.seek(0.0);
            return;
        }
        let n = self.context.len();
        self.ctx_pos = if self.ctx_pos == 0 {
            if self.repeat == Repeat::All {
                n - 1
            } else {
                0
            }
        } else {
            self.ctx_pos - 1
        };
        let id = self.context[self.ctx_pos].clone();
        self.play_id(&id);
    }

    fn toggle_play(&mut self) {
        if self.current.is_none() {
            let ids = if !self.data.liked.is_empty() { self.data.liked.clone() } else { self.mixes[0].ids.clone() };
            self.start(ids, 0);
            return;
        }
        if self.playing {
            self.player.pause();
            self.playing = false;
        } else if !self.player.loaded() {
            if let Some(id) = self.current.clone() {
                self.play_id(&id);
            }
        } else {
            self.player.play();
            self.playing = true;
        }
    }

    fn show_toast(&mut self, s: impl Into<String>) {
        self.toast = Some((s.into(), -1.0));
    }

    fn ids_for(&self, v: &View) -> Vec<String> {
        match v {
            View::Liked => self.data.liked.clone(),
            View::Playlist(i) => self.data.playlists.get(*i).map(|p| p.tracks.clone()).unwrap_or_default(),
            View::Album(a) => self.albums.iter().find(|x| &x.name == a).map(|x| x.tracks.clone()).unwrap_or_default(),
            View::Mix(i) => self.mixes.get(*i).map(|m| m.ids.clone()).unwrap_or_default(),
            View::Local => self.data.local.iter().map(|l| library::local_to_track(l).id).collect(),
            _ => vec![],
        }
    }

    fn go(&mut self, v: View) {
        if v != self.view {
            let old = std::mem::replace(&mut self.view, v);
            self.back.push(old);
            self.forward.clear();
            if self.view == View::Search {
                self.focus_search = true;
            }
        }
    }

    /// Draw the current page; if something in it can't be shown, say so instead of closing the app.
    fn view_content_safe(&mut self, ui: &mut Ui) {
        if let Err(e) = crate::safe::catch(|| self.view_content(ui)) {
            ui.add_space(40.0);
            ui.label(RichText::new("Part of this page couldn't be shown").font(bold(20.0)));
            ui.label(RichText::new(format!("Details: {e}")).color(SUBTLE));
            ui.add_space(8.0);
            if ui.button("Go back").clicked() {
                self.pending.push(Action::Back);
            }
        }
    }

    fn spawn_scan(&mut self, dir: PathBuf) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let r = crate::safe::catch(|| library::scan_folder(&dir)).unwrap_or_default();
            let _ = tx.send((dir, r));
        });
        self.scan_rx = Some(rx);
        self.show_toast("Looking for music…");
    }

    fn apply(&mut self, a: Action) {
        match a {
            Action::PlayList(ids, pos) => self.start(ids, pos),
            Action::TogglePlay => self.toggle_play(),
            Action::Next => self.next(false),
            Action::Prev => self.prev(),
            Action::Seek(f) => {
                let d = self.player.duration();
                self.player.seek(f.clamp(0.0, 1.0) * d);
            }
            Action::Volume(v) => {
                self.player.set_volume(v);
                self.data.volume = v;
                self.save();
            }
            Action::ToggleLike(id) => {
                if self.data.liked.contains(&id) {
                    self.data.liked.retain(|x| x != &id);
                    self.show_toast("Removed from Liked Songs");
                } else {
                    self.remember(&id);
                    self.data.liked.insert(0, id);
                    self.show_toast("Added to Liked Songs");
                }
                self.save();
            }
            Action::AddToPlaylist(id, p) => {
                self.remember(&id);
                if let Some(pl) = self.data.playlists.get_mut(p) {
                    let name = pl.name.clone();
                    if pl.tracks.contains(&id) {
                        self.show_toast(format!("Already in {name}"));
                    } else {
                        pl.tracks.push(id);
                        self.show_toast(format!("Added to {name}"));
                    }
                }
                self.save();
            }
            Action::NewPlaylist(first) => {
                let n = self.data.playlists.len() + 1;
                let mut pl = Playlist { name: format!("My Playlist #{n}"), description: String::new(), tracks: vec![], hue: fastrand::f32() };
                if let Some(id) = first {
                    self.remember(&id);
                    pl.tracks.push(id);
                }
                self.data.playlists.push(pl);
                let idx = self.data.playlists.len() - 1;
                self.save();
                self.go(View::Playlist(idx));
                self.show_toast("Playlist created");
            }
            Action::RemoveFromPlaylist(p, row) => {
                if let Some(pl) = self.data.playlists.get_mut(p) {
                    if row < pl.tracks.len() {
                        pl.tracks.remove(row);
                        let name = pl.name.clone();
                        self.show_toast(format!("Removed from {name}"));
                    }
                }
                self.save();
            }
            Action::DeletePlaylist(p) => {
                if p < self.data.playlists.len() {
                    let pl = self.data.playlists.remove(p);
                    self.back.clear();
                    self.forward.clear();
                    self.view = View::Library;
                    self.show_toast(format!("Deleted {}", pl.name));
                    self.save();
                }
            }
            Action::EditPlaylist(p) => {
                if let Some(pl) = self.data.playlists.get(p) {
                    self.editing = Some((p, pl.name.clone(), pl.description.clone()));
                }
            }
            Action::Queue(id) => {
                self.queue.push_back(id);
                self.show_toast("Added to queue");
            }
            Action::Go(v) => self.go(v),
            Action::Back => {
                if let Some(v) = self.back.pop() {
                    let old = std::mem::replace(&mut self.view, v);
                    self.forward.push(old);
                }
            }
            Action::Forward => {
                if let Some(v) = self.forward.pop() {
                    let old = std::mem::replace(&mut self.view, v);
                    self.back.push(old);
                }
            }
            Action::ToggleShuffle => {
                self.shuffle = !self.shuffle;
                self.data.shuffle = self.shuffle;
                self.save();
            }
            Action::CycleRepeat => {
                self.repeat = match self.repeat {
                    Repeat::Off => Repeat::All,
                    Repeat::All => Repeat::One,
                    Repeat::One => Repeat::Off,
                };
            }
            Action::AddFolder => {
                if self.scan_rx.is_none() {
                    if let Some(dir) = pick_music_folder() {
                        self.spawn_scan(dir);
                    }
                }
            }
            Action::RescanFolder(dir) => {
                if self.scan_rx.is_none() {
                    self.spawn_scan(dir);
                }
            }
            Action::SaveKey(key) => {
                let key = key.trim().to_string();
                if key.is_empty() {
                    self.data.jamendo_key.clear();
                    self.save();
                    self.popular.clear();
                    self.online_results.clear();
                    self.key_msg = Some(("Real music is turned off.".into(), true));
                } else if !self.key_testing {
                    self.key_testing = true;
                    self.key_msg = None;
                    self.spawn_net(move || {
                        let r = crate::safe::catch_res(|| online::popular(&key, 1));
                        NetMsg::KeyTest(key, r)
                    });
                }
            }
            Action::GetFullSongs => {
                if !self.full_tool_busy {
                    self.full_tool_busy = true;
                    self.full_tool_msg = Some(("Downloading full-song support…".into(), true));
                    self.spawn_net(|| NetMsg::FullTool(crate::safe::catch_res(online::install_full_support).map(|_| ())));
                }
            }
            Action::RemoveFolder(i) => {
                if i < self.data.folders.len() {
                    let f = self.data.folders.remove(i);
                    self.data.local.retain(|l| !l.path.starts_with(&f));
                    self.rebuild_index();
                    self.save();
                    self.show_toast("Folder removed");
                }
            }
        }
    }

    fn poll_net(&mut self, ctx: &Context) {
        while let Ok(msg) = self.net_rx.try_recv() {
            match msg {
                NetMsg::Cover(url, res) => {
                    if let Ok(img) = res {
                        let tex = ctx.load_texture(url.clone(), img, egui::TextureOptions::LINEAR);
                        self.covers.insert(url, Some(tex));
                    }
                }
                NetMsg::Catalog(q, res) => {
                    if q != self.cat_sent {
                        continue;
                    }
                    self.cat_busy = false;
                    self.cat_since = 0.0;
                    match res {
                        Ok(list) => {
                            self.cat_results = self.add_online(&list);
                            self.cat_err = None;
                        }
                        Err(e) => {
                            self.cat_results.clear();
                            self.cat_err = Some(e);
                        }
                    }
                }
                NetMsg::Hits(res) => {
                    self.hits_state = 2;
                    self.hits_since = 0.0;
                    match res {
                        Ok(list) => {
                            self.hits = self.add_online(&list);
                            self.hits_err = None;
                        }
                        Err(e) => self.hits_err = Some(e),
                    }
                }
                NetMsg::Search(q, res) => {
                    if q != self.online_sent {
                        continue;
                    }
                    self.online_busy = false;
                    match res {
                        Ok(list) => {
                            self.online_results = self.add_online(&list);
                            self.online_err = None;
                        }
                        Err(e) => {
                            self.online_results.clear();
                            self.online_err = Some(e);
                        }
                    }
                }
                NetMsg::Popular(res) => {
                    self.popular_state = 2;
                    match res {
                        Ok(list) => {
                            self.popular = self.add_online(&list);
                            self.popular_err = None;
                        }
                        Err(e) => self.popular_err = Some(e),
                    }
                }
                NetMsg::KeyTest(key, res) => {
                    self.key_testing = false;
                    match res {
                        Ok(_) => {
                            self.data.jamendo_key = key.clone();
                            self.key_buf = key;
                            self.save();
                            self.popular_state = 0;
                            self.online_sent.clear();
                            self.key_msg = Some(("Connected! Real music is on. Try Search or the Home page.".into(), true));
                        }
                        Err(e) => self.key_msg = Some((e, false)),
                    }
                }
                NetMsg::Downloaded(id, res) => {
                    if self.downloading.as_deref() == Some(id.as_str()) {
                        self.downloading = None;
                    }
                    let is_current = self.current.as_deref() == Some(id.as_str());
                    match res {
                        Ok(()) if is_current => self.play_id(&id),
                        Ok(()) => {}
                        Err(e) if is_current => self.show_toast(e),
                        Err(_) => {}
                    }
                }
                NetMsg::FullTool(res) => {
                    self.full_tool_busy = false;
                    match res {
                        Ok(()) => {
                            self.full_tool_msg = Some(("Full-length songs are on.".into(), true));
                            self.enable_full_songs();
                        }
                        Err(e) => self.full_tool_msg = Some((e, false)),
                    }
                }
            }
        }
    }

    fn poll_scan(&mut self) {
        let Some(rx) = &self.scan_rx else { return };
        if let Ok((dir, found)) = rx.try_recv() {
            self.scan_rx = None;
            let n = found.len();
            if n == 0 {
                self.show_toast("No music files found in that folder");
                return;
            }
            self.data.local.retain(|l| !l.path.starts_with(&dir));
            self.data.local.extend(found);
            if !self.data.folders.contains(&dir) {
                self.data.folders.push(dir);
            }
            self.rebuild_index();
            self.save();
            self.show_toast(format!("Added {n} songs from your computer"));
        }
    }

    // ---------------------------------------------------------------- frame

    fn tick(&mut self, ctx: &Context) {
        if self.playing && self.player.finished() {
            self.next(true);
        }
        // A song the listener picked is downloading: start it once enough has arrived.
        if let Some(id) = self.downloading.clone() {
            if self.current.as_deref() != Some(id.as_str()) {
                self.downloading = None;
            } else if let Some(st) = self.streams.get(&id).cloned() {
                if let Some(e) = st.failed() {
                    self.streams.remove(&id);
                    self.downloading = None;
                    self.show_toast(e);
                } else if st.ready() {
                    self.play_id(&id);
                }
            } else {
                self.downloading = None;
            }
        }
        // Phones: open the on-screen keyboard while a text box (like Search) is in use.
        let wants = ctx.wants_keyboard_input();
        if wants != self.keyboard_shown {
            self.keyboard_shown = wants;
            show_keyboard(wants);
        }
        if self.playing || self.scan_rx.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        // Real music: search as you type (after a short pause) and load "popular" for Home.
        let now = ctx.input(|i| i.time);
        if self.search != self.search_seen {
            self.search_seen = self.search.clone();
            self.search_changed = now;
        }
        // Every-song catalogue (no key needed).
        {
            let q = self.search.trim().to_string();
            if self.view == View::Search && !q.is_empty() && q != self.cat_sent && now - self.search_changed > 0.5 {
                self.cat_sent = q.clone();
                self.cat_busy = true;
                self.cat_since = now;
                let q1 = q.clone();
                self.spawn_net(move || {
                    let r = crate::safe::catch_res(|| online::catalog_search(&q1, 25));
                    NetMsg::Catalog(q1, r)
                });
            }
            // Home shows the top hits, and so does Search before anything is typed.
            let browsing = self.view == View::Home || (self.view == View::Search && q.is_empty());
            if self.hits_state == 0 && browsing {
                self.hits_state = 1;
                self.hits_since = now;
                self.spawn_net(|| NetMsg::Hits(crate::safe::catch_res(|| online::top_hits(20))));
            }
            // A blocked or very slow connection must never leave a section spinning forever.
            const GIVE_UP: f64 = 20.0;
            let stuck = |start: f64| start > 0.0 && now - start > GIVE_UP;
            if stuck(self.cat_since) {
                self.cat_busy = false;
                self.cat_err = Some(online::OFFLINE_MSG.into());
                self.cat_since = 0.0;
            }
            if stuck(self.hits_since) {
                self.hits_state = 2;
                self.hits_err = Some(online::OFFLINE_MSG.into());
                self.hits_since = 0.0;
            }
            if self.cat_busy || self.hits_state == 1 || self.covers.values().any(|c| c.is_none()) {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            }
        }
        if self.has_key() {
            let q = self.search.trim().to_string();
            if self.view == View::Search && !q.is_empty() && q != self.online_sent && now - self.search_changed > 0.6 {
                self.online_sent = q.clone();
                self.online_busy = true;
                let key = self.data.jamendo_key.clone();
                self.spawn_net(move || {
                    let r = crate::safe::catch_res(|| online::search(&key, &q, 30));
                    NetMsg::Search(q, r)
                });
            }
            if self.popular_state == 0 && self.view == View::Home {
                self.popular_state = 1;
                let key = self.data.jamendo_key.clone();
                self.spawn_net(move || NetMsg::Popular(crate::safe::catch_res(|| online::popular(&key, 12))));
            }
        }
        if self.online_busy || self.popular_state == 1 || self.downloading.is_some() || self.key_testing || (self.view == View::Search && self.search.trim() != self.online_sent) {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
        if self.user.is_some() && self.editing.is_none() && !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(Key::Space)) {
            self.toggle_play();
        }
    }

    fn handle_screenshot(&mut self, ctx: &Context) {
        let Some(path) = self.screenshot.clone() else { return };
        self.frames += 1;
        ctx.request_repaint();
        // Only take the picture while the window is actually visible (a minimised
        // window has no pixels to read).
        let (minimized, size) = ctx.input(|i| (i.viewport().minimized.unwrap_or(false), i.viewport().inner_rect.map(|r| r.size())));
        let visible = !minimized && size.map(|s| s.x >= 1.0 && s.y >= 1.0).unwrap_or(true);
        if !visible && self.frames % 20 == 0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        // Give online songs and cover pictures a moment to arrive (but never wait forever).
        let net_busy = self.hits_state == 1 || self.cat_busy || (self.view == View::Search && !self.search.trim().is_empty() && self.search.trim() != self.cat_sent) || self.covers.values().any(|c| c.is_none());
        let waited = ctx.input(|i| i.time);
        if self.frames >= 120 && (!net_busy || waited > 12.0) && !self.shot_requested && visible {
            self.shot_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            let [w, h] = img.size;
            let mut bytes = Vec::with_capacity(w * h * 4);
            for c in &img.pixels {
                bytes.extend_from_slice(&[c.r(), c.g(), c.b(), 255]);
            }
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    let _ = std::fs::create_dir_all(parent);
                }
            }
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                if let Err(e) = buf.save(&path) {
                    eprintln!("failed to save screenshot: {e}");
                }
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ctx.input(|i| i.time) > 40.0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    // ---------------------------------------------------------------- main screen

    fn main_ui(&mut self, ctx: &Context) {
        if is_phone(ctx) {
            self.phone_ui(ctx);
            self.edit_window(ctx);
            self.toast_ui(ctx);
            return;
        }
        egui::TopBottomPanel::bottom("player")
            .exact_height(92.0)
            .frame(Frame::new().fill(BG).inner_margin(Margin::symmetric(16, 8)))
            .show_separator_line(false)
            .show(ctx, |ui| self.player_bar(ui));

        egui::SidePanel::left("side")
            .exact_width(268.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(BG).inner_margin(Margin { left: 8, right: 8, top: 8, bottom: 0 }))
            .show(ctx, |ui| self.sidebar(ui));

        egui::CentralPanel::default()
            .frame(Frame::new().fill(BG).inner_margin(Margin { left: 0, right: 8, top: 8, bottom: 0 }))
            .show(ctx, |ui| {
                let card = ui.max_rect();
                let p = ui.painter().clone();
                p.rect_filled(card, cr(10), PANEL);
                let (hue, sat) = self.view_tint();
                rounded_top_gradient(&p, Rect::from_min_size(card.min, vec2(card.width(), 340.0f32.min(card.height()))), 10.0, hsv(hue, sat, 0.42), PANEL);
                ui.scope_builder(UiBuilder::new().max_rect(card), |ui| {
                    ui.set_clip_rect(card);
                    self.top_bar(ui);
                    let key = format!("{:?}", self.view);
                    ScrollArea::vertical().id_salt(key).auto_shrink([false, false]).show(ui, |ui| {
                        Frame::new().inner_margin(Margin { left: 24, right: 24, top: 0, bottom: 32 }).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            self.view_content_safe(ui);
                        });
                    });
                });
            });

        self.edit_window(ctx);
        self.toast_ui(ctx);
    }

    /// Phone layout: tabs at the bottom, a mini player above them, no sidebar.
    fn phone_ui(&mut self, ctx: &Context) {
        let top_inset: i8 = if cfg!(target_os = "android") { 30 } else { 0 };
        egui::TopBottomPanel::bottom("tabs")
            .exact_height(64.0)
            .frame(Frame::new().fill(BG))
            .show_separator_line(false)
            .show(ctx, |ui| self.tab_bar(ui));
        egui::TopBottomPanel::bottom("mini")
            .exact_height(72.0)
            .frame(Frame::new().fill(BG).inner_margin(Margin { left: 8, right: 8, top: 6, bottom: 2 }))
            .show_separator_line(false)
            .show(ctx, |ui| self.mini_player(ui));
        egui::CentralPanel::default()
            .frame(Frame::new().fill(BG).inner_margin(Margin { left: 0, right: 0, top: top_inset, bottom: 0 }))
            .show(ctx, |ui| {
                let card = ui.max_rect();
                let p = ui.painter().clone();
                p.rect_filled(card, cr(0), PANEL);
                let (hue, sat) = self.view_tint();
                vgradient(&p, Rect::from_min_size(card.min, vec2(card.width(), 300.0f32.min(card.height()))), hsv(hue, sat, 0.42), PANEL);
                ui.scope_builder(UiBuilder::new().max_rect(card), |ui| {
                    ui.set_clip_rect(card);
                    self.top_bar(ui);
                    let key = format!("{:?}", self.view);
                    ScrollArea::vertical().id_salt(key).auto_shrink([false, false]).show(ui, |ui| {
                        Frame::new().inner_margin(Margin { left: 14, right: 14, top: 0, bottom: 24 }).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            self.view_content_safe(ui);
                        });
                    });
                });
            });
    }

    fn tab_bar(&mut self, ui: &mut Ui) {
        let r = ui.max_rect();
        let p = ui.painter().clone();
        p.line_segment([r.left_top(), r.right_top()], Stroke::new(1.0_f32, Color32::from_white_alpha(14)));
        let tabs: [(&str, View); 4] = [("Home", View::Home), ("Search", View::Search), ("Your Library", View::Library), ("Profile", View::Account)];
        let w = r.width() / tabs.len() as f32;
        for (i, (label, v)) in tabs.iter().enumerate() {
            let cell = Rect::from_min_size(pos2(r.left() + i as f32 * w, r.top()), vec2(w, r.height()));
            let resp = ui.interact(cell, Id::new(("tab", i)), Sense::click());
            let sel = match (v, &self.view) {
                (View::Library, View::Liked | View::Playlist(_) | View::Album(_) | View::Mix(_) | View::Local) => true,
                (a, b) => a == b,
            };
            let col = if sel || resp.hovered() { TEXT } else { SUBTLE };
            let c = pos2(cell.center().x, cell.top() + 22.0);
            match i {
                0 => crate::theme::home(&p, c, 20.0, col, sel),
                1 => search(&p, c, 20.0, col, sel),
                2 => library(&p, c, 20.0, col),
                _ => {
                    p.circle_stroke(pos2(c.x, c.y - 4.0), 5.0, Stroke::new(1.8_f32, col));
                    crate::theme::arc(&p, pos2(c.x, c.y + 10.0), 9.0, std::f32::consts::PI, std::f32::consts::TAU, Stroke::new(1.8_f32, col));
                }
            }
            p.text(pos2(c.x, cell.top() + 44.0), Align2::CENTER_CENTER, *label, if sel { bold(11.5) } else { reg(11.5) }, col);
            if resp.clicked() {
                self.pending.push(Action::Go(v.clone()));
            }
        }
    }

    fn mini_player(&mut self, ui: &mut Ui) {
        let r = ui.max_rect();
        let p = ui.painter().clone();
        let Some(t) = self.current.clone().and_then(|id| self.track(&id).cloned()) else {
            p.rect_filled(r, cr(8), CARD);
            p.text(pos2(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, "Pick something to play", reg(14.0), SUBTLE);
            let c = pos2(r.right() - 30.0, r.center().y);
            let resp = ui.interact(Rect::from_center_size(c, vec2(44.0, 44.0)), Id::new("mini_play"), Sense::click());
            play(&p, c, 20.0, TEXT);
            if resp.clicked() {
                self.pending.push(Action::TogglePlay);
            }
            return;
        };
        let body = ui.interact(r, Id::new("mini_body"), Sense::click());
        p.rect_filled(r, cr(8), hsv(t.hue, 0.45, 0.28));
        let cv = Rect::from_min_size(pos2(r.left() + 8.0, r.center().y - 24.0), vec2(48.0, 48.0));
        let art = self.art_for(&t);
        draw_art(&p, cv, &art);
        let tw = r.width() - 64.0 - 110.0;
        fit_text(&p, pos2(cv.right() + 12.0, r.center().y - 1.0), Align2::LEFT_BOTTOM, &t.title, bold(14.0), TEXT, tw);
        fit_text(&p, pos2(cv.right() + 12.0, r.center().y + 2.0), Align2::LEFT_TOP, &t.artist, reg(12.5), SUBTLE, tw);
        let liked = self.data.liked.contains(&t.id);
        let hc = pos2(r.right() - 82.0, r.center().y);
        let hresp = ui.interact(Rect::from_center_size(hc, vec2(40.0, 40.0)), Id::new("mini_like"), Sense::click());
        heart(&p, hc, 18.0, if liked { ACCENT } else { TEXT }, liked);
        if hresp.clicked() {
            self.pending.push(Action::ToggleLike(t.id.clone()));
        }
        let pc = pos2(r.right() - 34.0, r.center().y);
        let presp = ui.interact(Rect::from_center_size(pc, vec2(44.0, 44.0)), Id::new("mini_play"), Sense::click());
        if self.playing {
            pause(&p, pc, 20.0, TEXT);
        } else {
            play(&p, pos2(pc.x + 1.0, pc.y), 20.0, TEXT);
        }
        if presp.clicked() {
            self.pending.push(Action::TogglePlay);
        }
        let dur = self.player.duration();
        let frac = if dur > 0.0 { (self.player.position() / dur).clamp(0.0, 1.0) } else { 0.0 };
        let line = Rect::from_min_size(pos2(r.left() + 8.0, r.bottom() - 3.0), vec2(r.width() - 16.0, 2.0));
        p.rect_filled(line, cr(1), Color32::from_white_alpha(50));
        p.rect_filled(Rect::from_min_size(line.min, vec2(line.width() * frac, 2.0)), cr(1), TEXT);
        if body.clicked() {
            self.pending.push(Action::Go(View::Queue));
        }
    }

    /// Big "now playing" card with full controls (shown on the phone's queue screen).
    pub(super) fn now_playing_card(&mut self, ui: &mut Ui) {
        let Some(t) = self.current.clone().and_then(|id| self.track(&id).cloned()) else { return };
        let w = ui.available_width();
        let side = (w - 20.0).min(340.0);
        let (r, _) = ui.allocate_exact_size(vec2(w, side + 230.0), Sense::hover());
        let p = ui.painter().clone();
        let cv = Rect::from_min_size(pos2(r.center().x - side / 2.0, r.top() + 8.0), vec2(side, side));
        p.rect_filled(cv.translate(vec2(0.0, 8.0)), cr(8), Color32::from_black_alpha(90));
        let art = self.art_for(&t);
        draw_art(&p, cv, &art);
        let x0 = cv.left();
        let y = cv.bottom() + 22.0;
        fit_text(&p, pos2(x0, y), Align2::LEFT_TOP, &t.title, bold(22.0), TEXT, side - 50.0);
        let ar = fit_text(&p, pos2(x0, y + 30.0), Align2::LEFT_TOP, &t.artist, reg(15.0), SUBTLE, side - 50.0);
        if t.preview {
            let lr = p.text(pos2(ar.right() + 10.0, ar.center().y), Align2::LEFT_CENTER, "· Full song on Spotify", bold(13.0), ACCENT);
            if ui.interact(lr.expand(6.0), Id::new("np_spotify"), Sense::click()).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(online::spotify_link(&t.title, &t.artist)));
            }
        }
        let liked = self.data.liked.contains(&t.id);
        let hc = pos2(cv.right() - 14.0, y + 22.0);
        let hresp = ui.interact(Rect::from_center_size(hc, vec2(44.0, 44.0)), Id::new("np_big_like"), Sense::click());
        heart(&p, hc, 22.0, if liked { ACCENT } else { TEXT }, liked);
        if hresp.clicked() {
            self.pending.push(Action::ToggleLike(t.id.clone()));
        }
        let dur = self.player.duration().max(0.0);
        let frac = if dur > 0.0 { (self.player.position() / dur).clamp(0.0, 1.0) } else { 0.0 };
        let shown = self.seek_drag.unwrap_or(frac);
        let bar = Rect::from_min_size(pos2(x0, y + 74.0), vec2(side, 4.0));
        if let Some(ev) = slider_bar(ui, &p, bar, "np_seek", shown, true) {
            match ev {
                SliderEv::Drag(v) => self.seek_drag = Some(v),
                SliderEv::Done(v) => {
                    self.seek_drag = None;
                    self.pending.push(Action::Seek(v));
                }
            }
        }
        p.text(pos2(x0, y + 90.0), Align2::LEFT_TOP, fmt_time(shown * dur), reg(12.0), SUBTLE);
        p.text(pos2(cv.right(), y + 90.0), Align2::RIGHT_TOP, fmt_time(dur), reg(12.0), SUBTLE);
        let cy = y + 150.0;
        let cx = r.center().x;
        let step = (side / 5.0).min(72.0);
        let hit = |ui: &mut Ui, c: Pos2, id: &str| ui.interact(Rect::from_center_size(c, vec2(48.0, 48.0)), Id::new(id), Sense::click());
        let sc = pos2(cx - 2.0 * step, cy);
        shuffle(&p, sc, 24.0, if self.shuffle { ACCENT } else { SUBTLE });
        if hit(ui, sc, "np_shuffle").clicked() {
            self.pending.push(Action::ToggleShuffle);
        }
        let pc = pos2(cx - step, cy);
        skip(&p, pc, 24.0, TEXT, false);
        if hit(ui, pc, "np_prev").clicked() {
            self.pending.push(Action::Prev);
        }
        p.circle_filled(pos2(cx, cy), 30.0, TEXT);
        if self.playing {
            pause(&p, pos2(cx, cy), 22.0, BG);
        } else {
            play(&p, pos2(cx + 2.0, cy), 22.0, BG);
        }
        if hit(ui, pos2(cx, cy), "np_play").clicked() {
            self.pending.push(Action::TogglePlay);
        }
        let nc = pos2(cx + step, cy);
        skip(&p, nc, 24.0, TEXT, true);
        if hit(ui, nc, "np_next").clicked() {
            self.pending.push(Action::Next);
        }
        let rc = pos2(cx + 2.0 * step, cy);
        repeat(&p, rc, 24.0, if self.repeat != Repeat::Off { ACCENT } else { SUBTLE }, self.repeat == Repeat::One);
        if hit(ui, rc, "np_repeat").clicked() {
            self.pending.push(Action::CycleRepeat);
        }
    }

    fn view_tint(&self) -> (f32, f32) {
        match &self.view {
            View::Home => (0.72, 0.45),
            View::Search => (0.6, 0.0),
            View::Library => (0.06, 0.35),
            View::Liked => (0.74, 0.65),
            View::Playlist(i) => (self.data.playlists.get(*i).map(|p| p.hue).unwrap_or(0.0), 0.6),
            View::Album(a) => (self.albums.iter().find(|x| &x.name == a).map(|x| x.hue).unwrap_or(0.0), 0.6),
            View::Mix(i) => (self.mixes.get(*i).map(|m| m.hue).unwrap_or(0.0), 0.6),
            View::Local => (0.48, 0.55),
            View::Queue => (0.0, 0.0),
            View::Account => (0.05, 0.5),
        }
    }

    fn sidebar(&mut self, ui: &mut Ui) {
        let p = ui.painter().clone();
        Frame::new().fill(PANEL).corner_radius(cr(10)).inner_margin(Margin::symmetric(12, 10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
            logo(&p, pos2(r.left() + 16.0, r.center().y), 14.0);
            p.text(pos2(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, "Wavelet", bold(20.0), TEXT);
            ui.add_space(4.0);
            let home = self.view == View::Home;
            if nav_item(ui, "Home", home, |p, c, col| crate::theme::home(p, c, 20.0, col, home)).clicked() {
                self.pending.push(Action::Go(View::Home));
            }
            let s = self.view == View::Search;
            if nav_item(ui, "Search", s, |p, c, col| search(p, c, 20.0, col, s)).clicked() {
                self.pending.push(Action::Go(View::Search));
            }
        });
        ui.add_space(8.0);
        Frame::new().fill(PANEL).corner_radius(cr(10)).inner_margin(Margin::symmetric(8, 10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(ui.available_height() - 20.0);
            let w = ui.available_width();
            let (r, resp) = ui.allocate_exact_size(vec2(w - 40.0, 36.0), Sense::click());
            let sel = self.view == View::Library;
            let col = if sel || resp.hovered() { TEXT } else { SUBTLE };
            library(&p, pos2(r.left() + 16.0, r.center().y), 20.0, col);
            p.text(pos2(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, "Your Library", bold(15.0), col);
            if resp.clicked() {
                self.pending.push(Action::Go(View::Library));
            }
            let pr = Rect::from_center_size(pos2(r.right() + 20.0, r.center().y), vec2(30.0, 30.0));
            let presp = ui.interact(pr, Id::new("new_pl_side"), Sense::click()).on_hover_text("Create playlist");
            if presp.hovered() {
                p.circle_filled(pr.center(), 15.0, CARD_HOVER);
            }
            plus(&p, pr.center(), 14.0, if presp.hovered() { TEXT } else { SUBTLE });
            if presp.clicked() {
                self.pending.push(Action::NewPlaylist(None));
            }
            ui.add_space(4.0);

            let list_h = ui.available_height() - 48.0;
            ScrollArea::vertical().max_height(list_h.max(60.0)).auto_shrink([false, true]).show(ui, |ui| {
                let liked_n = self.data.liked.len();
                if lib_row(ui, "Liked Songs", &format!("Playlist · {liked_n} songs"), Art::Liked, self.view == View::Liked).clicked() {
                    self.pending.push(Action::Go(View::Liked));
                }
                let pls: Vec<(usize, String, usize, f32)> = self.data.playlists.iter().enumerate().map(|(i, p)| (i, p.name.clone(), p.tracks.len(), p.hue)).collect();
                for (i, name, n, hue) in pls {
                    let resp = lib_row(ui, &name, &format!("Playlist · {n} songs"), Art::Cover(hue, name.clone()), self.view == View::Playlist(i));
                    if resp.clicked() {
                        self.pending.push(Action::Go(View::Playlist(i)));
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Play").clicked() {
                            let ids = self.ids_for(&View::Playlist(i));
                            self.pending.push(Action::PlayList(ids, 0));
                            ui.close_menu();
                        }
                        if ui.button("Edit details").clicked() {
                            self.pending.push(Action::EditPlaylist(i));
                            ui.close_menu();
                        }
                        if ui.button("Delete").clicked() {
                            self.pending.push(Action::DeletePlaylist(i));
                            ui.close_menu();
                        }
                    });
                }
                if !self.data.local.is_empty() {
                    let n = self.data.local.len();
                    if lib_row(ui, "Local files", &format!("{n} songs on this computer"), Art::Local, self.view == View::Local).clicked() {
                        self.pending.push(Action::Go(View::Local));
                    }
                }
                let albums: Vec<(String, String, f32)> = self.albums.iter().filter(|a| !a.local).map(|a| (a.name.clone(), a.artist.clone(), a.hue)).collect();
                for (name, artist, hue) in albums {
                    let sel = self.view == View::Album(name.clone());
                    if lib_row(ui, &name, &format!("Album · {artist}"), Art::Cover(hue, name.clone()), sel).clicked() {
                        self.pending.push(Action::Go(View::Album(name)));
                    }
                }
            });
            ui.add_space(6.0);
            let label = if self.scan_rx.is_some() { "Scanning your music…" } else { "Add music from your computer" };
            if outline_button(ui, label, w, 36.0).clicked() {
                self.pending.push(Action::AddFolder);
            }
        });
    }

    fn top_bar(&mut self, ui: &mut Ui) {
        let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::hover());
        let p = ui.painter().clone();
        for (i, left) in [true, false].iter().enumerate() {
            let c = pos2(bar.left() + 36.0 + i as f32 * 42.0, bar.center().y);
            let r = Rect::from_center_size(c, vec2(32.0, 32.0));
            let enabled = if *left { !self.back.is_empty() } else { !self.forward.is_empty() };
            let resp = ui.interact(r, Id::new(("nav", i)), Sense::click());
            p.circle_filled(c, 16.0, Color32::from_black_alpha(if resp.hovered() && enabled { 200 } else { 130 }));
            chevron(&p, c, 18.0, if enabled { TEXT } else { FAINT }, *left);
            if resp.clicked() && enabled {
                self.pending.push(if *left { Action::Back } else { Action::Forward });
            }
        }
        let narrow = bar.width() < 560.0;
        if self.view == View::Search {
            let right = if narrow { bar.right() - 14.0 } else { (bar.left() + 490.0).min(bar.right() - 180.0) };
            let sr = Rect::from_min_max(pos2(bar.left() + 104.0, bar.center().y - 22.0), pos2(right, bar.center().y + 22.0));
            p.rect_filled(sr, cr(22), CARD_HOVER);
            search(&p, pos2(sr.left() + 24.0, sr.center().y), 18.0, SUBTLE, false);
            let inner = Rect::from_min_max(pos2(sr.left() + 44.0, sr.center().y - 11.0), pos2(sr.right() - 16.0, sr.center().y + 11.0));
            let resp = ui.put(inner, TextEdit::singleline(&mut self.search).frame(false).hint_text("What do you want to listen to?").font(reg(15.0)).desired_width(inner.width()));
            if self.focus_search {
                resp.request_focus();
                self.focus_search = false;
            }
        }
        let Some(user) = self.user.clone() else { return };
        if narrow {
            if self.view != View::Search {
                let badge = Rect::from_min_size(pos2(bar.right() - 14.0 - 92.0, bar.center().y - 15.0), vec2(92.0, 30.0));
                p.rect_filled(badge, cr(15), Color32::from_black_alpha(120));
                p.rect_stroke(badge, cr(15), Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.7)), egui::StrokeKind::Inside);
                p.circle_filled(pos2(badge.left() + 16.0, badge.center().y), 4.0, ACCENT);
                p.text(pos2(badge.left() + 28.0, badge.center().y), Align2::LEFT_CENTER, "Ad-free", bold(13.0), TEXT);
            }
            return;
        }
        let ac = pos2(bar.right() - 36.0, bar.center().y);
        let ar = Rect::from_center_size(ac, vec2(36.0, 36.0));
        let aresp = ui.interact(ar, Id::new("avatar"), Sense::click()).on_hover_text(&user.display_name);
        p.circle_filled(ac, 18.0, Color32::from_black_alpha(140));
        p.circle_filled(ac, 14.0, hsv(library::hue_of(&user.username), 0.55, 0.9));
        let initial = user.display_name.chars().next().unwrap_or('?').to_uppercase().to_string();
        p.text(ac, Align2::CENTER_CENTER, initial, bold(14.0), BG);
        if aresp.hovered() {
            p.circle_stroke(ac, 18.0, Stroke::new(1.5_f32, TEXT));
        }
        let badge = Rect::from_min_size(pos2(ac.x - 30.0 - 92.0, bar.center().y - 15.0), vec2(92.0, 30.0));
        p.rect_filled(badge, cr(15), Color32::from_black_alpha(120));
        p.rect_stroke(badge, cr(15), Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.7)), egui::StrokeKind::Inside);
        p.circle_filled(pos2(badge.left() + 16.0, badge.center().y), 4.0, ACCENT);
        p.text(pos2(badge.left() + 28.0, badge.center().y), Align2::LEFT_CENTER, "Ad-free", bold(13.0), TEXT);
        ui.interact(badge, Id::new("adfree"), Sense::hover()).on_hover_text("No ads. No interruptions between songs. Ever.");

        let popup_id = Id::new("acct_popup");
        if aresp.clicked() {
            ui.memory_mut(|m| m.toggle_popup(popup_id));
        }
        egui::popup::popup_below_widget(ui, popup_id, &aresp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(190.0);
            ui.label(RichText::new(&user.display_name).font(bold(14.0)));
            ui.label(RichText::new(format!("@{}", user.username)).color(SUBTLE).size(12.5));
            ui.separator();
            if ui.button("Profile").clicked() {
                self.pending.push(Action::Go(View::Account));
            }
            if ui.button("Queue").clicked() {
                self.pending.push(Action::Go(View::Queue));
            }
        });
    }

    // ---------------------------------------------------------------- player bar

    fn player_bar(&mut self, ui: &mut Ui) {
        let r = ui.max_rect();
        let p = ui.painter().clone();
        let cur = self.current.clone().and_then(|id| self.track(&id).cloned());

        let left_w = (r.width() * 0.3).min(360.0);
        if let Some(t) = &cur {
            let cv = Rect::from_min_size(pos2(r.left(), r.center().y - 30.0), vec2(60.0, 60.0));
            let art = self.art_for(t);
            draw_art(&p, cv, &art);
            let tw = left_w - 120.0;
            if t.preview {
                let lr = p.text(pos2(cv.right() + 14.0, r.center().y + 20.0), Align2::LEFT_TOP, "Preview · Full song on Spotify", reg(11.5), ACCENT);
                if ui.interact(lr.expand(3.0), Id::new("pb_spotify"), Sense::click()).on_hover_text("Opens this song in Spotify").clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(online::spotify_link(&t.title, &t.artist)));
                }
            }
            let tr = fit_text(&p, pos2(cv.right() + 14.0, r.center().y - 2.0), Align2::LEFT_BOTTOM, &t.title, bold(14.5), TEXT, tw);
            fit_text(&p, pos2(cv.right() + 14.0, r.center().y + 2.0), Align2::LEFT_TOP, &t.artist, reg(12.5), SUBTLE, tw);
            if ui.interact(tr, Id::new("np_title"), Sense::click()).clicked() {
                self.pending.push(Action::Go(View::Album(t.album.clone())));
            }
            let liked = self.data.liked.contains(&t.id);
            let hx = (tr.right().max(cv.right() + 74.0) + 22.0).min(r.left() + left_w - 10.0);
            let hc = pos2(hx, r.center().y);
            let hresp = ui.interact(Rect::from_center_size(hc, vec2(28.0, 28.0)), Id::new("np_like"), Sense::click());
            heart(&p, hc, 15.0, if liked { ACCENT } else if hresp.hovered() { TEXT } else { SUBTLE }, liked);
            if hresp.clicked() {
                self.pending.push(Action::ToggleLike(t.id.clone()));
            }
        } else {
            p.text(pos2(r.left(), r.center().y), Align2::LEFT_CENTER, "Pick something to play", reg(14.0), SUBTLE);
        }

        let cx = r.center().x;
        let cy = r.top() + 26.0;
        let has = cur.is_some();
        let btn = |ui: &mut Ui, c: Pos2, id: &str, tip: &str| ui.interact(Rect::from_center_size(c, vec2(32.0, 32.0)), Id::new(id), Sense::click()).on_hover_text(tip);

        let sc = pos2(cx - 100.0, cy);
        let s = btn(ui, sc, "pb_shuffle", "Shuffle");
        shuffle(&p, sc, 20.0, if self.shuffle { ACCENT } else if s.hovered() { TEXT } else { SUBTLE });
        if self.shuffle {
            p.circle_filled(pos2(sc.x, sc.y + 14.0), 2.0, ACCENT);
        }
        if s.clicked() {
            self.pending.push(Action::ToggleShuffle);
        }
        let pc = pos2(cx - 50.0, cy);
        let pr = btn(ui, pc, "pb_prev", "Previous");
        skip(&p, pc, 18.0, if pr.hovered() && has { TEXT } else { SUBTLE }, false);
        if pr.clicked() {
            self.pending.push(Action::Prev);
        }
        let mc = pos2(cx, cy);
        let m = ui.interact(Rect::from_center_size(mc, vec2(38.0, 38.0)), Id::new("pb_play"), Sense::click()).on_hover_text(if self.playing { "Pause" } else { "Play" });
        p.circle_filled(mc, if m.hovered() { 19.5 } else { 18.0 }, TEXT);
        if self.playing {
            pause(&p, mc, 14.0, BG);
        } else {
            play(&p, pos2(mc.x + 1.5, mc.y), 14.0, BG);
        }
        if m.clicked() {
            self.pending.push(Action::TogglePlay);
        }
        let nc = pos2(cx + 50.0, cy);
        let nr = btn(ui, nc, "pb_next", "Next");
        skip(&p, nc, 18.0, if nr.hovered() && has { TEXT } else { SUBTLE }, true);
        if nr.clicked() {
            self.pending.push(Action::Next);
        }
        let rc = pos2(cx + 100.0, cy);
        let tip = match self.repeat {
            Repeat::Off => "Repeat",
            Repeat::All => "Repeat all",
            Repeat::One => "Repeat one",
        };
        let rr = btn(ui, rc, "pb_repeat", tip);
        let rcol = if self.repeat != Repeat::Off { ACCENT } else if rr.hovered() { TEXT } else { SUBTLE };
        repeat(&p, rc, 20.0, rcol, self.repeat == Repeat::One);
        if self.repeat != Repeat::Off {
            p.circle_filled(pos2(rc.x, rc.y + 14.0), 2.0, ACCENT);
        }
        if rr.clicked() {
            self.pending.push(Action::CycleRepeat);
        }

        let dur = self.player.duration().max(0.0);
        let pos = if has { self.player.position() } else { 0.0 };
        let bw = (r.width() * 0.4).min(600.0);
        let py = r.top() + 62.0;
        let bar = Rect::from_center_size(pos2(cx, py), vec2(bw, 4.0));
        let frac = if dur > 0.0 { (pos / dur).clamp(0.0, 1.0) } else { 0.0 };
        let shown = self.seek_drag.unwrap_or(frac);
        let loading = self.downloading.is_some() && self.downloading == self.current;
        p.text(pos2(bar.left() - 12.0, py), Align2::RIGHT_CENTER, if loading { "Loading…".to_string() } else { fmt_time(shown * dur) }, reg(12.0), if loading { ACCENT } else { SUBTLE });
        p.text(pos2(bar.right() + 12.0, py), Align2::LEFT_CENTER, fmt_time(dur), reg(12.0), SUBTLE);
        if let Some(ev) = slider_bar(ui, &p, bar, "seek", shown, has) {
            match ev {
                SliderEv::Drag(v) => self.seek_drag = Some(v),
                SliderEv::Done(v) => {
                    self.seek_drag = None;
                    self.pending.push(Action::Seek(v));
                }
            }
        }

        let qc = pos2(r.right() - 196.0, r.center().y);
        let qr = btn(ui, qc, "pb_queue", "Queue");
        queue(&p, qc, 18.0, if self.view == View::Queue { ACCENT } else if qr.hovered() { TEXT } else { SUBTLE });
        if qr.clicked() {
            self.pending.push(if self.view == View::Queue { Action::Back } else { Action::Go(View::Queue) });
        }
        let vc = pos2(r.right() - 156.0, r.center().y);
        let vr = btn(ui, vc, "pb_mute", "Mute");
        let vol = self.data.volume;
        volume(&p, vc, 20.0, if vr.hovered() { TEXT } else { SUBTLE }, vol);
        if vr.clicked() {
            self.pending.push(Action::Volume(if vol > 0.0 { 0.0 } else { 0.7 }));
        }
        let vbar = Rect::from_min_size(pos2(r.right() - 130.0, r.center().y - 2.0), vec2(120.0, 4.0));
        if let Some(ev) = slider_bar(ui, &p, vbar, "volume", vol, true) {
            let v = match ev {
                SliderEv::Drag(v) | SliderEv::Done(v) => v,
            };
            self.pending.push(Action::Volume(v));
        }
    }

    fn edit_window(&mut self, ctx: &Context) {
        let Some((idx, mut name, mut desc)) = self.editing.clone() else { return };
        let mut close = false;
        let mut save = false;
        let ew = (ctx.screen_rect().width() - 90.0).clamp(200.0, 380.0);
        egui::Window::new("Edit details")
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .frame(Frame::new().fill(CARD).corner_radius(cr(12)).inner_margin(Margin::same(24)).stroke(Stroke::new(1.0_f32, CARD_HOVER)))
            .show(ctx, |ui| {
                ui.set_width(ew);
                ui.label(RichText::new("Edit details").font(bold(22.0)));
                ui.add_space(10.0);
                field_label(ui, "Name");
                let r = ui.add(text_field(&mut name, "Playlist name", false, ew));
                if enter_pressed(ui, &r) {
                    save = true;
                }
                field_label(ui, "Description");
                ui.add(TextEdit::multiline(&mut desc).hint_text("Add an optional description").desired_width(ew).desired_rows(3).margin(Margin::symmetric(10, 8)));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if pill_button(ui, "Save", 100.0, 38.0, TEXT, BG).clicked() {
                        save = true;
                    }
                    if ui.add(egui::Button::new("Cancel").frame(false)).clicked() {
                        close = true;
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            close = true;
        }
        if save {
            if let Some(pl) = self.data.playlists.get_mut(idx) {
                if !name.trim().is_empty() {
                    pl.name = name.trim().to_string();
                }
                pl.description = desc.trim().to_string();
            }
            self.save();
            self.editing = None;
        } else if close {
            self.editing = None;
        } else {
            self.editing = Some((idx, name, desc));
        }
    }

    fn toast_ui(&mut self, ctx: &Context) {
        let now = ctx.input(|i| i.time);
        let Some((msg, t0)) = self.toast.clone() else { return };
        let t0 = if t0 < 0.0 {
            self.toast = Some((msg.clone(), now));
            now
        } else {
            t0
        };
        if now - t0 > 2.6 {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        egui::Area::new(Id::new("toast"))
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, if is_phone(ctx) { -150.0 } else { -112.0 }))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                Frame::new().fill(TEXT).corner_radius(cr(8)).inner_margin(Margin::symmetric(16, 10)).show(ui, |ui| {
                    ui.label(RichText::new(msg).color(BG).font(bold(14.0)));
                });
            });
    }
}

fn is_phone(ctx: &Context) -> bool {
    ctx.screen_rect().width() < 700.0
}

/// Shown instead of the app when it closed unexpectedly last time, so the reason can be shared.
pub struct CrashReport {
    pub text: String,
}

impl eframe::App for CrashReport {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(24.0);
                ui.label(RichText::new("Wavelet closed unexpectedly last time").font(bold(22.0)));
                ui.add_space(8.0);
                ui.label("Please take a screenshot of this screen and send it, so it can be fixed. Then close Wavelet and open it again.");
                ui.add_space(12.0);
                ui.label(RichText::new(&self.text).monospace().size(12.0));
            });
        });
    }
}

impl eframe::App for WaveletApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.poll_scan();
        self.poll_net(ctx);
        self.main_ui(ctx);
        let actions = std::mem::take(&mut self.pending);
        for a in actions {
            self.apply(a);
        }
        self.tick(ctx);
        self.handle_screenshot(ctx);
    }

    /// Some phone keyboards only send key presses, not typed letters. Turn those
    /// presses into text so typing in Search works on every phone.
    #[cfg(target_os = "android")]
    fn raw_input_hook(&mut self, _ctx: &Context, raw: &mut egui::RawInput) {
        if raw.events.iter().any(|e| matches!(e, egui::Event::Text(_))) {
            self.saw_text_input = true;
        }
        if self.saw_text_input {
            return;
        }
        let mut typed = vec![];
        for e in &raw.events {
            if let egui::Event::Key { key, pressed: true, modifiers, .. } = e {
                let name = key.name();
                let text = if *key == Key::Space {
                    Some(" ".to_string())
                } else if name.chars().count() == 1 && name.chars().all(|c| c.is_ascii_alphanumeric()) {
                    Some(if modifiers.shift { name.to_string() } else { name.to_lowercase() })
                } else {
                    None
                };
                if let Some(t) = text {
                    typed.push(egui::Event::Text(t));
                }
            }
        }
        raw.events.extend(typed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait(app: &mut WaveletApp, ctx: &Context, secs: u64, cond: impl Fn(&WaveletApp) -> bool) -> bool {
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_secs() < secs {
            app.poll_net(ctx);
            app.tick(ctx);
            if cond(app) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        false
    }

    /// Real network: the chart songs load and play one after another.
    #[test]
    #[ignore]
    fn chart_list_plays_through() {
        let ctx = Context::default();
        let mut app = WaveletApp::new(Some(std::env::temp_dir().join("wavelet-test.png")));
        app.hits_state = 1;
        app.spawn_net(|| NetMsg::Hits(online::top_hits(6)));
        assert!(wait(&mut app, &ctx, 40, |a| a.hits_state == 2), "the charts load");
        let hits = app.hits.clone();
        assert!(hits.len() >= 3, "{:?}", app.hits_err);

        app.start(hits.clone(), 1);
        assert!(wait(&mut app, &ctx, 120, |a| a.playing), "a chart song starts");
        assert_eq!(app.current.as_deref(), Some(hits[1].as_str()));
        println!("playing {} ({:.0}s)", app.track(&hits[1]).unwrap().title, app.player.duration());

        app.next(true);
        assert!(wait(&mut app, &ctx, 120, |a| a.playing), "the next song follows");
        assert_eq!(app.current.as_deref(), Some(hits[2].as_str()));
    }
}
