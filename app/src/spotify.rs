//! Spotify's real catalogue, read the way Spotify's own web player does.
//!
//! There is no developer key and no account here. The web player asks for an
//! anonymous access token, proving itself with a one-time code that is
//! generated from a secret shipped in its own JavaScript bundle; the token then
//! reads search results and public playlists. This is the same route the
//! `SpotAPI` project documents, re-implemented in Rust so it also runs on the
//! phone, where a Python sidecar could not.
//!
//! The audio never comes from here: a song found this way is played through the
//! app's existing whole-song resolution, exactly like any other catalogue song.

use crate::online::OnlineTrack;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const HOME: &str = "https://open.spotify.com/";
const TOKEN: &str = "https://open.spotify.com/api/token";
const QUERY: &str = "https://api-partner.spotify.com/pathfinder/v2/query";

/// Playlists that are the same for everyone: Spotify's own charts.
pub const TODAYS_TOP_HITS: &str = "37i9dQZF1DXcBWIGoYBM5M";
pub const TOP_50_GLOBAL: &str = "37i9dQZEVXbMDoHDwVN2tF";

/// Hashes of Spotify's stored queries. Spotify re-issues them with new builds,
/// so they are only a starting point: [`hashes`] looks for the current ones in
/// the web player's own bundle and prefers those.
const SEARCH_HASH: &str = "db61238974d27839a136c9dc02bfdbe3fab7635f21cf85976ebff9a1ee281345";
const PLAYLIST_HASH: &str = "21ea11a6ac75a5f3e01fd365d29c18e0a4a8b7dc09f89bbb5d73910190c6acaf";

const BUSY: &str = "Spotify is busy right now. Try again in a moment.";

/// A song as Spotify describes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Song {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub secs: f32,
}

/// The token, the query hashes and the bundle they came from, kept for the
/// length of the run so a search costs one request, not four.
#[derive(Default)]
struct Kept {
    token: String,
    /// Milliseconds since the epoch; 0 means "no token yet".
    expires: u64,
    hashes: HashMap<String, String>,
}

static KEPT: Mutex<Option<Kept>> = Mutex::new(None);

fn kept<T>(f: impl FnOnce(&mut Kept) -> T) -> T {
    let mut slot = match KEPT.lock() {
        Ok(s) => s,
        Err(broken) => broken.into_inner(),
    };
    let k = slot.get_or_insert_with(Kept::default);
    f(k)
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------- the one-time code

/// SHA-1, written out here so the app needs no extra dependency for it.
fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for i in 0..5 {
        out[i * 4..i * 4 + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    out
}

fn hmac_sha1(key: &[u8], msg: &[u8]) -> [u8; 20] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..20].copy_from_slice(&sha1(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Vec::with_capacity(64 + msg.len());
    inner.extend(k.iter().map(|b| b ^ 0x36));
    inner.extend_from_slice(msg);
    let step = sha1(&inner);
    let mut outer = Vec::with_capacity(84);
    outer.extend(k.iter().map(|b| b ^ 0x5C));
    outer.extend_from_slice(&step);
    sha1(&outer)
}

/// The secret in the bundle is scrambled: each character is replaced by the
/// digits of itself exclusive-or'd with a position-dependent number.
fn unscramble(secret: &str) -> Vec<u8> {
    let mut out = String::new();
    for (i, c) in secret.chars().enumerate() {
        out.push_str(&((c as u32) ^ (i as u32 % 33 + 9)).to_string());
    }
    out.into_bytes()
}

/// A six-digit code for this moment, from a key (standard TOTP, 30-second step).
fn code(key: &[u8], at_secs: u64) -> String {
    let counter = at_secs / 30;
    let digest = hmac_sha1(key, &counter.to_be_bytes());
    let offset = (digest[19] & 0x0F) as usize;
    let part = u32::from_be_bytes([digest[offset], digest[offset + 1], digest[offset + 2], digest[offset + 3]]);
    format!("{:06}", (part & 0x7FFF_FFFF) % 1_000_000)
}

// ---------------------------------------------------------------- reading the bundle

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Every web-player script named by the front page.
fn bundle_urls(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let needle = "https://open.spotifycdn.com/cdn/build/web-player/web-player.";
    let mut at = 0;
    while let Some(i) = html[at..].find(needle) {
        let start = at + i;
        let end = html[start..].find(".js").map(|e| start + e + 3).unwrap_or(0);
        if end > start {
            let url = &html[start..end];
            let name = &url[needle.len()..];
            if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.') && !out.iter().any(|u: &String| u == url) {
                out.push(url.to_string());
            }
            at = end;
        } else {
            at = start + needle.len();
        }
    }
    out
}

/// The newest scrambled secret and its version number inside a bundle.
fn secret_in(src: &[u8]) -> Option<(String, u32)> {
    let mut best: Option<(String, u32)> = None;
    let mut at = 0;
    while let Some(i) = find(&src[at..], b"secret") {
        let start = at + i + b"secret".len();
        at = start;
        let mut p = start;
        while p < src.len() && src[p].is_ascii_whitespace() {
            p += 1;
        }
        if p < src.len() && src[p] == b':' {
            p += 1;
        }
        while p < src.len() && src[p].is_ascii_whitespace() {
            p += 1;
        }
        if p >= src.len() || !matches!(src[p], b'"' | b'\'') {
            continue;
        }
        let quote = src[p];
        let Some(len) = src[p + 1..].iter().position(|b| *b == quote) else { continue };
        let secret = text(&src[p + 1..p + 1 + len]);
        let tail_start = p + 1 + len;
        let Some(v) = find(&src[tail_start..], b"version") else { continue };
        let mut d = tail_start + v + b"version".len();
        while d < src.len() && !src[d].is_ascii_digit() {
            if d - (tail_start + v) > 24 {
                break;
            }
            d += 1;
        }
        let end = d + src[d.min(src.len())..].iter().take_while(|b| b.is_ascii_digit()).count();
        if let Ok(ver) = text(&src[d..end]).parse::<u32>() {
            if best.as_ref().map_or(true, |b| ver > b.1) {
                best = Some((secret, ver));
            }
        }
    }
    best
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|i| &hay[*i..*i + needle.len()] == needle)
}

