# Update and signing keys: owner steps

One-time steps for the owner. An agent never generates, reads or handles these private keys.
Run everything on your own PC, in a folder outside the repo (for example `C:\Users\Sam\voxa-keys`),
in Git Bash (Git for Windows includes `openssl` 3 and `xxd`).

What each key does:

| Key | Signs | Lives | If lost |
|---|---|---|---|
| K1 (Ed25519) | The update feeds `desktop.json` and `android.json` | GitHub Environment `release` secret `VOXA_UPDATE_KEY`, plus your offline backup | Rotate to K2 (below) |
| K2 (Ed25519) | Nothing, until K1 is lost | Offline only (password manager plus one offline copy). Never in CI | Generate a new pair in the next release while K1 still works |
| Android release keystore | The APKs from 1.0.0 on | Offline (and later Environment secrets if CI builds Android) | Android users must uninstall once. Never lose it |
| macOS "Voxa Code Signing" certificate | The macOS app, so Accessibility access survives updates | Environment `release` secrets `MAC_SIGN_P12`, `MAC_SIGN_P12_PASSWORD`, plus offline | Every Mac user must allow Accessibility again. Never regenerate it |

Until you do steps 1 and 2, the app ships with placeholder update keys: it can never install an
update, the build prints a warning, and a tagged release build (`VOXA_RELEASE_BUILD=1` in CI, or
the Android build with `-Pvoxa.release=true`) fails on purpose (`core/voxa-update/build.rs`,
lint `update-key-present` with `python tools/lint/run.py --release`).

## 1. Generate K1 and K2

```bash
cd ~/voxa-keys
openssl genpkey -algorithm ed25519 -out voxa-update-k1.pem
openssl genpkey -algorithm ed25519 -out voxa-update-k2.pem
openssl pkey -in voxa-update-k1.pem -pubout -outform DER | tail -c 32 | xxd -p -c 32   # K1 public, 64 hex characters
openssl pkey -in voxa-update-k2.pem -pubout -outform DER | tail -c 32 | xxd -p -c 32   # K2 public, 64 hex characters
```

Paste the two public hex strings into `core/voxa-update/src/keys.rs` as `K1_HEX` and `K2_HEX`
(public keys are safe to commit). Then check:

```bash
cargo test -q --release -p voxa-update
python tools/lint/run.py --release          # update-key-present passes once both are real
```

## 2. Put K1 into a protected GitHub Environment

Why an Environment and not a repo secret: `desktop.yml` also runs on pull requests with
`contents: write`; a plain repo secret is readable by any workflow change pushed to a branch. Only
the `feed` job (desktop) and `android-feed.yml` name `environment: release`.

1. GitHub, Settings, Environments, New environment: `release`.
2. Required reviewers: yourself. Deployment branches and tags: selected tags only, `v*` and
   `android-v*`.
3. Add the secret from the file (nothing is echoed to the terminal):

```bash
gh secret set VOXA_UPDATE_KEY --env release --repo blackscythe123/voxa < voxa-update-k1.pem
```

## 3. Store K2 offline only

Put `voxa-update-k2.pem` (and a copy of `voxa-update-k1.pem`) in your password manager plus one
offline copy (a USB stick kept at home). Never put K2 in CI. Then delete the loose `.pem` files.

## 4. Protect the tags

GitHub, Settings, Rules, New tag ruleset: target `v*`, `android-v*` and `update-feed`; restrict
deletions and block force pushes.

## 5. Create the feed release once

```bash
gh release create update-feed --prerelease --latest=false --title "Update feed" --notes "Signed update feeds. Not a release."
```

A prerelease never becomes "latest", so `releases/latest/download/Voxa-Setup.exe` keeps pointing at
the desktop installer. Each workflow replaces only its own asset with `--clobber`.

## K1 leaked or lost: rotation

1. Put K2 into `VOXA_UPDATE_KEY` (as in step 2, with `voxa-update-k2.pem`) and change the feed jobs'
   `key_id` from 1 to 2.
2. Generate K3 offline (step 1).
3. Ship a release signed with K2 whose app embeds K2 and K3 (`keys.rs`).
4. Remove K1 from `keys.rs`.

Installs that have not updated yet keep trusting K1 until they do. That is accepted: an attacker
would also need to publish at the GitHub feed URL.

## 6. Android release key and the v3 lineage (B2)

Today Android APKs are signed with the debug key. The current key is backed up at
`C:\Users\Sam\voxa-signing-backup\voxa-android-current-signing-key.keystore` (the live one is
`%USERPROFILE%\.android\debug.keystore`; password `android`, alias `androiddebugkey`). Keep that
backup forever: without it, existing installs can never be updated.

The tools are in the Android SDK (`%LOCALAPPDATA%\Android\Sdk\build-tools\<version>\apksigner.bat`)
and Android Studio's JBR (`C:\Program Files\Android\Android Studio\jbr\bin\keytool.exe`).

