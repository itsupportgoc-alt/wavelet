@echo off
title Wavelet - build and publish
rem One click: builds the Windows + Android app, then uploads everything to GitHub.
set LOG=%~dp0build-and-publish.log
cd /d "%~dp0app"
echo Wavelet build and publish > "%LOG%"
echo.
echo [1/4] Building the Windows app...
cargo build >> "%LOG%" 2>&1 || goto fail
echo [2/4] Testing search and sound (you may hear a few seconds of music)...
cargo test --lib live_stream -- --ignored --nocapture >> "%LOG%" 2>&1
echo [3/4] Building the Android app - this can take 10 minutes or more...
powershell -NoProfile -ExecutionPolicy Bypass -File build-android.ps1 >> "%LOG%" 2>&1 || goto fail
echo [4/4] Uploading to GitHub...
cd /d "%~dp0"
git add -A >> "%LOG%" 2>&1
git commit -m "Wavelet 0.5.1: app icon, sound and search fixes, 32-bit phones" >> "%LOG%" 2>&1
git push origin main >> "%LOG%" 2>&1 || goto fail
echo ALL DONE >> "%LOG%"
echo.
echo ALL DONE. The new Wavelet.apk is on GitHub. You can close this window.
pause
exit /b 0
:fail
echo FAILED >> "%LOG%"
echo.
echo Something failed. Tell Claude - the details are in build-and-publish.log
pause
exit /b 1