/// The hash of a stored query, from `new X.Y("name","query","<hash>",…)`.
fn hash_in(src: &[u8], name: &str) -> Option<String> {
    let mut want = Vec::new();
    want.extend_from_slice(b"\"");
    want.extend_from_slice(name.as_bytes());
    want.extend_from_slice(b"\",\"query\",\"");
    let at = find(src, &want)? + want.len();
    let hex: String = text(&src[at..at.saturating_add(64).min(src.len())]);
    if hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(hex)
    } else {
        None
    }
}

// ---------------------------------------------------------------- talking to Spotify

#[cfg(not(target_os = "android"))]
fn send(url: &str, headers: &[(&str, String)], body: Option<&[u8]>, wait: u32) -> Result<Vec<u8>, String> {
    let mut cmd = std::process::Command::new("curl");
    cmd.args(["-sSL", "--fail", "--compressed", "--connect-timeout", "8", "--max-time", &wait.to_string()]);
    for (k, v) in headers {
        cmd.arg("-H").arg(format!("{k}: {v}"));
    }
    if body.is_some() {
        cmd.args(["--data-binary", "@-"]);
    }
    cmd.arg(url);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // no console window
    }
    if body.is_some() {
        cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
        let mut child = cmd.spawn().map_err(|_| "Couldn't start the download tool (curl).".to_string())?;
        if let Some(mut into) = child.stdin.take() {
            use std::io::Write;
            let _ = into.write_all(body.unwrap_or_default());
        }
        let out = child.wait_with_output().map_err(|_| "Couldn't start the download tool (curl).".to_string())?;
        if !out.status.success() {
            return Err(BUSY.into());
        }
        Ok(out.stdout)
    } else {
        let out = cmd.output().map_err(|_| "Couldn't start the download tool (curl).".to_string())?;
        if !out.status.success() {
            return Err(BUSY.into());
        }
        Ok(out.stdout)
    }
}

#[cfg(target_os = "android")]
fn send(url: &str, headers: &[(&str, String)], body: Option<&[u8]>, wait: u32) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(10))
        .timeout_read(std::time::Duration::from_secs(wait as u64))
        .redirects(8)
        .build();
    let req = match body {
        Some(b) => agent.post(url).set("Content-Length", &b.len().to_string()).send_bytes(b),
        None => agent.get(url).call(),
    };
    let _ = headers; // Android's agent sends its own; Spotify only needs the token here.
    let resp = req.map_err(|_| BUSY.to_string())?;
    let mut buf = Vec::new();
    resp.into_reader().read_to_end(&mut buf).map_err(|_| BUSY.to_string())?;
    Ok(buf)
}

