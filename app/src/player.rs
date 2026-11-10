use crate::library::{Track, TrackSource};
use crate::synth::SynthSource;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::fs::File;
use std::io::BufReader;
use std::time::{Duration, Instant};

/// Audio output. Falls back to a silent clock when no sound device exists
/// (or in screenshot mode) so the rest of the app behaves the same.
pub struct Player {
    audio: bool,
    _stream: Option<OutputStream>,
    handle: Option<OutputStreamHandle>,
    sink: Option<Sink>,
    volume: f32,
    duration: f32,
    loaded: bool,
    // silent clock
    sim_offset: f32,
    sim_started: Option<Instant>,
}

impl Player {
    pub fn new(audio: bool) -> Self {
        let (stream, handle) = if audio {
            match OutputStream::try_default() {
                Ok((s, h)) => (Some(s), Some(h)),
                Err(_) => (None, None),
            }
        } else {
            (None, None)
        };
        Self {
            audio,
            _stream: stream,
            handle,
            sink: None,
            volume: 0.7,
            duration: 0.0,
            loaded: false,
            sim_offset: 0.0,
            sim_started: None,
        }
    }

    pub fn loaded(&self) -> bool {
        self.loaded
    }

    /// True when a speaker / headphones output was found.
    pub fn has_device(&self) -> bool {
        self.handle.is_some()
    }

    /// Re-open the sound output so songs always go to the current speakers or headphones
    /// (for example after Bluetooth headphones were connected while the app was open).
    fn reopen_output(&mut self) {
        if !self.audio {
            return;
        }
        self.handle = None;
        self._stream = None;
        if let Ok((s, h)) = OutputStream::try_default() {
            self._stream = Some(s);
            self.handle = Some(h);
        }
    }

    /// Play an internet song that is still downloading.
    pub fn load_stream(&mut self, track: &Track, st: &std::sync::Arc<crate::online::StreamState>) -> Result<(), String> {
        self.stop();
        self.reopen_output();
        self.duration = track.duration;
        self.sim_offset = 0.0;
        self.sim_started = None;
        if let Some(h) = &self.handle {
            let sink = Sink::try_new(h).map_err(|e| e.to_string())?;
            sink.pause();
            sink.set_volume(self.volume * self.volume);
            let rd = crate::online::StreamState::reader(st)?;
            let dec = Decoder::new(BufReader::new(rd)).map_err(|e| format!("Can't play this song: {e}"))?;
            if self.duration <= 0.0 {
                if let Some(d) = dec.total_duration() {
                    self.duration = d.as_secs_f32();
                }
            }
            sink.append(dec);
            self.sink = Some(sink);
        }
        self.loaded = true;
        Ok(())
    }

    pub fn load(&mut self, track: &Track) -> Result<(), String> {
        self.stop();
        self.reopen_output();
        self.duration = track.duration;
        self.sim_offset = 0.0;
        self.sim_started = None;
        if let Some(h) = &self.handle {
            let sink = Sink::try_new(h).map_err(|e| e.to_string())?;
            sink.pause();
            sink.set_volume(self.volume * self.volume);
            match &track.source {
                TrackSource::Demo(spec) => sink.append(SynthSource::new(spec.clone())),
                TrackSource::Online { cache, .. } => {
                    let f = File::open(cache).map_err(|_| "This song hasn't finished downloading.".to_string())?;
                    let dec = Decoder::new(BufReader::new(f)).map_err(|e| format!("Can't play this song: {e}"))?;
                    if self.duration <= 0.0 {
                        if let Some(d) = dec.total_duration() {
                            self.duration = d.as_secs_f32();
                        }
                    }
                    sink.append(dec);
                }
                TrackSource::File(path) => {
                    let f = File::open(path).map_err(|e| format!("Can't open file: {e}"))?;
                    let dec = Decoder::new(BufReader::new(f)).map_err(|e| format!("Can't play this file: {e}"))?;
                    if self.duration <= 0.0 {
                        if let Some(d) = dec.total_duration() {
                            self.duration = d.as_secs_f32();
                        }
                    }
                    sink.append(dec);
                }
            }
            self.sink = Some(sink);
        }
        self.loaded = true;
        Ok(())
    }

    pub fn duration(&self) -> f32 {
        self.duration
    }

    pub fn play(&mut self) {
        if let Some(s) = &self.sink {
            s.play();
        } else if self.loaded && self.sim_started.is_none() {
            self.sim_started = Some(Instant::now());
        }
    }

    pub fn pause(&mut self) {
        if let Some(s) = &self.sink {
            s.pause();
        } else if let Some(st) = self.sim_started.take() {
            self.sim_offset += st.elapsed().as_secs_f32();
        }
    }

    pub fn stop(&mut self) {
        if let Some(s) = self.sink.take() {
            s.stop();
        }
        self.loaded = false;
        self.sim_started = None;
        self.sim_offset = 0.0;
    }

    pub fn position(&self) -> f32 {
        if let Some(s) = &self.sink {
            s.get_pos().as_secs_f32()
        } else {
            let run = self.sim_started.map(|s| s.elapsed().as_secs_f32()).unwrap_or(0.0);
            (self.sim_offset + run).min(self.duration.max(0.0))
        }
    }

    pub fn seek(&mut self, secs: f32) {
        let secs = secs.max(0.0);
        if let Some(s) = &self.sink {
            let _ = s.try_seek(Duration::from_secs_f32(secs));
        } else {
            self.sim_offset = secs;
            if self.sim_started.is_some() {
                self.sim_started = Some(Instant::now());
            }
        }
    }

    pub fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 1.0);
        if let Some(s) = &self.sink {
            s.set_volume(self.volume * self.volume);
        }
    }

    /// True once the loaded song has played to its end.
    pub fn finished(&self) -> bool {
        if !self.loaded {
            return false;
        }
        if let Some(s) = &self.sink {
            s.empty()
        } else {
            self.sim_started.is_some() && self.duration > 0.0 && self.position() >= self.duration
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plays a demo song quietly on the real sound device for a moment.
    #[test]
    fn plays_and_seeks_on_real_device() {
        let mut p = Player::new(true);
        let t = crate::library::demo_catalog().remove(0);
        p.set_volume(0.05);
        p.load(&t).unwrap();
        p.play();
        std::thread::sleep(Duration::from_millis(700));
        let a = p.position();
        p.seek(60.0);
        std::thread::sleep(Duration::from_millis(300));
        let b = p.position();
        p.pause();
        println!("device: {}, pos after play {a:.2}s, after seek {b:.2}s", p._stream.is_some());
        assert!(a > 0.3, "position should advance while playing");
        assert!(b >= 60.0, "seek should move forward");
        assert!(!p.finished());
    }
}
