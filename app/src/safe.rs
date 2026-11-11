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
}
