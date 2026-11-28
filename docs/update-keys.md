# Update and signing keys: owner steps

One-time steps for the owner. An agent never generates, reads or handles these private keys.
The release order that uses them is [release-runbook.md](release-runbook.md).

**Never publish a build until the real update keys are committed.** A build with the placeholder
keys can never install an update: every friend who gets one is stuck on it and has to reinstall by
hand. That is why every release build refuses placeholder keys (below).

## Where the files live

Work in one folder outside the repo, for example `C:\Users\Sam\voxa-keys`, in Git Bash (Git for
Windows includes `openssl` 3 and `xxd`). Create it first:

```bash
mkdir -p ~/voxa-keys
```

Commands that change the repo (`cargo`, `python tools/lint/run.py`, `git`) run in the repo folder,
`C:\projects\voxa`; key commands run in `~/voxa-keys`. Each block below says which.

| Key | Signs | On the build PC | Offline backup (password manager plus one offline copy) | If lost |
|---|---|---|---|---|
| K1 (Ed25519) | The update feeds `desktop.json` and `android.json` | Nothing (CI has it as the Environment `release` secret `VOXA_UPDATE_KEY`) | `voxa-update-k1.pem` | Rotate to K2 (below) |
| K2 (Ed25519) | Nothing, until K1 is lost | Nothing. Never in CI | `voxa-update-k2.pem` | Generate a new pair in the next release while K1 still works |
| Android release keystore | Every APK from 1.0.0 on | Optional: `voxa-release.jks`, only for the local fallback `android/release.ps1` (CI has it as the Environment `release` secrets `ANDROID_RELEASE_*`, step 6) | `voxa-release.jks` and its password | Every Android user must uninstall and lose History. **Never lose it, never regenerate it** |
| macOS "Voxa Code Signing" certificate | The macOS app, so Accessibility access survives updates | Nothing (CI has `MAC_SIGN_P12`, `MAC_SIGN_P12_PASSWORD`) | `.p12`, `.key`, `.crt` and the password | Every Mac user must allow Accessibility again. Never regenerate it |

After each step, delete the loose `.pem`, `.b64` and `.key` files from `~/voxa-keys`: at most
`voxa-release.jks` stays on the build PC, for the local fallback.

### What refuses placeholder keys

Until you do steps 1 and 2 the app has placeholder update keys. Then:

- `cargo build` and the debug builds print a warning; the app says "Updates are off in this build".
- Any release build fails: CI tags (`VOXA_RELEASE_BUILD=1`, `core/voxa-update/build.rs`), the
  Windows installer (`ISCC desktop\windows\voxa.iss` stops with an error), and every Android
  release build (`./gradlew assembleRelease`, Android Studio "Build APK", `android/release.ps1`,
  `.github/workflows/android-release.yml`).
- Only an explicit dev flag builds a local test copy, never handed out: `VOXA_ALLOW_PLACEHOLDER_KEYS=1`
  (cargo), `ISCC /DAllowPlaceholderKeys`, `./gradlew assembleRelease -Pvoxa.devRelease=true`.
- `python tools/lint/run.py --release` fails (`update-key-present`); CI runs it on every tag.

## 1. Generate K1 and K2

In `~/voxa-keys`:

```bash
cd ~/voxa-keys
openssl genpkey -algorithm ed25519 -out voxa-update-k1.pem
openssl genpkey -algorithm ed25519 -out voxa-update-k2.pem
openssl pkey -in voxa-update-k1.pem -pubout -outform DER | tail -c 32 | xxd -p -c 32   # K1 public, 64 hex characters
openssl pkey -in voxa-update-k2.pem -pubout -outform DER | tail -c 32 | xxd -p -c 32   # K2 public, 64 hex characters
```

In the repo: paste the two public hex strings into `core/voxa-update/src/keys.rs` as `K1_HEX`
(line 15) and `K2_HEX` (line 17). Public keys are safe to commit. Then:

```bash
cd /c/projects/voxa
cargo test -q --release -p voxa-update
python tools/lint/run.py --release          # update-key-present passes once both are real
git add core/voxa-update/src/keys.rs
git commit -m "release: the real update public keys"
```

## 2. Put K1 into a protected GitHub Environment

Why an Environment and not a repo secret: `desktop.yml` also runs on pull requests with
`contents: write`; a plain repo secret is readable by any workflow change pushed to a branch. Only
these jobs name `environment: release`: the desktop `feed` job, the `macos` job on tags (for the
Mac certificate), the `build` job (the Android release keystore) and the `feed` job (K1) of
`android-release.yml`, and `feed-pause.yml`. Each run waits for your approval, so a desktop tag
run asks twice (macos, then feed) and an Android tag run asks twice (build, then feed). The
Android keystore and K1 are never in the same job (lint `release-android-ci`).

