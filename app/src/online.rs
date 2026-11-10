//! Real music from the internet, no key needed:
//! - Deezer's public catalogue: every well-known song, with cover art and a 30-second preview.
//! - Audius: full-length songs (trending, search), no key.
//! - Jamendo (optional, free key from devportal.jamendo.com): extra full-length Creative Commons songs.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub const KEY_SITE: &str = "https://devportal.jamendo.com/";
const API: &str = "https://api.jamendo.com/v3.0/tracks/";

/// Shown when the music services can't be reached (offline, blocked or much too slow).
pub const OFFLINE_MSG: &str = "Can't reach the internet, so the songs can't load. Check your connection, then try again.";

/// Ask for a web page, giving up after 25 seconds instead of hanging.
pub fn http_get(url: &str) -> Result<Vec<u8>, String> {
    http_get_within(url, 25)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OnlineTrack {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: f32,
    pub audio: String,
    /// "deezer" (30-second previews) or "jamendo" (full songs).
    #[serde(default)]
    pub service: String,
    #[serde(default)]
    pub cover: String,
}

impl OnlineTrack {
    pub fn is_preview(&self) -> bool {
        self.service == "deezer"
    }
    /// Short tag used in ids and cache file names.
    pub fn prefix(&self) -> &'static str {
        match self.service.as_str() {
            "deezer" => "dz",
            "audius" => "au",
            _ => "jam",
        }
    }
    pub fn track_id(&self) -> String {
        format!("{}:{}", self.prefix(), self.id)
    }
}

/// Web address that opens this song in Spotify (search by title + artist).
pub fn spotify_link(title: &str, artist: &str) -> String {
    let q = format!("{title} {artist}");
    let mut o = String::from("https://open.spotify.com/search/");
    for b in q.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => o.push(b as char),
            _ => o.push_str(&format!("%{b:02X}")),
        }
    }
    o
}

// ------------------------------------------------------------ Audius (no key, full songs)

const AUDIUS: &str = "https://api.audius.co/v1";
const APP_NAME: &str = "wavelet-music-player";

pub fn parse_audius(body: &[u8]) -> Result<Vec<OnlineTrack>, String> {
    let v: serde_json::Value = serde_json::from_slice(body).map_err(|_| "The full-song service sent an unexpected reply.".to_string())?;
    let Some(items) = v["data"].as_array() else {
        return Err("The full-song service is busy right now. Try again in a moment.".into());
    };
    let mut out = vec![];
    for r in items {
        if r["is_streamable"].as_bool() == Some(false) || r["is_delete"].as_bool() == Some(true) {
            continue;
        }
        let id = r["id"].as_str().unwrap_or("").to_string();
        if id.is_empty() {
            continue;
        }
        let art = &r["artwork"];
        let cover = art["480x480"].as_str().or(art["150x150"].as_str()).unwrap_or("").to_string();
        let genre = r["genre"].as_str().unwrap_or("").to_string();
        out.push(OnlineTrack {
            audio: format!("{AUDIUS}/tracks/{id}/stream?app_name={APP_NAME}"),
            id,
            title: r["title"].as_str().unwrap_or("").to_string(),
            artist: r["user"]["name"].as_str().unwrap_or("").to_string(),
            album: if genre.is_empty() { "Single".into() } else { format!("{genre} · Single") },
            duration: r["duration"].as_f64().unwrap_or(0.0) as f32,
            service: "audius".into(),
            cover,
        });
    }
    Ok(out)
}

fn with_retry(url: &str, parse: fn(&[u8]) -> Result<Vec<OnlineTrack>, String>) -> Result<Vec<OnlineTrack>, String> {
    match http_get(url).and_then(|b| parse(&b)) {
        Ok(t) => Ok(t),
        Err(_) => {
            std::thread::sleep(std::time::Duration::from_millis(1200));
            parse(&http_get(url)?)
        }
    }
}

/// Songs trending right now, full length.
pub fn trending_full(limit: u32) -> Result<Vec<OnlineTrack>, String> {
    with_retry(&format!("{AUDIUS}/tracks/trending?app_name={APP_NAME}&limit={limit}"), parse_audius)
}

