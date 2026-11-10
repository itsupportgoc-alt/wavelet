@echo off
rem Uploads Wavelet (this whole folder) to https://github.com/itsupportgoc-alt/wavelet
cd /d "%~dp0"
echo ==== push to GitHub ==== > push-to-github.log
git add -A >> push-to-github.log 2>&1
git commit -m "Update Wavelet" >> push-to-github.log 2>&1
git push origin main >> push-to-github.log 2>&1
if errorlevel 1 (echo PUSH FAILED >> push-to-github.log & echo Push failed - see push-to-github.log) else (echo PUSHED OK >> push-to-github.log & echo Pushed to GitHub.)
pause
