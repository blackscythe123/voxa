# LOCAL FALLBACK ONLY. Android releases are normally built in CI: push the tag android-vX.Y.Z and
# .github/workflows/android-release.yml builds, publishes and signs the feed
# (docs/release-runbook.md B5 to B7). Use this script only when CI cannot build. It builds the
# same two APKs on the owner's PC and prints the publish steps; it never publishes anything itself.
#
#   powershell -File android/release.ps1
#
# Needs the permanent release key in android/local.properties (voxa.release.* keys,
# docs/update-keys.md) and the real update keys in core/voxa-update/src/keys.rs: without either,
# the build refuses (app/build.gradle.kts). After signing, verifyReleaseSigner checks the APKs
# against android/signing/release-cert.sha256.
#
# Output (android/build/release/), the exact names README.md links and android-release.yml expects:
#   Voxa-Android-<ver>-arm64-v8a.apk   most phones (per ABI, smallest)
#   Voxa-Android-<ver>-universal.apk   any phone (arm64, 32-bit ARM, x86_64)

$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $here
if (-not $env:JAVA_HOME) { $env:JAVA_HOME = "C:/Program Files/Android/Android Studio/jbr" }
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

# Build exactly a committed state, so the tag below names the commit that was built.
$dirty = git status --porcelain
if ($dirty) { throw "the working tree has uncommitted changes; commit or stash them first, so the release matches a commit" }
$commit = (git rev-parse HEAD).Trim()

$gradle = Get-Content "app/build.gradle.kts" -Raw
$ver = [regex]::Match($gradle, 'versionName = "([0-9]+\.[0-9]+\.[0-9]+)"').Groups[1].Value
if (-not $ver) { throw "versionName not found in app/build.gradle.kts" }
$out = Join-Path $here "build/release"
New-Item -ItemType Directory -Force $out | Out-Null

./gradlew assembleRelease
if ($LASTEXITCODE -ne 0) { throw "per ABI build failed" }
Copy-Item "app/build/outputs/apk/release/app-arm64-v8a-release.apk" (Join-Path $out "Voxa-Android-$ver-arm64-v8a.apk") -Force

./gradlew assembleRelease -Puniversal
if ($LASTEXITCODE -ne 0) { throw "universal build failed" }
Copy-Item "app/build/outputs/apk/release/app-release.apk" (Join-Path $out "Voxa-Android-$ver-universal.apk") -Force

python ../tools/lint/run.py --release --require-apk --apk (Join-Path $out "Voxa-Android-$ver-arm64-v8a.apk")
if ($LASTEXITCODE -ne 0) { throw "guardrails failed (size budget, keys or release certificate?)" }

Write-Host ""
Write-Host "Built Voxa for Android $ver from $commit in $out. The CI fallback, to publish (yourself, docs/release-runbook.md):"
Write-Host "  git push origin $commit"
Write-Host "  gh release create android-v$ver $out/Voxa-Android-$ver-arm64-v8a.apk $out/Voxa-Android-$ver-universal.apk --target $commit --prerelease --latest=false --title `"Voxa for Android $ver`""
Write-Host "  (the new tag starts android-release.yml: reject its build approval, these APKs are the release)"
Write-Host "  gh workflow run android-release.yml --ref android-v$ver -f prebuilt=true    (then approve the feed job)"
