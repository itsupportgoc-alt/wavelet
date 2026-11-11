@echo off
title Wavelet - build and publish
rem One click: builds the Windows + Android app, then uploads everything to GitHub.
set LOG1=%~dp0build-windows.log
set LOG2=%~dp0build-android.log
set LOG3=%~dp0publish.log
cd /d "%~dp0app"
echo.
echo [1/3] Building the Windows app...
cargo build > "%LOG1%" 2>&1 || goto fail
echo [2/3] Building the Android app - this can take 10 minutes or more...
powershell -NoProfile -ExecutionPolicy Bypass -File build-android.ps1 > "%LOG2%" 2>&1 || goto fail
echo [3/3] Uploading to GitHub...
cd /d "%~dp0"
git add -A > "%LOG3%" 2>&1
git commit -m "Wavelet 0.10.2: pages glide smoothly after a flick on the phone" >> "%LOG3%" 2>&1
git push origin main >> "%LOG3%" 2>&1 || goto fail
echo.
echo ALL DONE. The new Wavelet.apk is on GitHub. You can close this window.
pause
exit /b 0
:fail
echo.
echo Something failed. Tell Claude - it will read the log files.
pause
exit /b 1