```bash
cd ~/voxa-keys
KT="/c/Program Files/Android/Android Studio/jbr/bin/keytool.exe"
AS="$(ls -d "$LOCALAPPDATA"/Android/Sdk/build-tools/* | tail -1)/apksigner.bat"

# 1. The permanent release key (asks for a password; use a long one and store it with the keys).
"$KT" -genkeypair -keystore voxa-release.jks -alias voxa -keyalg RSA -keysize 4096 -validity 36500 -dname "CN=Voxa"

# 2. The lineage from the CURRENT debug key to the new key, so 0.3.0 installs update in place.
cp /c/Users/Sam/voxa-signing-backup/voxa-android-current-signing-key.keystore old-debug.keystore
"$AS" rotate --out voxa.lineage \
  --old-signer --ks old-debug.keystore --ks-key-alias androiddebugkey --ks-pass pass:android \
  --new-signer --ks voxa-release.jks --ks-key-alias voxa
"$AS" lineage --in voxa.lineage --print-certs        # shows both certificates, old first
```

Keep `voxa-release.jks`, its password and `voxa.lineage` offline with the other keys.

### How gradle finds the release key (never committed)

Add these lines to `android/local.properties` (already git-ignored), or set the matching
environment variables. With none of them, release builds fall back to the debug key and print a
loud warning, so 1.0.0 is signed exactly as today until you have done this step.

```properties
voxa.release.storeFile=C:/Users/Sam/voxa-keys/voxa-release.jks
voxa.release.storePassword=...
voxa.release.keyAlias=voxa
voxa.release.keyPassword=...
voxa.release.lineage=C:/Users/Sam/voxa-keys/voxa.lineage
```

Environment variables: `VOXA_RELEASE_STORE_FILE`, `VOXA_RELEASE_STORE_PASSWORD`,
`VOXA_RELEASE_KEY_ALIAS`, `VOXA_RELEASE_KEY_PASSWORD`, `VOXA_RELEASE_LINEAGE`.

Build a real release with `./gradlew assembleRelease -Pvoxa.release=true` (from `android/`, or
`android/release.ps1`, which also builds the universal APK). `-Pvoxa.release=true` makes the Rust
core refuse placeholder update keys. AGP can't sign with a lineage, so the `lineageSignRelease`
task re-signs the release APKs with `apksigner` after packaging: the debug key as the old signer,
`--next-signer` the release key, `--lineage voxa.lineage`.

Once the lineage exists, a release build signed with the debug key fails (the gradle check in
`android/app/build.gradle.kts`, and the lint `android-signing`). Check a release APK with:

```bash
"$AS" verify --print-certs --verbose app/build/outputs/apk/release/app-arm64-v8a-release.apk
```

It should list the v3 signer as the release key with the debug key in its lineage.

## 7. macOS: one stable self-signed certificate

macOS ties the Accessibility grant to the app's designated requirement. Ad hoc signing changes it
with every build, so every update drops the grant. A self-signed certificate that never changes
keeps the requirement stable, so the grant survives updates. Expect exactly one re-grant, at the
switch from ad hoc to this certificate. **Never regenerate this certificate**: a new one resets
every Mac user's Accessibility grant. Back it up with the other keys.

Create it once, on Windows, in Git Bash:

```bash
cd ~/voxa-keys
openssl req -x509 -newkey rsa:2048 -sha256 -days 3650 -nodes \
  -keyout voxa-mac-sign.key -out voxa-mac-sign.crt \
  -subj "/CN=Voxa Code Signing" \
  -addext "basicConstraints=critical,CA:FALSE" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning"

# A .p12 that macOS's `security import` reads (3DES and SHA-1 MAC: the format macOS expects).
openssl pkcs12 -export -inkey voxa-mac-sign.key -in voxa-mac-sign.crt -name "Voxa Code Signing" \
  -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1 -out voxa-mac-sign.p12
# (asks for an export password: store it with the keys)

openssl base64 -A -in voxa-mac-sign.p12 -out voxa-mac-sign.p12.b64
gh secret set MAC_SIGN_P12 --env release --repo blackscythe123/voxa < voxa-mac-sign.p12.b64
gh secret set MAC_SIGN_P12_PASSWORD --env release --repo blackscythe123/voxa   # prompts for the password
```

Store `voxa-mac-sign.p12`, its password, `.key` and `.crt` offline with the other keys, then delete
the loose files and the `.b64`.

After the first signed build, CI prints the designated requirement (`codesign -d -r- Voxa.app`).
Paste that line into `desktop/macos/designated-requirement.txt` and commit it: from then on CI
fails if the requirement ever changes (a changed requirement would reset every Mac user's grant).
The app shows "Voxa needs Accessibility access again after the update." only when the running
app's requirement differs from the one recorded at the last start and access is really off.
