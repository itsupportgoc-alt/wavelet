//! Wavelet: an ad-free music player. Shared by the desktop program and the Android app.

pub mod accounts;
pub mod app;
pub mod library;
pub mod online;
pub mod player;
pub mod synth;
pub mod theme;

pub use app::{set_data_dir, set_keyboard_hook, CrashReport, WaveletApp};
