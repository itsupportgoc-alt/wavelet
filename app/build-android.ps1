# Builds the Android app (Wavelet.apk) for 64-bit and 32-bit ARM phones.
# Uses the Android SDK/NDK and cargo-apk unpacked into target\android-tools.
# Short (8.3) paths are used because the Android tools break on folder names with spaces.

$ErrorActionPreference = "Stop"
$fso = New-Object -ComObject Scripting.FileSystemObject
$app = $fso.GetFolder($PSScriptRoot).ShortPath
$tools = "$app\target\android-tools"

$jdk = Get-ChildItem "C:\Program Files\Eclipse Adoptium" -Directory -Filter "jdk-17*" | Select-Object -First 1
if ($jdk) { $env:JAVA_HOME = $fso.GetFolder($jdk.FullName).ShortPath }
$env:ANDROID_HOME = "$tools\sdk"
$env:ANDROID_NDK_ROOT = "$tools\android-ndk-r26d"
$env:ANDROID_NDK_HOME = $env:ANDROID_NDK_ROOT
$env:CARGO_TARGET_DIR = "$app\target\android-build"
$env:PATH = "$tools\cargo\bin;$env:PATH"

rustup target add aarch64-linux-android armv7-linux-androideabi | Out-Null
Push-Location "$app\android"
try {
    cargo apk build --release --lib
    if ($LASTEXITCODE -ne 0) { throw "Android build failed" }
} finally { Pop-Location }

$apk = Get-Item "$app\target\android-build\release\apk\Wavelet.apk"
New-Item -ItemType Directory -Force "$app\dist" | Out-Null
Copy-Item $apk.FullName "$app\dist\Wavelet.apk" -Force
Write-Host "Android app ready: $PSScriptRoot\dist\Wavelet.apk"