/// Full-length songs matching the search.
pub fn full_search(query: &str, limit: u32) -> Result<Vec<OnlineTrack>, String> {
    let mut t = with_retry(&format!("{AUDIUS}/tracks/search?query={}&app_name={APP_NAME}", encode(&clean_query(query))), parse_audius)?;
    t.truncate(limit as usize);
    Ok(t)
}

// ------------------------------------------------------------ Deezer catalogue (no key)

const DEEZER: &str = "https://api.deezer.com";

pub fn parse_deezer(body: &[u8]) -> Result<Vec<OnlineTrack>, String> {
    let v: serde_json::Value = serde_json::from_slice(body).map_err(|_| "The song catalogue sent an unexpected reply.".to_string())?;
    if v.get("error").is_some() {
        return Err("The song catalogue is busy right now. Try again in a moment.".into());
    }
    let items = v["data"].as_array().cloned().unwrap_or_else(|| if v["id"].is_number() { vec![v.clone()] } else { vec![] });
    let mut out = vec![];
    for r in items {
        let preview = r["preview"].as_str().unwrap_or("").to_string();
        if preview.is_empty() || r["readable"].as_bool() == Some(false) {
            continue;
        }
        out.push(OnlineTrack {
            id: r["id"].as_i64().map(|n| n.to_string()).unwrap_or_default(),
            title: r["title"].as_str().unwrap_or("").to_string(),
            artist: r["artist"]["name"].as_str().unwrap_or("").to_string(),
            album: r["album"]["title"].as_str().unwrap_or("").to_string(),
            duration: 30.0,
            audio: preview,
            service: "deezer".into(),
            cover: r["album"]["cover_medium"].as_str().unwrap_or("").to_string(),
        });
    }
    Ok(out)
}

/// People often type "songs of yellow" or "play yellow song"; the catalogue takes every word
/// literally, so drop those filler words and keep what names the song or artist.
pub fn clean_query(q: &str) -> String {
    const LEAD: [&str; 16] = [
        "play ", "listen to ", "search ", "find ", "the songs of ", "the song of ", "songs of ", "song of ", "songs by ", "song by ",
        "songs from ", "music of ", "music by ", "music from ", "the song ", "song ",
    ];
    const TRAIL: [&str; 6] = [" songs", " song", " music", " lyrics", " mp3", " full song"];
    let mut s = q.trim().to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ");
    let original = s.clone();
    loop {
        let before = s.clone();
        for p in LEAD {
            if let Some(rest) = s.strip_prefix(p) {
                s = rest.trim().to_string();
            }
        }
        for p in TRAIL {
            if let Some(rest) = s.strip_suffix(p) {
                s = rest.trim().to_string();
            }
        }
        if s == before {
            break;
        }
    }
    let s = s.replace(" by ", " ");
    if s.is_empty() {
        original
    } else {
        s
    }
}

pub fn catalog_search(query: &str, limit: u32) -> Result<Vec<OnlineTrack>, String> {
    let url = format!("{DEEZER}/search?q={}&limit={limit}", encode(&clean_query(query)));
    match http_get(&url).and_then(|b| parse_deezer(&b)) {
        Ok(t) => Ok(t),
        Err(_) => {
            // The catalogue sometimes asks clients to slow down; try once more.
            std::thread::sleep(std::time::Duration::from_millis(1200));
            parse_deezer(&http_get(&url)?)
        }
    }
}

pub fn top_hits(limit: u32) -> Result<Vec<OnlineTrack>, String> {
    parse_deezer(&http_get(&format!("{DEEZER}/chart/0/tracks?limit={limit}"))?)
}

/// Preview links expire after a while, so ask for a fresh one right before downloading.
pub fn fresh_preview(id: &str) -> Option<String> {
    let body = http_get(&format!("{DEEZER}/track/{id}")).ok()?;
    parse_deezer(&body).ok()?.into_iter().next().map(|t| t.audio)
}