fn browser_headers() -> Vec<(&'static str, String)> {
    vec![
        ("User-Agent", UA.to_string()),
        ("Accept-Language", "en-US,en;q=0.9".to_string()),
        ("Referer", HOME.to_string()),
        ("Origin", "https://open.spotify.com".to_string()),
        ("App-Platform", "WebPlayer".to_string()),
    ]
}

/// Read the bundle once: the secret for a token, and the current query hashes.
fn learn(want: &[&str]) -> Result<(), String> {
    let html = text(&send(HOME, &browser_headers(), None, 20)?);
    let mut src = Vec::new();
    for url in bundle_urls(&html) {
        let mut h = browser_headers();
        h.push(("Accept", "*/*".to_string()));
        if let Ok(b) = send(&url, &h, None, 30) {
            src.extend_from_slice(&b);
        }
    }
    let (secret, version) = secret_in(&src).ok_or_else(|| BUSY.to_string())?;
    let key = unscramble(&secret);
    let one_time = code(&key, now_millis() / 1000);
    let url = format!("{TOKEN}?reason=init&productType=web-player&totp={one_time}&totpServer={one_time}&totpVer={version}");
    let reply: Value = serde_json::from_slice(&send(&url, &browser_headers(), None, 20)?).map_err(|_| BUSY.to_string())?;
    let token = reply["accessToken"].as_str().unwrap_or("").to_string();
    if token.is_empty() {
        return Err(BUSY.into());
    }
    let expires = reply["accessTokenExpirationTimestampMs"].as_u64().unwrap_or(0);
    let found: Vec<(String, String)> = want.iter().filter_map(|n| hash_in(&src, n).map(|h| (n.to_string(), h))).collect();
    kept(|k| {
        k.token = token;
        k.expires = expires;
        for (n, h) in found {
            k.hashes.insert(n, h);
        }
    });
    Ok(())
}

/// A live access token, learning one first if there is none or it has expired.
fn token(want: &[&str]) -> Result<String, String> {
    let (token, fresh) = kept(|k| {
        let fresh = !k.token.is_empty() && k.expires > now_millis() + 30_000;
        let missing = want.iter().any(|n| !k.hashes.contains_key(*n));
        (k.token.clone(), fresh && !missing)
    });
    if fresh {
        return Ok(token);
    }
    learn(want)?;
    let token = kept(|k| k.token.clone());
    if token.is_empty() {
        Err(BUSY.into())
    } else {
        Ok(token)
    }
}

fn run(op: &str, fallback: &str, variables: Value) -> Result<Value, String> {
    let hash = token(&[op]).ok().and_then(|_| kept(|k| k.hashes.get(op).cloned())).unwrap_or_else(|| fallback.to_string());
    let token = token(&[op])?;
    let body = serde_json::json!({
        "variables": variables,
        "operationName": op,
        "extensions": { "persistedQuery": { "version": 1, "sha256Hash": hash } }
    })
    .to_string();
    let mut h = browser_headers();
    h.push(("Authorization", format!("Bearer {token}")));
    h.push(("Content-Type", "application/json;charset=UTF-8".to_string()));
    h.push(("Accept", "application/json".to_string()));
    let reply: Value = serde_json::from_slice(&send(QUERY, &h, Some(body.as_bytes()), 25)?).map_err(|_| BUSY.to_string())?;
    if reply["errors"].is_array() || reply["data"].is_null() {
        // The stored query may have been re-issued since this build: learn again next time.
        kept(|k| k.hashes.remove(op));
        return Err(BUSY.into());
    }
    Ok(reply["data"].clone())
}

// ---------------------------------------------------------------- songs out of the reply

fn song_from(node: &Value) -> Option<Song> {
    let uri = node["uri"].as_str().unwrap_or("");
    // Podcasts and other non-songs are listed next to the songs; only songs play here.
    let id = match uri.strip_prefix("spotify:track:") {
        Some(id) if !id.is_empty() => id,
        _ if uri.is_empty() => node["id"].as_str()?,
        _ => return None,
    };
    let title = node["name"].as_str().unwrap_or("").to_string();
    if title.is_empty() {
        return None;
    }
    let mut names: Vec<String> = Vec::new();
    let none: Vec<Value> = Vec::new();
    for a in node["artists"]["items"].as_array().unwrap_or(&none) {
        let n = a["profile"]["name"].as_str().or(a["name"].as_str()).unwrap_or("");
        if !n.is_empty() {
            names.push(n.to_string());
        }
    }
    let album = node["albumOfTrack"]["name"].as_str().unwrap_or("").to_string();
    let ms = node["duration"]["totalMilliseconds"]
        .as_u64()
        .or(node["trackDuration"]["totalMilliseconds"].as_u64())
        .unwrap_or(0);
    Some(Song {
        id: id.to_string(),
        title,
        artist: if names.is_empty() { "Spotify".into() } else { names.join(", ") },
        album,
        secs: ms as f32 / 1000.0,
    })
}