1. GitHub, Settings, Environments, New environment: `release`.
2. Required reviewers: yourself. Deployment branches and tags: selected tags only, `v*` and
   `android-v*`. Workflows you start by hand (`android-release` re-runs, `feed-pause`) must
   therefore run from a tag ref: `gh workflow run android-release.yml --ref android-v1.0.0`.
   The workflow refuses any other ref itself.
3. Add the secret from the file, in `~/voxa-keys` (nothing is echoed to the terminal):

```bash
gh secret set VOXA_UPDATE_KEY --env release --repo blackscythe123/voxa < voxa-update-k1.pem
```

## 3. Store K2 (and a copy of K1) offline only

Put `voxa-update-k2.pem` and `voxa-update-k1.pem` in your password manager plus one offline copy
(a USB stick kept at home). Never put K2 in CI. Then delete both loose `.pem` files.

## 4. Protect the tags

GitHub, Settings, Rules, New tag ruleset: target `v*`, `android-v*` and `update-feed`; restrict
deletions and block force pushes.

## 5. Create the feed release once (before the first tag)

```bash
gh release create update-feed --prerelease --latest=false --title "Update feed" --notes "Signed update feeds. Not a release."
```

A prerelease never becomes "latest", so `releases/latest/download/Voxa-Setup.exe` keeps pointing at
the desktop installer. Each workflow replaces only its own asset with `--clobber`. Without this
release, the first tag's feed job fails after the release is already public.

## K1 leaked or lost: rotation

The app embeds exactly two keys with ids 1 and 2 (`keys.rs`, the `KEYS` array on line 20; the test
`every_compiled_key_is_32_bytes_and_ids_are_unique` expects two). Rotating means:

1. GitHub: put K2 into `VOXA_UPDATE_KEY` (step 2 with `voxa-update-k2.pem`).
2. Repo: change `key_id:1` to `key_id:2` in the three signing steps:
   `.github/workflows/desktop.yml` (the `jq ... '{key_id:1,...}'` line in "Sign the desktop
   feed"), `.github/workflows/android-release.yml` ("Sign the feed") and `.github/workflows/feed-pause.yml`
   ("Sign it").
3. `~/voxa-keys`: generate K3 (step 1, as `voxa-update-k3.pem`).
4. Repo, `keys.rs`: K1's slot gets K3 (`K1_HEX` = K3's public hex, still id 1), `K2_HEX` stays.
   Ship that release signed with K2 (the feed jobs now say `key_id:2`).
5. The next release can go back to signing with id 1 (K3) once installs have updated.
6. The first feed signed after a rotation may fail `guard-forward` because the published feed was
   signed with the old key: run that one job with `--force` only if you rotated on purpose.

Installs that have not updated yet keep trusting K1 until they do. That is accepted: an attacker
would also need to publish at the GitHub feed URL.

## 6. Android release key (from 1.0.0 on)

**Decision (2026-11-28):** Voxa 1.0.0 is signed with a proper release key from day one. There is
no lineage from the debug key and no `apksigner rotate` step: the pre-1.0 APKs were test builds
for the owner and one friend, so 1.0.0 does not install over them. Anyone with a test build
uninstalls it once (its History isn't kept), then installs 1.0.0. Every version from 1.0.0 on
updates in place. versionCode stays 10000 for 1.0.0.

Facts checked for the record (2026-11-28): the published `Voxa-Android-0.3.0.apk` is signed by
certificate SHA-256 `e648aa43bbc2a5e431a0f46a6e6d20cfd9422ec6ebe723922e8c86986fad802d`, the
certificate of both `%USERPROFILE%\.android\debug.keystore` and the backup
`C:\Users\Sam\voxa-signing-backup\voxa-android-current-signing-key.keystore`; its versionCode is
5 (below 10000). That digest is pinned in `android/signing/forbidden-signers.txt`: a release
signed with it fails the build (`verifyReleaseSigner`) and `android-release.yml`.

In `~/voxa-keys`, with Android Studio's keytool:

```bash
cd ~/voxa-keys
KT="/c/Program Files/Android/Android Studio/jbr/bin/keytool.exe"
# The permanent release key. It asks for a password: use a long one and store it with the keys.
"$KT" -genkeypair -keystore voxa-release.jks -alias voxa -keyalg RSA -keysize 4096 -validity 36500 -dname "CN=Voxa"
# Its certificate's SHA-256, for the repo (public):
"$KT" -list -v -keystore voxa-release.jks -alias voxa | grep "SHA256:"
```

Back up `voxa-release.jks` and its password offline at once (password manager plus the USB copy).
**Never regenerate it**: a new key means every Android user must uninstall and lose their History.

In the repo: paste the SHA256 value, without colons and in lowercase, as the only line of
`android/signing/release-cert.sha256` (below its comment) and commit it. From then on every
release build and `android-release.yml` refuse an APK signed by anything else, and
`python tools/lint/run.py --release` passes `release-android-signing`.

### Put the release key into the release Environment (CI builds Android)

Android releases are built by `.github/workflows/android-release.yml` on an `android-v*` tag
(owner decision 2026-11-28). Its `build` job reads four secrets from the `release` Environment.
In `~/voxa-keys`, in Git Bash (nothing is echoed to the terminal):

```bash
cd ~/voxa-keys
base64 -w0 voxa-release.jks > voxa-release.jks.b64
gh secret set ANDROID_RELEASE_KEYSTORE_B64 --env release --repo blackscythe123/voxa < voxa-release.jks.b64
gh secret set ANDROID_RELEASE_STORE_PASSWORD --env release --repo blackscythe123/voxa   # prompts
gh secret set ANDROID_RELEASE_KEY_PASSWORD --env release --repo blackscythe123/voxa     # prompts (same as the store password if keytool did not ask)
gh secret set ANDROID_RELEASE_KEY_ALIAS --env release --repo blackscythe123/voxa       # prompts: voxa
rm voxa-release.jks.b64
```

The job decodes the keystore to the runner's temp folder under `umask 077`, checks its
certificate against `release-cert.sha256` before building, passes the passwords to Gradle only
as environment values (`VOXA_RELEASE_*`, below), and deletes the file in an `if: always()` step.
K1 is in a separate job (`feed`) that never sees the keystore.

**The trade-off, stated plainly.** The permanent Android key now lives in GitHub as well as
offline. Anyone who controls the owner's GitHub account (or can push a tag and approve the
`release` environment) can now build and sign an Android update that phones accept, and with K1
in the same Environment, publish its feed. Before, that also needed the owner's PC. What limits
it: the `release` Environment has a required reviewer (the owner) and admits only `v*` and
`android-v*` tags; the tag rulesets (step 4) stop those tags being deleted or moved (add
"Restrict creations" there too if anyone else ever gets write access); GitHub never shows a
secret's value after it is set. Protect the account accordingly: a hardware
security key or passkey for 2FA, no long-lived personal access tokens with `repo` scope, and
review the audit log after any surprise. If the account is ever compromised: pause the feed
(release-runbook.md E), delete the four `ANDROID_RELEASE_*` secrets and `VOXA_UPDATE_KEY`, and
rotate K1 (above). The Android key itself cannot be rotated without every user reinstalling,
which is why it never leaves the protected Environment and the offline backup.

