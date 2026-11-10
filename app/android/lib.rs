//! Android entry point for Wavelet. Build with `build-android.ps1` in the app folder.

#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: AndroidApp) {
    if let Some(dir) = app.internal_data_path() {
        wavelet::set_data_dir(dir.join("Wavelet"));
    }
    // Open / close the on-screen keyboard when a text box (like Search) is tapped.
    let kb = app.clone();
    wavelet::set_keyboard_hook(Box::new(move |show| {
        if show {
            kb.show_soft_input(false);
        } else {
            kb.hide_soft_input(false);
        }
    }));
    let options = eframe::NativeOptions { android_app: Some(app), ..Default::default() };
    let _ = eframe::run_native(
        "Wavelet",
        options,
        Box::new(|cc| {
            wavelet::theme::setup(&cc.egui_ctx);
            Ok(Box::new(wavelet::WaveletApp::new(None)))
        }),
    );
}
