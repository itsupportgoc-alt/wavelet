//! Playing the audio inside an MP4 / M4A download.
//!
//! Music services send a song's audio as a *fragmented* MP4 file. The player's own decoder
//! (rodio's wrapper) gives up on those with a seek error while reading the file, even though
//! the decoder underneath it (symphonia) understands them perfectly well. So this module
//! drives symphonia directly and hands the samples to the player.

use rodio::Source;
use std::io::{Read, Seek, SeekFrom};
use std::time::Duration;
use symphonia::core::audio::{SampleBuffer, SignalSpec};
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

/// Something a song can be read from: a finished file, or a download still arriving.
pub trait AudioReader: Read + Seek + Send + Sync {}
impl<T: Read + Seek + Send + Sync> AudioReader for T {}

struct Wrapped {
    inner: Box<dyn AudioReader>,
    /// How long the file is: the MP4 reader needs it to find the song's index at the end.
    len: Option<u64>,
}

impl Read for Wrapped {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl Seek for Wrapped {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

impl MediaSource for Wrapped {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        self.len
    }
}

/// True when the first bytes say this is an MP4 / M4A file (so the audio inside it is used).
pub fn looks_like_mp4<R: Read + Seek>(r: &mut R) -> bool {
    let mut head = [0u8; 12];
    let read = r.read_exact(&mut head).is_ok();
    let _ = r.seek(SeekFrom::Start(0));
    read && &head[4..8] == b"ftyp"
}

/// One song's audio, decoded a piece at a time.
pub struct Mp4Audio {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    buffer: Option<SampleBuffer<i16>>,
    samples: Vec<i16>,
    at: usize,
    spec: Option<SignalSpec>,
    duration: Option<Duration>,
    errors: usize,
}

impl Mp4Audio {
    /// `len` is the size of the finished song. A download still arriving knows that in advance,
    /// so it can be played from its first part; a finished file can be measured instead.
    pub fn open(mut reader: Box<dyn AudioReader>, len: Option<u64>) -> Result<Self, String> {
        let len = match len {
            Some(n) if n > 0 => Some(n),
            _ => match reader.seek(SeekFrom::End(0)) {
                Ok(n) => {
                    let _ = reader.seek(SeekFrom::Start(0));
                    Some(n)
                }
                Err(_) => None,
            },
        };
        let mss = MediaSourceStream::new(Box::new(Wrapped { inner: reader, len }), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("m4a");
        let options = FormatOptions { enable_gapless: true, ..Default::default() };
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &options, &MetadataOptions::default())
            .map_err(|e| format!("can't read this song ({e})"))?;
        let format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL && t.codec_params.sample_rate.is_some())
            .ok_or_else(|| "this song has no audio in it".to_string())?;
        let track_id = track.id;
        let duration = track.codec_params.time_base.zip(track.codec_params.n_frames).map(|(b, n)| {
            let t = b.calc_time(n);
            Duration::from_secs_f64(t.seconds as f64 + t.frac)
        });
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| format!("can't play this song ({e})"))?;
        let mut song = Self {
            format,
            decoder,
            track_id,
            buffer: None,
            samples: Vec::new(),
            at: 0,
            spec: None,
            duration,
            errors: 0,
        };
        // Decode the first piece now, so the player knows the rate and channels up front.
        song.fill().map_err(|e| format!("can't play this song ({e})"))?;
        Ok(song)
    }

    /// Decode the next piece of audio into `samples`. `Ok(false)` means the song has ended.
    fn fill(&mut self) -> Result<bool, Error> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(Error::IoError(_)) | Err(Error::ResetRequired) => return Ok(false),
                Err(e) => return Err(e),
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = decoded.spec().to_owned();
                    let capacity = decoded.capacity() as u64;
                    self.buffer = Some(SampleBuffer::<i16>::new(capacity, spec));
                    self.spec = Some(spec);
                    let buffer = self.buffer.as_mut().expect("just set");
                    buffer.copy_interleaved_ref(decoded);
                    self.samples.clear();
                    self.samples.extend_from_slice(buffer.samples());
                    self.at = 0;
                    return Ok(!self.samples.is_empty());
                }
                // A damaged piece is skipped; a song that is damaged throughout just ends.
                Err(Error::DecodeError(_)) => {
                    self.errors += 1;
                    if self.errors > 8 {
                        return Ok(false);
                    }
                }
                Err(Error::IoError(_)) => return Ok(false),
                Err(e) => return Err(e),
            }
        }
    }
}

impl Iterator for Mp4Audio {
    type Item = i16;

    fn next(&mut self) -> Option<i16> {
        loop {
            if self.at < self.samples.len() {
                let v = self.samples[self.at];
                self.at += 1;
                return Some(v);
            }
            match self.fill() {
                Ok(true) => continue,
                _ => return None,
            }
        }
    }
}

impl Source for Mp4Audio {
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.samples.len().saturating_sub(self.at))
    }

    fn channels(&self) -> u16 {
        self.spec.map(|s| s.channels.count() as u16).unwrap_or(2).max(1)
    }

    fn sample_rate(&self) -> u32 {
        self.spec.map(|s| s.rate).unwrap_or(44100).max(1)
    }

    fn total_duration(&self) -> Option<Duration> {
        self.duration
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        let to = SeekTo::Time { time: Time::from(pos), track_id: Some(self.track_id) };
        match self.format.seek(SeekMode::Accurate, to) {
            Ok(_) => {
                // Whatever was decoded belongs to the old position.
                self.at = self.samples.len();
                self.errors = 0;
                Ok(())
            }
            Err(_) => Err(rodio::source::SeekError::NotSupported { underlying_source: "this song" }),
        }
    }
}