### How Gradle finds the release key (never committed)

For the local fallback (`android/release.ps1`) add these lines to `android/local.properties` (git-ignored), or set the matching environment
variables (`VOXA_RELEASE_STORE_FILE`, `VOXA_RELEASE_STORE_PASSWORD`, `VOXA_RELEASE_KEY_ALIAS`,
`VOXA_RELEASE_KEY_PASSWORD`):

```properties
voxa.release.storeFile=C:/Users/Sam/voxa-keys/voxa-release.jks
voxa.release.storePassword=...
voxa.release.keyAlias=voxa
voxa.release.keyPassword=...
```

A release build (`android-release.yml`, `android/release.ps1`, or `./gradlew assembleRelease` from `android/`) fails
when the update keys are placeholders, when no release key is configured, when the configured key
is a debug key, or when the signed APK's certificate is a forbidden signer or not the pinned one.
Check an APK by hand with:

```bash
AS="$(ls -d "$LOCALAPPDATA"/Android/Sdk/build-tools/* | grep -v rc | sort -V | tail -1)/apksigner.bat"
"$AS" verify --print-certs app/build/outputs/apk/release/app-arm64-v8a-release.apk
```

It lists one signer, whose SHA-256 is the one in `android/signing/release-cert.sha256`.

## 7. macOS: one stable self-signed certificate

macOS ties the Accessibility grant to the app's designated requirement. Ad hoc signing changes it
with every build, so every update drops the grant. A self-signed certificate that never changes
keeps the requirement stable, so the grant survives updates. Expect exactly one re-grant, at the
switch from ad hoc to this certificate. **Never regenerate this certificate**: a new one resets
every Mac user's Accessibility grant. Back it up with the other keys.

Create it once, in `~/voxa-keys`, in Git Bash:

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

CI (`desktop.yml`, the macos job on a tag) imports it and exports `VOXA_MAC_SIGN_IDENTITY`;
`desktop/macos/package.sh` signs with it (ad hoc when it is absent) and runs
`desktop/macos/check-signature.sh`. After the first signed build, CI prints the designated
requirement. Paste that line into `desktop/macos/designated-requirement.txt` and commit it: from
then on the build fails if the requirement ever changes. The app shows "Voxa needs Accessibility
access again after the update." only when the running app's requirement differs from the one
recorded at the last start and access is really off.

## Which lints guard this

`update-key-present` (real keys, with `--release`), `update-secret` (no private key in the tree),
`release-placeholder-guard`, `release-android-signing`, `release-feed-guard`,
`release-android-ci` (the CI release job: `environment: release`, the keystore and K1 never in
one job, no echoed or inline secrets, umask 077, the keystore deleted in an `always()` step) and
`android-release-key` (docs/guardrails.md, sections 10 and 12).
