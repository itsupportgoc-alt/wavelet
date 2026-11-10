//! Android entry point for Wavelet. Build with `build-android.ps1` in the app folder.

#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

/// Write a line to the phone's system log (visible with `adb logcat -s Wavelet`).
#[cfg(target_os = "android")]
fn log(msg: &str) {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int};
    #[link(name = "log")]
    extern "C" {
        fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }
    let tag = CString::new("Wavelet").unwrap_or_default();
    if let Ok(text) = CString::new(msg.replace('\0', " ")) {
        unsafe {
            __android_log_write(6, tag.as_ptr(), text.as_ptr());
        }
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: AndroidApp) {
    let data = app.internal_data_path().map(|d| d.join("Wavelet"));
    if let Some(dir) = &data {
        let _ = std::fs::create_dir_all(dir);
        wavelet::set_data_dir(dir.clone());
    }

    // If the app closed unexpectedly, remember why and show it on the next start.
    let crash_file = data.as_ref().map(|d| d.join("last-crash.txt"));
    let previous = crash_file.as_ref().and_then(|f| std::fs::read_to_string(f).ok());
    if let Some(f) = &crash_file {
        let _ = std::fs::remove_file(f);
    }
    let cf = crash_file.clone();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        let msg = format!("Wavelet {} (thread {thread}): {info}", env!("CARGO_PKG_VERSION"));
        log(&msg);
        if let Some(f) = &cf {
            let _ = std::fs::write(f, &msg);
        }
    }));
    log("Wavelet starting");

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
    let result = eframe::run_native(
        "Wavelet",
        options,
        Box::new(move |cc| {
            wavelet::theme::setup(&cc.egui_ctx);
            if let Some(text) = previous {
                return Ok(Box::new(wavelet::CrashReport { text }) as Box<dyn eframe::App>);
            }
            Ok(Box::new(wavelet::WaveletApp::new(None)) as Box<dyn eframe::App>)
        }),
    );
    if let Err(e) = result {
        let msg = format!("Wavelet could not start its screen: {e}");
        log(&msg);
        if let Some(f) = &crash_file {
            let _ = std::fs::write(f, &msg);
        }
    }
}
