//! Keeps one bad song, cover picture or search result from closing the whole app.
//!
//! Some songs and cover pictures from the internet are damaged or unusual, and the
//! libraries that read them can stop with an "index out of bounds" error. `catch` turns
//! that into a normal error message instead, so only that one song or picture is skipped.

use std::any::Any;
use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// True while running inside `catch` on this thread. The crash recorder uses this to
/// ignore problems that were already handled (so the phone doesn't show a crash screen).
pub fn guarded() -> bool {
    DEPTH.with(|d| d.get() > 0)
}

/// Run `f`; if it fails with a panic, return the panic text as an error.
pub fn catch<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    DEPTH.with(|d| d.set(d.get() + 1));
    let r = catch_unwind(AssertUnwindSafe(f));
    DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    r.map_err(|e| panic_text(&e))
}

/// Like `catch`, for work that already returns `Result<T, String>`.
pub fn catch_res<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    match catch(f) {
        Ok(r) => r,
        Err(e) => Err(format!("Something in this result couldn't be read ({e}). Try again or pick another song.")),
    }
}

pub fn panic_text(e: &Box<dyn Any + Send>) -> String {
    if let Some(s) = e.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = e.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown error".into()
    }
}

/// A song file seen without the information block (ID3 tag: title, cover picture...) at its start.
///
/// Some songs carry a tag the sound library can't read, and it then refuses the whole song
/// with "out of bounds". The player doesn't need the tag, so it is skipped.
pub struct SkipTag<R> {
    inner: R,
    base: u64,
}

/// Skip any ID3 tags at the start of a song.
pub fn skip_tags<R: std::io::Read + std::io::Seek>(mut inner: R) -> SkipTag<R> {
    use std::io::SeekFrom;
    let mut base = 0u64;
    for _ in 0..4 {
        let mut h = [0u8; 10];
        if inner.seek(SeekFrom::Start(base)).is_err() || inner.read_exact(&mut h).is_err() {
            break;
        }
        let is_tag = &h[0..3] == b"ID3" && h[3] != 0xFF && h[4] != 0xFF && h[6..10].iter().all(|b| *b < 0x80);
        if !is_tag {
            break;
        }
        let size = ((h[6] as u64) << 21) | ((h[7] as u64) << 14) | ((h[8] as u64) << 7) | h[9] as u64;
        let footer = if h[5] & 0x10 != 0 { 10 } else { 0 };
        base += 10 + size + footer;
    }
    if inner.seek(SeekFrom::Start(base)).is_err() {
        base = 0;
        let _ = inner.seek(SeekFrom::Start(0));
    }
    SkipTag { inner, base }
}

impl<R: std::io::Read> std::io::Read for SkipTag<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<R: std::io::Seek> std::io::Seek for SkipTag<R> {
    fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
        use std::io::SeekFrom;
        let abs = match to {
            SeekFrom::Start(n) => self.inner.seek(SeekFrom::Start(n + self.base))?,
            other => self.inner.seek(other)?,
        };
        if abs < self.base {
            // Never step back into the tag.
            self.inner.seek(SeekFrom::Start(self.base))?;
            return Ok(0);
        }
        Ok(abs - self.base)
    }
}

/// Wraps a song decoder so a damaged song just ends instead of crashing the sound system.
pub struct SafeSource<S> {
    inner: S,
    dead: bool,
}

impl<S> SafeSource<S> {
    pub fn new(inner: S) -> Self {
        Self { inner, dead: false }
    }
}

impl<S> Iterator for SafeSource<S>
where
    S: rodio::Source,
    S::Item: rodio::Sample,
{
    type Item = S::Item;
    fn next(&mut self) -> Option<Self::Item> {
        if self.dead {
            return None;
        }
        match catch(|| self.inner.next()) {
            Ok(v) => v,
            Err(_) => {
                self.dead = true;
                None
            }
        }
    }
}

impl<S> rodio::Source for SafeSource<S>
where
    S: rodio::Source,
    S::Item: rodio::Sample,
{
    fn current_frame_len(&self) -> Option<usize> {
        if self.dead {
            return Some(0);
        }
        catch(|| self.inner.current_frame_len()).unwrap_or(Some(0))
    }
    fn channels(&self) -> u16 {
        catch(|| self.inner.channels()).unwrap_or(2).max(1)
    }
    fn sample_rate(&self) -> u32 {
        catch(|| self.inner.sample_rate()).unwrap_or(44100).max(1)
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        catch(|| self.inner.total_duration()).unwrap_or(None)
    }
    fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), rodio::source::SeekError> {
        if self.dead {
            return Ok(());
        }
        match catch(|| self.inner.try_seek(pos)) {
            Ok(r) => r,
            Err(_) => {
                self.dead = true;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catch_turns_index_errors_into_messages() {
        let v: Vec<u8> = vec![];
        let i = 3;
        let r = catch(|| v[i]);
        assert!(r.unwrap_err().contains("out of bounds"));
        assert!(!guarded());
        assert_eq!(catch(|| 5).unwrap(), 5);
    }

    #[test]
    fn tag_at_start_is_skipped() {
        use std::io::{Read, Seek, SeekFrom};
        // 10-byte header saying "20 bytes of tag", the tag, then the music.
        let mut data = b"ID3\x04\x00\x00\x00\x00\x00\x14".to_vec();
        data.extend([7u8; 20]);
        data.extend(b"MUSIC");
        let mut r = skip_tags(std::io::Cursor::new(data));
        let mut out = String::new();
        r.read_to_string(&mut out).unwrap();
        assert_eq!(out, "MUSIC");
        assert_eq!(r.seek(SeekFrom::Start(1)).unwrap(), 1);
        assert_eq!(r.seek(SeekFrom::End(0)).unwrap(), 5);
        // No tag: nothing is skipped.
        let mut r = skip_tags(std::io::Cursor::new(b"MUSIC".to_vec()));
        let mut out = String::new();
        r.read_to_string(&mut out).unwrap();
        assert_eq!(out, "MUSIC");
    }
}