/// Every playable song in a list of wrapped items, in order.
fn songs_in(items: &Value) -> Vec<Song> {
    let mut out = Vec::new();
    let none: Vec<Value> = Vec::new();
    for it in items.as_array().unwrap_or(&none) {
        let node = &it["itemV2"]["data"];
        let node = if node.is_null() { &it["item"]["data"] } else { node };
        if let Some(s) = song_from(node) {
            if !out.iter().any(|x: &Song| x.id == s.id) {
                out.push(s);
            }
        }
    }
    out
}

fn as_online(s: &Song) -> OnlineTrack {
    OnlineTrack {
        id: s.id.clone(),
        title: s.title.clone(),
        artist: s.artist.clone(),
        album: if s.album.is_empty() { "Spotify".into() } else { s.album.clone() },
        duration: s.secs,
        // The song itself is fetched when it is played, like any catalogue song.
        audio: String::new(),
        service: "spotify".into(),
        cover: String::new(),
    }
}

/// Songs Spotify knows for this search, most relevant first.
pub fn search(query: &str, limit: u32) -> Result<Vec<OnlineTrack>, String> {
    let data = run(
        "searchDesktop",
        SEARCH_HASH,
        serde_json::json!({
            "searchTerm": query,
            "offset": 0,
            "limit": limit.clamp(1, 50),
            "numberOfTopResults": limit.clamp(1, 50),
            "includeAudiobooks": false,
            "includeArtistHasConcertsField": false,
            "includePreReleases": false,
            "includeLocalConcertsField": false,
            "includeAuthors": false
        }),
    )?;
    let songs = songs_in(&data["searchV2"]["tracksV2"]["items"]);
    if songs.is_empty() {
        return Err("Spotify found no songs for that.".into());
    }
    Ok(songs.iter().map(as_online).collect())
}

/// The songs of one of Spotify's playlists, in playlist order.
pub fn playlist(id: &str, limit: u32) -> Result<Vec<OnlineTrack>, String> {
    let data = run(
        "fetchPlaylistContents",
        PLAYLIST_HASH,
        serde_json::json!({
            "uri": format!("spotify:playlist:{id}"),
            "offset": 0,
            "limit": limit.clamp(1, 100),
            "enableWatchFeedEntrypoint": false,
            "includeEpisodeContentRatingsV2": true
        }),
    )?;
    let songs = songs_in(&data["playlistV2"]["content"]["items"]);
    if songs.is_empty() {
        return Err(BUSY.into());
    }
    Ok(songs.iter().map(as_online).collect())
}

