# Builds the Android release APKs the owner uploads by hand (spec 9.2), and prints the publish
# steps. It never publishes anything itself.
#
#   powershell -File android/release.ps1
#
# Needs the release key in android/local.properties (voxa.release.* keys, docs/update-keys.md).
# Without it the build falls back to the debug key with a loud warning; never publish that once
# the lineage exists (the build then refuses anyway).
#
# Output (android/build/release/):
#   Voxa-Android-<ver>-arm64-v8a.apk   phones (per ABI, smallest)
#   Voxa-Android-<ver>-universal.apk   every other phone (arm64, 32-bit ARM, x86_64)

$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $here
if (-not $env:JAVA_HOME) { $env:JAVA_HOME = "C:/Program Files/Android/Android Studio/jbr" }
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

$gradle = Get-Content "app/build.gradle.kts" -Raw
$ver = [regex]::Match($gradle, 'versionName = "([0-9]+\.[0-9]+\.[0-9]+)"').Groups[1].Value
if (-not $ver) { throw "versionName not found in app/build.gradle.kts" }
$out = Join-Path $here "build/release"
New-Item -ItemType Directory -Force $out | Out-Null

# -Pvoxa.release=true: the Rust core refuses to build while the update keys are placeholders.
./gradlew assembleRelease "-Pvoxa.release=true"
if ($LASTEXITCODE -ne 0) { throw "per ABI build failed" }
Copy-Item "app/build/outputs/apk/release/app-arm64-v8a-release.apk" (Join-Path $out "Voxa-Android-$ver-arm64-v8a.apk") -Force

./gradlew assembleRelease "-Pvoxa.release=true" -Puniversal
if ($LASTEXITCODE -ne 0) { throw "universal build failed" }
Copy-Item "app/build/outputs/apk/release/app-release.apk" (Join-Path $out "Voxa-Android-$ver-universal.apk") -Force

python ../tools/lint/run.py --require-apk --apk (Join-Path $out "Voxa-Android-$ver-arm64-v8a.apk")
if ($LASTEXITCODE -ne 0) { throw "guardrails failed (size budget?)" }

Write-Host ""
Write-Host "Built Voxa for Android $ver in $out. To publish (yourself):"
Write-Host "  gh release create android-v$ver $out/Voxa-Android-$ver-arm64-v8a.apk $out/Voxa-Android-$ver-universal.apk --latest=false --title `"Voxa for Android $ver`""
Write-Host "  then run the android-feed workflow (Actions, android-feed, tag android-v$ver) to sign and publish android.json."
