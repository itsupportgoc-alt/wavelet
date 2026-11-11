use crate::synth::SynthSpec;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub enum TrackSource {
    Demo(SynthSpec),
    File(PathBuf),
    /// A song from the internet, downloaded to `cache` before playing.
    Online { url: String, cache: PathBuf },
}

#[derive(Clone, Debug)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub duration: f32,
    pub source: TrackSource,
    pub hue: f32,
    /// Real cover art (web address), if any.
    pub cover: Option<String>,
    /// Only a 30-second preview is playable here.
    pub preview: bool,
}

#[derive(Clone, Debug)]
pub struct AlbumInfo {
    pub name: String,
    pub artist: String,
    pub hue: f32,
    pub tracks: Vec<String>,
    pub local: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalTrack {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Playlist {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub tracks: Vec<String>,
    pub hue: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct UserData {
    pub liked: Vec<String>,
    pub playlists: Vec<Playlist>,
    pub folders: Vec<PathBuf>,
    pub local: Vec<LocalTrack>,
    pub recent: Vec<String>,
    pub volume: f32,
    pub shuffle: bool,
    /// Free Jamendo key ("Client ID") for real music.
    pub jamendo_key: String,
    /// Internet songs this person liked, saved or played.
    pub online: Vec<crate::online::OnlineTrack>,
}

impl Default for UserData {
    fn default() -> Self {
        Self {
            liked: vec![],
            playlists: vec![],
            folders: vec![],
            local: vec![],
            recent: vec![],
            volume: 0.7,
            shuffle: false,
            jamendo_key: String::new(),
            online: vec![],
        }
    }
}

impl UserData {
    fn path(dir: &Path, user: &str) -> PathBuf {
        dir.join("users").join(format!("{user}.json"))
    }
    pub fn load(dir: &Path, user: &str) -> Self {
        std::fs::read_to_string(Self::path(dir, user))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, dir: &Path, user: &str) {
        let p = Self::path(dir, user);
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, s);
        }
    }
    pub fn delete(dir: &Path, user: &str) {
        let _ = std::fs::remove_file(Self::path(dir, user));
    }
}

pub fn hue_of(s: &str) -> f32 {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    (h % 1000) as f32 / 1000.0
}

struct AlbumDef {
    album: &'static str,
    artist: &'static str,
    genre: &'static str,
    hue: f32,
    bpm: f32,
    root: i32,
    minor: bool,
    lead: u8,
    drums: u8,
    swing: f32,
    songs: [(&'static str, [i32; 4], u32); 3],
}

pub fn demo_catalog() -> Vec<Track> {
    let defs = [
        AlbumDef {
            album: "Night Drive",
            artist: "Neon Harbor",
            genre: "Electronic",
            hue: 0.74,
            bpm: 118.0,
            root: 45,
            minor: true,
            lead: 1,
            drums: 2,
            swing: 0.0,
            songs: [
                ("City Lights Fade", [0, 5, 2, 6], 132),
                ("Midnight Overpass", [0, 3, 5, 4], 148),
                ("Chrome Reflections", [5, 3, 0, 4], 126),
            ],
        },
        AlbumDef {
            album: "Morning Light",
            artist: "Lumen Fields",
            genre: "Chill",
            hue: 0.11,
            bpm: 92.0,
            root: 48,
            minor: false,
            lead: 0,
            drums: 1,
            swing: 0.08,
            songs: [
                ("First Coffee", [0, 4, 5, 3], 124),
                ("Open Windows", [3, 0, 4, 5], 138),
                ("Slow Sunday", [0, 5, 3, 4], 150),
            ],
        },
        AlbumDef {
            album: "Low Tide",
            artist: "Paper Moons",
            genre: "Lo-fi",
            hue: 0.53,
            bpm: 76.0,
            root: 50,
            minor: false,
            lead: 2,
            drums: 1,
            swing: 0.18,
            songs: [
                ("Sea Glass", [1, 4, 0, 5], 128),
                ("Driftwood", [3, 2, 1, 0], 142),
                ("Harbor Rain", [5, 1, 4, 0], 120),
            ],
        },
        AlbumDef {
            album: "Ember & Ash",
            artist: "Copper Sky",
            genre: "Ambient",
            hue: 0.03,
            bpm: 66.0,
            root: 47,
            minor: true,
            lead: 0,
            drums: 0,
            swing: 0.0,
            songs: [
                ("Campfire Stories", [0, 5, 3, 6], 156),
                ("Northern Trail", [0, 2, 5, 4], 140),
                ("Dusk Signal", [5, 6, 0, 0], 132),
            ],
        },
        AlbumDef {
            album: "Afterglow",
            artist: "Solstice Theory",
            genre: "Dance",
            hue: 0.90,
            bpm: 126.0,
            root: 46,
            minor: true,
            lead: 3,
            drums: 2,
            swing: 0.0,
            songs: [
                ("Run the Night", [0, 5, 6, 4], 136),
                ("Signal Lost", [0, 6, 5, 6], 128),
                ("Electric Bloom", [3, 4, 0, 0], 144),
            ],
        },
    ];

    let mut out = vec![];
    for (ai, d) in defs.iter().enumerate() {
        for (si, (title, prog, secs)) in d.songs.iter().enumerate() {
            let seed = (ai as u64 + 1) * 7919 + si as u64 * 104729;
            out.push(Track {
                id: format!("demo:{ai}:{si}"),
                title: title.to_string(),
                artist: d.artist.to_string(),
                album: d.album.to_string(),
                genre: d.genre.to_string(),
                duration: *secs as f32,
                source: TrackSource::Demo(SynthSpec {
                    seed,
                    bpm: d.bpm + si as f32 * 2.0,
                    root: d.root + [0, 2, -3][si],
                    minor: d.minor,
                    prog: *prog,
                    lead: d.lead,
                    drums: d.drums,
                    secs: *secs,
                    swing: d.swing,
                }),
                hue: d.hue,
                cover: None,
                preview: false,
            });
        }
    }
    out
}

pub fn online_to_track(o: &crate::online::OnlineTrack, cache_dir: &Path) -> Track {
    Track {
        id: o.track_id(),
        title: o.title.clone(),
        artist: o.artist.clone(),
        album: o.album.clone(),
        genre: "Online".into(),
        source: TrackSource::Online { url: o.audio.clone(), cache: cache_dir.join(format!("{}_{}.mp3", o.prefix(), o.id)) },
        hue: hue_of(&o.album),
        cover: if o.cover.is_empty() { None } else { Some(o.cover.clone()) },
        // A catalogue song only plays 30 seconds when the full-song helper is missing.
        // A catalogue song is a real song: it plays its whole length, so no preview tag
        // and no 30-second length.
        preview: false,
        duration: o.duration,
    }
}

pub fn local_to_track(l: &LocalTrack) -> Track {
    Track {
        id: format!("file:{}", l.path.display()),
        title: l.title.clone(),
        artist: l.artist.clone(),
        album: l.album.clone(),
        genre: "Local files".into(),
        duration: l.duration,
        source: TrackSource::File(l.path.clone()),
        hue: hue_of(&l.album),
        cover: None,
        preview: false,
    }
}

const AUDIO_EXT: [&str; 7] = ["mp3", "flac", "wav", "ogg", "m4a", "aac", "opus"];

pub fn scan_folder(dir: &Path) -> Vec<LocalTrack> {
    use lofty::prelude::*;
    let mut out = vec![];
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let ext = p
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if !AUDIO_EXT.contains(&ext.as_str()) {
                continue;
            }
            let stem = p
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Unknown".into());
            let folder = p
                .parent()
                .and_then(|x| x.file_name())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Local files".into());
            let mut lt = LocalTrack {
                path: p.clone(),
                title: stem,
                artist: "Unknown artist".into(),
                album: folder,
                duration: 0.0,
            };
            if let Ok(tf) = lofty::read_from_path(&p) {
                lt.duration = tf.properties().duration().as_secs_f32();
                if let Some(tag) = tf.primary_tag().or_else(|| tf.first_tag()) {
                    if let Some(t) = tag.title() {
                        if !t.trim().is_empty() {
                            lt.title = t.to_string();
                        }
                    }
                    if let Some(a) = tag.artist() {
                        if !a.trim().is_empty() {
                            lt.artist = a.to_string();
                        }
                    }
                    if let Some(a) = tag.album() {
                        if !a.trim().is_empty() {
                            lt.album = a.to_string();
                        }
                    }
                }
            }
            out.push(lt);
        }
    }
    out.sort_by(|a, b| (&a.artist, &a.album, &a.path).cmp(&(&b.artist, &b.album, &b.path)));
    out
}
