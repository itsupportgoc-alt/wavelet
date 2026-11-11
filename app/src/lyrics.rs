//! The words of a song, found by artist and title on LRCLIB (lrclib.net), a free public lyrics
//! service that needs no key. When it has timed ("synced") words the app follows the song and
//! highlights the line being sung; otherwise the plain words are shown.

use serde::Deserialize;

/// Where the words came from, shown under them.
pub const SERVICE: &str = "LRCLIB";

const SEARCH: &str = "https://lrclib.net/api/search";
/// The service asks apps to say who they are.
const UA: &str = "Wavelet/0.10 (music player; https://github.com/itsupportgoc-alt/wavelet)";

const NOTHING: &str = "The words for this song aren't available.";

/// The words of a song, with times when the service supplied them.
#[derive(Clone, Debug, Default)]
pub struct Lyrics {
    /// True when every line carries a time, so the line being sung can be followed.
    pub timed: bool,
    /// Each line, with its time in seconds (0.0 for every line when the words aren't timed).
    pub lines: Vec<(f32, String)>,
}

impl Lyrics {
    /// The line being sung at `pos` seconds, if the words are timed.
    pub fn index_at(&self, pos: f32) -> Option<usize> {
        if !self.timed || self.lines.is_empty() {
            return None;
        }
        let mut found = None;
        for (i, (t, _)) in self.lines.iter().enumerate() {
            if *t <= pos + 0.15 {
                found = Some(i);
            } else {
                break;
            }
        }
        found
    }
}

/// Find the words for a song. Nothing is made up: when the service has nothing, it says so.
pub fn fetch(artist: &str, title: &str, duration: f32) -> Result<Lyrics, String> {
    if title.trim().is_empty() || artist.trim().is_empty() {
        return Err(NOTHING.into());
    }
    // Tags like "(Official Video)" or "feat. Someone" stop a match, so the plain titles are tried
    // too, and so is the main artist of a collaboration.
    let mut tries: Vec<(String, String)> = vec![(artist.trim().to_string(), title.trim().to_string())];
    let plain = clean_title(title);
    let main = main_artist(artist);
    if !plain.eq_ignore_ascii_case(title.trim()) {
        tries.push((artist.trim().to_string(), plain.clone()));
    }
    if main != artist.trim() {
        tries.push((main.clone(), title.trim().to_string()));
        if !plain.eq_ignore_ascii_case(title.trim()) {
            tries.push((main, plain));
        }
    }

    let mut best: Option<(i32, Lyrics)> = None;
    for (a, t) in tries {
        let url = format!("{SEARCH}?artist_name={}&track_name={}", encode(&a), encode(&t));
        let Ok(body) = crate::online::http_get_with_ua(&url, UA) else { continue };
        let Ok(hits) = serde_json::from_slice::<Vec<Hit>>(&body) else { continue };
        for hit in hits {
            if hit.instrumental {
                continue;
            }
            let Some(found) = from_hit(&hit) else { continue };
            let mut score = if found.timed { 100 } else { 50 };
            // The right recording lasts about as long as the one playing.
            if duration > 0.0 && hit.duration > 0.0 {
                let gap = (hit.duration - duration).abs();
                score += if gap < 3.0 {
                    40
                } else if gap < 10.0 {
                    20
                } else if gap > 45.0 {
                    -40
                } else {
                    0
                };
            }
            if score > best.as_ref().map(|b| b.0).unwrap_or(i32::MIN) {
                best = Some((score, found));
            }
        }
        // Timed words that match the song closely: good enough.
        if best.as_ref().map(|b| b.0).unwrap_or(0) >= 140 {
            break;
        }
    }
    best.map(|(_, l)| l).ok_or_else(|| NOTHING.to_string())
}

#[derive(Deserialize)]
struct Hit {
    #[serde(default)]
    duration: f32,
    #[serde(default)]
    instrumental: bool,
    #[serde(default, rename = "plainLyrics")]
    plain: Option<String>,
    #[serde(default, rename = "syncedLyrics")]
    synced: Option<String>,
}

fn from_hit(hit: &Hit) -> Option<Lyrics> {
    if let Some(s) = hit.synced.as_deref() {
        let lines = parse_lrc(s);
        if !lines.is_empty() {
            return Some(Lyrics { timed: true, lines });
        }
    }
    if let Some(p) = hit.plain.as_deref() {
        let lines: Vec<(f32, String)> = p
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .map(|l| (0.0, l.to_string()))
            .collect();
        if !lines.is_empty() {
            return Some(Lyrics { timed: false, lines });
        }
    }
    None
}