/// What the whole world is listening to right now: Spotify's two biggest charts,
/// in order and without repeats.
pub fn charts(limit: u32) -> Result<Vec<OnlineTrack>, String> {
    let mut out: Vec<OnlineTrack> = Vec::new();
    let mut trouble: Option<String> = None;
    for id in [TODAYS_TOP_HITS, TOP_50_GLOBAL] {
        if out.len() >= limit as usize {
            break;
        }
        match playlist(id, limit) {
            Ok(list) => {
                for t in list {
                    if out.len() >= limit as usize {
                        break;
                    }
                    let key = format!("{}|{}", t.title.to_lowercase(), t.artist.to_lowercase());
                    if !out.iter().any(|o| format!("{}|{}", o.title.to_lowercase(), o.artist.to_lowercase()) == key) {
                        out.push(t);
                    }
                }
            }
            Err(e) => trouble = trouble.or(Some(e)),
        }
    }
    if out.is_empty() {
        return Err(trouble.unwrap_or_else(|| BUSY.into()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hashing_matches_the_standard() {
        assert_eq!(format!("{:x}", Sha1Hex(&sha1(b"abc"))), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(format!("{:x}", Sha1Hex(&sha1(b""))), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }

    /// Lets a test print a digest the way every reference prints it.
    struct Sha1Hex<'a>(&'a [u8; 20]);

    impl std::fmt::LowerHex for Sha1Hex<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            for b in self.0 {
                write!(f, "{b:02x}")?;
            }
            Ok(())
        }
    }

    #[test]
    fn the_one_time_code_matches_the_standard() {
        // RFC 6238's own example key, at its own example time.
        let key = b"12345678901234567890";
        assert_eq!(code(key, 59), "287082");
        assert_eq!(code(key, 1111111109), "081804");
    }

    #[test]
    fn the_scrambled_secret_is_recovered() {
        // "abc" scrambled the way the bundle does it: 97^9=104, 98^10=104, 99^11=104.
        assert_eq!(unscramble("abc"), b"104104104".to_vec());
        assert_eq!(unscramble(""), Vec::<u8>::new());
    }

    #[test]
    fn a_bundle_is_read_for_its_secret_and_hashes() {
        let hash = "a".repeat(64);
        let src = format!(r#"x=new q.l("searchDesktop","query","{hash}",null);var s={{secret:"abc",version:61,other:1}};"#);
        assert_eq!(hash_in(src.as_bytes(), "searchDesktop"), Some(hash.clone()));
        assert_eq!(hash_in(src.as_bytes(), "notThere"), None);
        assert_eq!(secret_in(src.as_bytes()), Some(("abc".to_string(), 61)));
        // The newest version wins when a bundle carries more than one.
        let both = format!(r#"{{secret:"old",version:58}}{{secret:"new",version:61}}"#);
        assert_eq!(secret_in(both.as_bytes()), Some(("new".to_string(), 61)));
    }

    #[test]
    fn the_front_page_names_its_bundle() {
        let html = r#"<script src="https://open.spotifycdn.com/cdn/build/web-player/web-player.abc123.js"></script>"#;
        assert_eq!(bundle_urls(html), vec!["https://open.spotifycdn.com/cdn/build/web-player/web-player.abc123.js"]);
        assert!(bundle_urls("<html></html>").is_empty());
    }

    #[test]
    fn a_search_reply_becomes_songs() {
        let reply: Value = serde_json::from_str(
            r#"{"searchV2":{"tracksV2":{"items":[
              {"item":{"data":{"uri":"spotify:track:abc","name":"Perfect","duration":{"totalMilliseconds":263400},
                "artists":{"items":[{"profile":{"name":"Ed Sheeran"}}]},"albumOfTrack":{"name":"Divide"}}}},
              {"item":{"data":{"uri":"spotify:track:abc","name":"Perfect","duration":{"totalMilliseconds":263400},
                "artists":{"items":[{"profile":{"name":"Ed Sheeran"}}]}}}},
              {"item":{"data":{"uri":"spotify:episode:xyz","name":"A podcast"}}}
            ]}}}"#,
        )
        .unwrap();
        let songs = songs_in(&reply["searchV2"]["tracksV2"]["items"]);
        assert_eq!(songs.len(), 1, "repeats are dropped");
        assert_eq!(songs[0].id, "abc");
        assert_eq!(songs[0].artist, "Ed Sheeran");
        assert_eq!(songs[0].secs, 263.4);
        let online = as_online(&songs[0]);
        assert_eq!(online.track_id(), "sp:abc");
        assert!(online.is_catalogue(), "a Spotify song plays its whole length");
    }

    #[test]
    fn a_playlist_reply_becomes_songs() {
        let reply: Value = serde_json::from_str(
            r#"{"playlistV2":{"content":{"items":[
              {"itemV2":{"data":{"uri":"spotify:track:one","name":"Housewife",
                "artists":{"items":[{"profile":{"name":"Tate McRae"}}]},"trackDuration":{"totalMilliseconds":208810}}}}
            ]}}}"#,
        )
        .unwrap();
        let songs = songs_in(&reply["playlistV2"]["content"]["items"]);
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].artist, "Tate McRae");
        assert_eq!(songs[0].secs, 208.81);
    }

    /// Talks to the real Spotify.
    #[test]
    #[ignore]
    fn live_spotify_search_and_charts() {
        let hits = charts(8).expect("the charts");
        println!("charts: {:?}", hits.iter().map(|t| format!("{} — {}", t.title, t.artist)).collect::<Vec<_>>());
        assert_eq!(hits.len(), 8);
        let found = search("perfect ed sheeran", 5).expect("a search");
        println!("search: {:?}", found.iter().map(|t| format!("{} — {}", t.title, t.artist)).collect::<Vec<_>>());
        assert!(found.iter().any(|t| t.title.to_lowercase().contains("perfect")));
    }
}
