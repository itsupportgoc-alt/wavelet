@echo off
rem Puts the Wavelet source code on GitHub: https://github.com/itsupportgoc-alt/wavelet
cd /d "%~dp0"
echo ==== push to GitHub ==== > push-to-github.log
if not exist .git git init >> push-to-github.log 2>&1
git add -A >> push-to-github.log 2>&1
git commit -m "first commit" >> push-to-github.log 2>&1
git branch -M main >> push-to-github.log 2>&1
git remote remove origin >nul 2>&1
git remote add origin https://github.com/itsupportgoc-alt/wavelet.git >> push-to-github.log 2>&1
git push -u origin main >> push-to-github.log 2>&1
if errorlevel 1 (echo PUSH FAILED >> push-to-github.log & echo Push failed - see push-to-github.log) else (echo PUSHED OK >> push-to-github.log & echo Pushed to GitHub.)
pause