/// Read timed words: "[mm:ss.xx] a line", sometimes with several times for one line.
fn parse_lrc(text: &str) -> Vec<(f32, String)> {
    let mut out: Vec<(f32, String)> = Vec::new();
    for raw in text.lines() {
        let mut rest = raw.trim_start();
        let mut times = Vec::new();
        while let Some(after) = rest.strip_prefix('[') {
            let Some(end) = after.find(']') else { break };
            match parse_stamp(&after[..end]) {
                Some(t) => {
                    times.push(t);
                    rest = &after[end + 1..];
                }
                None => break,
            }
        }
        let words = rest.trim().to_string();
        if times.is_empty() || words.is_empty() {
            continue;
        }
        for t in times {
            out.push((t, words.clone()));
        }
    }
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// "01:02.50" (or "01:02:50") as seconds.
fn parse_stamp(stamp: &str) -> Option<f32> {
    let (minutes, rest) = stamp.split_once(':')?;
    let minutes: f32 = minutes.trim().parse().ok()?;
    let (seconds, frac) = match rest.split_once('.') {
        Some((a, b)) => (a, b),
        None => match rest.split_once(':') {
            Some((a, b)) => (a, b),
            None => (rest, ""),
        },
    };
    let seconds: f32 = seconds.trim().parse().ok()?;
    let frac = frac.trim();
    let part: f32 = if frac.is_empty() {
        0.0
    } else {
        let n: f32 = frac.parse().ok()?;
        n / 10f32.powi(frac.len() as i32)
    };
    Some(minutes * 60.0 + seconds + part)
}

/// "Perfect (Official Video)" -> "Perfect"; "Song - Remastered 2011" -> "Song".
pub fn clean_title(title: &str) -> String {
    let mut out = String::new();
    let mut depth: i32 = 0;
    for c in title.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth > 0 => {}
            _ => out.push(c),
        }
    }
    let mut s = out.as_str();
    let cuts = [" - ", " – ", " feat.", " ft.", " featuring ", " with "];
    if let Some(i) = cuts.iter().filter_map(|c| s.find(c)).min() {
        s = &s[..i];
    }
    s.trim().to_string()
}

/// "Calvin Harris & Dua Lipa" -> "Calvin Harris".
fn main_artist(artist: &str) -> String {
    let first = artist
        .split(|c| c == '&' || c == ',' || c == '/' || c == ';')
        .next()
        .unwrap_or(artist)
        .trim();
    // "X feat. Y" keeps only X.
    let cut = [" feat.", " ft.", " featuring "]
        .iter()
        .filter_map(|c| first.to_ascii_lowercase().find(c))
        .min();
    match cut {
        Some(i) => first[..i].trim().to_string(),
        None => first.to_string(),
    }
}

fn encode(s: &str) -> String {
    let mut o = String::new();
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => o.push(*b as char),
            _ => o.push_str(&format!("%{b:02X}")),
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_timed_words() {
        let lines = parse_lrc("[00:12.34] First line\n[00:15.00]Second\n[01:02.50][01:30.00] Chorus\n\n");
        assert_eq!(lines.len(), 4, "{lines:?}");
        assert!((lines[0].0 - 12.34).abs() < 0.01);
        assert_eq!(lines[0].1, "First line");
        assert_eq!(lines[1].1, "Second");
        assert!((lines[2].0 - 62.5).abs() < 0.01);
        assert!((lines[3].0 - 90.0).abs() < 0.01, "the repeat is its own line");
    }

    #[test]
    fn follows_the_line_being_sung() {
        let l = Lyrics { timed: true, lines: vec![(10.0, "a".into()), (20.0, "b".into())] };
        assert_eq!(l.index_at(2.0), None, "before the singing starts");
        assert_eq!(l.index_at(10.4), Some(0));
        assert_eq!(l.index_at(19.8), Some(0));
        // The next line lights up a moment early, so the words lead the singing.
        assert_eq!(l.index_at(19.9), Some(1));
        assert_eq!(l.index_at(25.0), Some(1));
        let plain = Lyrics { timed: false, lines: vec![(0.0, "a".into())] };
        assert_eq!(plain.index_at(5.0), None, "untimed words are never followed");
    }

    #[test]
    fn tidies_song_names_for_looking_up() {
        assert_eq!(clean_title("Perfect (Official Video)"), "Perfect");
        assert_eq!(clean_title("Blinding Lights - Remastered 2020"), "Blinding Lights");
        assert_eq!(clean_title("Stay feat. Justin Bieber"), "Stay");
        assert_eq!(main_artist("Calvin Harris & Dua Lipa"), "Calvin Harris");
        assert_eq!(main_artist("Ed Sheeran"), "Ed Sheeran");
    }

    /// Real network: a very well-known song has words, and they are timed.
    #[test]
    #[ignore]
    fn live_lyrics_for_a_known_song() {
        let l = fetch("Ed Sheeran", "Perfect", 263.0).expect("the words");
        println!("{} lines, timed: {}", l.lines.len(), l.timed);
        if l.timed {
            println!("first: {:.1}s {:?}", l.lines[0].0, l.lines[0].1);
            println!("at 60s: {:?}", l.index_at(60.0).map(|i| &l.lines[i].1));
        }
        assert!(l.lines.len() > 10, "a whole song of words");
    }
}