/// One shared web client, built into the app (no outside tools needed).
fn agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        let b = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(10))
            .timeout_read(std::time::Duration::from_secs(30))
            .user_agent("Wavelet/0.5 (music player)")
            .redirects(8);
        // On computers, use the system's own secure-connection support.
        #[cfg(not(target_os = "android"))]
        let b = match native_tls::TlsConnector::new() {
            Ok(c) => b.tls_connector(Arc::new(c)),
            Err(_) => b,
        };
        b.build()
    })
}

fn friendly(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(429, _) => "The music service is busy right now. Try again in a moment.".into(),
        ureq::Error::Status(code, _) => format!("The music service answered with an error ({code}). Try again in a moment."),
        ureq::Error::Transport(_) => OFFLINE_MSG.into(),
    }
}

/// Download a web address, giving up after `secs`.
pub fn http_get_within(url: &str, secs: u32) -> Result<Vec<u8>, String> {
    let resp = agent().get(url).timeout(std::time::Duration::from_secs(secs as u64)).call().map_err(friendly)?;
    let mut buf = Vec::new();
    resp.into_reader().read_to_end(&mut buf).map_err(|_| OFFLINE_MSG.to_string())?;
    Ok(buf)
}

// ------------------------------------------------------------ streaming (play while downloading)

/// A song being downloaded in the background. Playback can start as soon as
/// the first part has arrived, instead of waiting for the whole file.
pub struct StreamState {
    /// Bytes saved so far.
    pub got: AtomicU64,
    /// Full size when the server told us (0 = unknown).
    pub total: AtomicU64,
    pub done: AtomicBool,
    pub error: Mutex<Option<String>>,
    /// File the bytes are written to while downloading.
    pub part: PathBuf,
}

/// How much must have arrived before a song starts (about 8 seconds of music).
const START_BYTES: u64 = 320 * 1024;

impl StreamState {
    pub fn failed(&self) -> Option<String> {
        self.error.lock().ok().and_then(|e| e.clone())
    }
    /// Enough has arrived to start playing.
    pub fn ready(&self) -> bool {
        if self.failed().is_some() {
            return false;
        }
        let got = self.got.load(Ordering::SeqCst);
        self.done.load(Ordering::SeqCst) || got >= START_BYTES
    }
    pub fn reader(state: &Arc<StreamState>) -> Result<StreamReader, String> {
        let file = File::open(&state.part).map_err(|_| "This song hasn't started downloading.".to_string())?;
        Ok(StreamReader { file, pos: 0, st: state.clone() })
    }
}

fn stream_fail(st: &StreamState, msg: String) {
    if let Ok(mut e) = st.error.lock() {
        *e = Some(msg);
    }
    st.done.store(true, Ordering::SeqCst);
}

/// Start downloading a song to `cache`. For catalogue previews (`dz_id`), a fresh link
/// is fetched first because old ones expire.
pub fn start_stream(url: String, dz_id: Option<String>, cache: PathBuf) -> Arc<StreamState> {
    if let Some(p) = cache.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let part = cache.with_extension(format!("part{}", fastrand::u32(..)));
    let st = Arc::new(StreamState {
        got: AtomicU64::new(0),
        total: AtomicU64::new(0),
        done: AtomicBool::new(false),
        error: Mutex::new(None),
        part: part.clone(),
    });
    let file = match File::create(&part) {
        Ok(f) => f,
        Err(e) => {
            stream_fail(&st, format!("Couldn't save the song: {e}"));
            return st;
        }
    };
    let st2 = st.clone();
    std::thread::spawn(move || {
        let mut file = file;
        let url = match dz_id {
            Some(id) => fresh_preview(&id).unwrap_or(url),
            None => url,
        };
        let mut tries = 0;
        let resp = loop {
            tries += 1;
            match agent().get(&url).call() {
                Ok(r) => break r,
                Err(e) if tries < 2 => {
                    let _ = e;
                    std::thread::sleep(std::time::Duration::from_millis(800));
                }
                Err(e) => {
                    stream_fail(&st2, friendly(e));
                    return;
                }
            }
        };
        if let Some(n) = resp.header("Content-Length").and_then(|v| v.trim().parse::<u64>().ok()) {
            st2.total.store(n, Ordering::SeqCst);
        }
        let mut rd = resp.into_reader();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match rd.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if file.write_all(&buf[..n]).is_err() {
                        stream_fail(&st2, "Couldn't save the song (is the disk full?).".into());
                        return;
                    }
                    st2.got.fetch_add(n as u64, Ordering::SeqCst);
                }
                Err(_) => {
                    stream_fail(&st2, "The song stopped downloading. Check your connection, then try again.".into());
                    return;
                }
            }
        }
        let _ = file.flush();
        drop(file);
        if st2.got.load(Ordering::SeqCst) < 1024 {
            stream_fail(&st2, "The song couldn't be downloaded.".into());
            return;
        }
        // Keep a finished copy so the song plays instantly next time.
        let tmp = cache.with_extension(format!("tmp{}", fastrand::u32(..)));
        if std::fs::copy(&part, &tmp).is_ok() && std::fs::rename(&tmp, &cache).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        st2.done.store(true, Ordering::SeqCst);
    });
    st
}

