//! Wavelet: an ad-free music player. Shared by the desktop program and the Android app.

pub mod accounts;
pub mod app;
pub mod library;
pub mod lyrics;
pub mod m4a;
pub mod online;
pub mod player;
pub mod safe;
pub mod synth;
pub mod theme;

pub use app::{set_data_dir, set_keyboard_hook, CrashReport, WaveletApp};

/// Shown in the app's profile page, so it is obvious which build is installed.
pub const VERSION: &str = "0.10.1";
