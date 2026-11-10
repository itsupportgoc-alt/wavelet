@echo off
rem Rebuilds Wavelet with the sound + search fix and tests it. Results go to fix-check.log.
cd /d "%~dp0"
echo Building Wavelet... (this can take a few minutes)
echo ==== build ==== > fix-check.log
cargo build >> fix-check.log 2>&1
echo ==== live test: search + streaming sound ==== >> fix-check.log
cargo test --lib live_stream -- --ignored --nocapture >> fix-check.log 2>&1
echo ==== android build ==== >> fix-check.log
powershell -NoProfile -ExecutionPolicy Bypass -File build-android.ps1 >> fix-check.log 2>&1
echo ==== finished ==== >> fix-check.log
echo Done. You can close this window.
pause