/// Reads a song file that may still be downloading: waits for bytes that haven't arrived yet.
pub struct StreamReader {
    file: File,
    pos: u64,
    st: Arc<StreamState>,
}

impl std::io::Read for StreamReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut waited = 0;
        loop {
            let got = self.st.got.load(Ordering::SeqCst);
            if self.pos < got {
                let n = ((got - self.pos) as usize).min(buf.len());
                self.file.seek(SeekFrom::Start(self.pos))?;
                let r = self.file.read(&mut buf[..n])?;
                self.pos += r as u64;
                return Ok(r);
            }
            if self.st.done.load(Ordering::SeqCst) || waited > 600 {
                return Ok(0);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            waited += 1;
        }
    }
}

impl Seek for StreamReader {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let total = match self.st.total.load(Ordering::SeqCst) {
            0 => self.st.got.load(Ordering::SeqCst),
            n => n,
        };
        let new = match to {
            SeekFrom::Start(n) => n as i64,
            SeekFrom::Current(d) => self.pos as i64 + d,
            SeekFrom::End(d) => total as i64 + d,
        };
        if new < 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "seek before start"));
        }
        self.pos = new as u64;
        Ok(self.pos)
    }
}

fn encode(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => o.push(b as char),
            b' ' => o.push('+'),
            _ => o.push_str(&format!("%{b:02X}")),
        }
    }
    o
}

/// Turn the service's reply into songs, or a plain-language error.
pub fn parse(body: &[u8]) -> Result<Vec<OnlineTrack>, String> {
    let v: serde_json::Value = serde_json::from_slice(body).map_err(|_| "The music service sent an unexpected reply.".to_string())?;
    let h = &v["headers"];
    if h["status"].as_str() != Some("success") {
        let code = h["code"].as_i64().unwrap_or(-1);
        return Err(match code {
            5 => "That key isn't valid. Copy the Client ID from your Jamendo app page.".into(),
            11 => "Jamendo has switched this key's application off (a key copied from a public list stops working). Make your own free key at devportal.jamendo.com.".into(),
            _ => format!("The music service said: {}", h["error_message"].as_str().unwrap_or("unknown error")),
        });
    }
    let mut out = vec![];
    for r in v["results"].as_array().cloned().unwrap_or_default() {
        let s = |k: &str| r[k].as_str().map(|x| x.to_string()).or_else(|| r[k].as_i64().map(|n| n.to_string())).unwrap_or_default();
        let audio = s("audio");
        if audio.is_empty() {
            continue;
        }
        out.push(OnlineTrack {
            id: s("id"),
            title: s("name"),
            artist: s("artist_name"),
            album: s("album_name"),
            duration: r["duration"].as_f64().or_else(|| r["duration"].as_str().and_then(|d| d.parse().ok())).unwrap_or(0.0) as f32,
            audio,
            service: "jamendo".into(),
            cover: s("image"),
        });
    }
    Ok(out)
}

