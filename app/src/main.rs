#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use std::path::PathBuf;

/// The Wavelet logo, shown on the taskbar, the window and its shortcut.
fn window_icon() -> egui::IconData {
    const LOGO: &[u8] = include_bytes!("../android/wavelet-icon-512.png");
    match image::load_from_memory(LOGO) {
        Ok(img) => {
            let img = img.thumbnail(256, 256).to_rgba8();
            egui::IconData { rgba: img.as_raw().clone(), width: img.width(), height: img.height() }
        }
        // No picture is better than a crash at startup.
        Err(_) => egui::IconData { rgba: vec![0, 0, 0, 0], width: 1, height: 1 },
    }
}

fn main() -> eframe::Result<()> {
    // Problems Wavelet already handles (a damaged song or cover picture) stay quiet.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if !wavelet::safe::guarded() {
            default_hook(info);
        }
    }));

    let args: Vec<String> = std::env::args().collect();
    let mut screenshot: Option<PathBuf> = None;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--screenshot" && i + 1 < args.len() {
            screenshot = Some(PathBuf::from(&args[i + 1]));
            i += 1;
        }
        i += 1;
    }

    // Optional window size for testing the phone layout, e.g. WAVELET_SHOT_SIZE=412x880
    let mut size = [1280.0, 800.0];
    if let Ok(s) = std::env::var("WAVELET_SHOT_SIZE") {
        if let Some((w, h)) = s.split_once('x') {
            if let (Ok(w), Ok(h)) = (w.parse(), h.parse()) {
                size = [w, h];
            }
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Wavelet")
            .with_icon(window_icon())
            .with_inner_size(size)
            .with_min_inner_size([360.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Wavelet",
        options,
        Box::new(move |cc| {
            wavelet::theme::setup(&cc.egui_ctx);
            Ok(Box::new(wavelet::WaveletApp::new(screenshot.clone())))
        }),
    )
}