pub fn search(key: &str, query: &str, limit: u32) -> Result<Vec<OnlineTrack>, String> {
    let url = format!("{API}?client_id={}&format=json&limit={limit}&audioformat=mp32&include=musicinfo&search={}", encode(key.trim()), encode(query));
    parse(&http_get(&url)?)
}

pub fn popular(key: &str, limit: u32) -> Result<Vec<OnlineTrack>, String> {
    let url = format!("{API}?client_id={}&format=json&limit={limit}&audioformat=mp32&order=popularity_week", encode(key.trim()));
    parse(&http_get(&url)?)
}

/// Download one song to the cache so it can be played (and seeked) like a local file.
pub fn download(url: &str, path: &Path) -> Result<(), String> {
    // Song files are much bigger than API replies, so they get longer.
    let bytes = http_get_within(url, 240)?;
    if bytes.len() < 1024 {
        return Err("The song couldn't be downloaded.".into());
    }
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    // Unique temporary name, so two downloads of the same song can't clash.
    let tmp = path.with_extension(format!("part{}", fastrand::u32(..)));
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_songs_and_errors() {
        let ok = br#"{"headers":{"status":"success","code":0},"results":[
            {"id":"1532771","name":"Morning","duration":185,"artist_name":"Some Band","album_name":"First","audio":"https://prod-1.storage.jamendo.com/?trackid=1532771&format=mp32"},
            {"id":"2","name":"No audio","duration":10,"artist_name":"x","album_name":"y","audio":""}]}"#;
        let t = parse(ok).unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].track_id(), "jam:1532771");
        assert_eq!(t[0].duration, 185.0);
        let bad = br#"{"headers":{"status":"failed","code":5,"error_message":"x"},"results":[]}"#;
        assert!(parse(bad).unwrap_err().contains("isn't valid"));
        assert_eq!(encode("lo fi & chill"), "lo+fi+%26+chill");
        let dz = br#"{"data":[{"id":908604612,"readable":true,"title":"Blinding Lights","duration":200,"preview":"https://x/p.mp3","artist":{"name":"The Weeknd"},"album":{"title":"After Hours","cover_medium":"https://x/c.jpg"}}]}"#;
        let t = parse_deezer(dz).unwrap();
        assert_eq!(t[0].track_id(), "dz:908604612");
        assert!(t[0].is_preview() && t[0].cover.ends_with("c.jpg"));
        assert_eq!(clean_query("songs of yellow"), "yellow");
        assert_eq!(clean_query("Play  Yellow song"), "yellow");
        assert_eq!(clean_query("yellow by coldplay"), "yellow coldplay");
        assert_eq!(clean_query("songs by coldplay"), "coldplay");
        assert_eq!(clean_query("songs"), "songs");
        assert_eq!(spotify_link("Blinding Lights", "The Weeknd"), "https://open.spotify.com/search/Blinding%20Lights%20The%20Weeknd");
    }

    /// Real network: trending full songs are listed and one downloads in full.
    #[test]
    #[ignore]
    fn live_full_songs() {
        let t = trending_full(5).unwrap();
        assert!(!t.is_empty());
        assert!(t[0].duration > 60.0, "full length, not a preview: {:?}", t[0]);
        assert_eq!(t[0].track_id(), format!("au:{}", t[0].id));
        let p = std::env::temp_dir().join("wavelet-test-full.mp3");
        download(&t[0].audio, &p).unwrap();
        let len = std::fs::metadata(&p).unwrap().len();
        println!("trending: {} by {} ({:.0}s), downloaded {} KB", t[0].title, t[0].artist, t[0].duration, len / 1024);
        assert!(len > 500_000);
        assert!(!full_search("love", 5).unwrap().is_empty());
    }

    /// Real network: the public catalogue finds a famous song and its preview downloads.
    #[test]
    #[ignore]
    fn live_catalog() {
        let t = catalog_search("blinding lights the weeknd", 3).unwrap();
        assert!(t.iter().any(|x| x.title.contains("Blinding Lights")), "{t:?}");
        assert!(!top_hits(5).unwrap().is_empty());
        let fresh = fresh_preview(&t[0].id).unwrap();
        let p = std::env::temp_dir().join("wavelet-test-preview.mp3");
        download(&fresh, &p).unwrap();
        let cover = http_get(&t[0].cover).unwrap();
        assert!(image::load_from_memory(&cover).is_ok(), "cover art decodes");
        println!("found {} / cover {} bytes", t[0].title, cover.len());
    }

    /// Real network: wrong key gives the friendly message; a song file downloads.
    #[test]
    #[ignore]
    fn live_service() {
        assert!(search("abc123", "chill", 1).unwrap_err().contains("isn't valid"));
        let p = std::env::temp_dir().join("wavelet-test-song.mp3");
        download("https://prod-1.storage.jamendo.com/?trackid=1532771&format=mp32", &p).unwrap();
        assert!(std::fs::metadata(&p).unwrap().len() > 100_000);
        // and the app's player can play it
        let o = OnlineTrack { id: "1532771".into(), title: "t".into(), artist: "a".into(), album: "b".into(), duration: 0.0, audio: String::new(), service: "jamendo".into(), cover: String::new() };
        let mut t = crate::library::online_to_track(&o, &std::env::temp_dir());
        t.source = crate::library::TrackSource::Online { url: String::new(), cache: p.clone() };
        let mut pl = crate::player::Player::new(true);
        pl.set_volume(0.0);
        pl.load(&t).unwrap();
        pl.play();
        std::thread::sleep(std::time::Duration::from_millis(600));
        let pos = pl.position();
        println!("downloaded song plays: position {pos:.2}s, length {:.0}s", pl.duration());
        assert!(pos > 0.2);
    }
}

#[cfg(test)]
mod stream_probe {
    /// Real network: a search works and a song starts playing before it has fully downloaded.
    #[test]
    #[ignore]
    fn live_stream() {
        let found = super::catalog_search("songs of yellow", 5).expect("search works");
        println!("search 'songs of yellow' -> {} songs, first: {} by {}", found.len(), found[0].title, found[0].artist);
        let full = super::full_search("yellow", 5).expect("full search works");
        println!("full songs for 'yellow' -> {}", full.len());
        let trend = super::trending_full(3).expect("trending works");
        let t = &trend[0];
        let cache = std::env::temp_dir().join(format!("wavelet-stream-{}.mp3", t.id));
        let _ = std::fs::remove_file(&cache);
        let st = super::start_stream(t.audio.clone(), None, cache.clone());
        let t0 = std::time::Instant::now();
        while !st.ready() && t0.elapsed().as_secs() < 60 {
            assert!(st.failed().is_none(), "{:?}", st.failed());
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        println!("'{}' ready to play after {:?} ({} KB of {} KB)", t.title, t0.elapsed(), st.got.load(std::sync::atomic::Ordering::SeqCst) / 1024, st.total.load(std::sync::atomic::Ordering::SeqCst) / 1024);
        assert!(st.ready());
        let track = crate::library::online_to_track(t, &std::env::temp_dir());
        let mut pl = crate::player::Player::new(true);
        println!("sound device found: {}", pl.has_device());
        pl.set_volume(0.3);
        pl.load_stream(&track, &st).expect("song loads");
        pl.play();
        std::thread::sleep(std::time::Duration::from_millis(2500));
        let pos = pl.position();
        println!("playing: position {pos:.2}s");
        assert!(pos > 1.0, "song should be playing");
        pl.stop();
    }
}

#[cfg(test)]
mod cover_probe {
    #[test]
    #[ignore]
    fn covers_decode() {
        for t in super::top_hits(8).unwrap() {
            let t0 = std::time::Instant::now();
            let r = super::http_get(&t.cover).map_err(|e| e).and_then(|b| image::load_from_memory(&b).map(|i| i.width()).map_err(|e| e.to_string()));
            println!("{:<28} {:?} in {:?}", t.title, r, t0.elapsed());
        }
    }
}
