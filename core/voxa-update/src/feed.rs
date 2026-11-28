//! The signed feed. One file per channel:
//!
//! `{ "key_id": 1, "payload": "<base64 of the payload JSON bytes>", "sig": "<base64 Ed25519 signature>" }`
//!
//! The signature is checked over the exact payload bytes before anything in them is parsed, so
//! no JSON canonicalization is ever needed and an unverified payload never reaches serde.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Channel, MAX_FEED_BYTES, Origin, UpdateError, Version, b64_decode, split_url};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Envelope {
    pub key_id: u8,
    pub payload: String,
    pub sig: String,
}

/// The verified payload. Unknown fields are ignored, so fields can be added later.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Manifest {
    pub schema: u32,
    pub channel: String,
    pub serial: u64,
    pub version: String,
    /// Android only: `versionCode`, the one number Android compares.
    #[serde(default)]
    pub version_code: Option<u64>,
    pub notes_url: String,
    /// Below this version the update is offered as a manual download only.
    #[serde(default)]
    pub min_auto_from: Option<String>,
    /// The kill switch: nothing is offered or installed while it is set.
    #[serde(default)]
    pub paused: bool,
    pub assets: BTreeMap<String, Asset>,
    /// Android only: the SHA-256 of the APK signing certificate.
    #[serde(default)]
    pub signer_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Asset {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

/// Verify `bytes` (a whole downloaded feed) with `keys`, then parse and check it.
///
/// `highest_serial` is the highest serial this client has verified before; a lower one is a
/// replay of an older signed feed and is refused. The same serial is fine (the feed re-read).
pub fn verify_feed(bytes: &[u8], keys: &[(u8, [u8; 32])], channel: Channel, highest_serial: u64, origin: &Origin) -> Result<Manifest, UpdateError> {
    if bytes.len() as u64 > MAX_FEED_BYTES {
        return Err(UpdateError::TooLarge);
    }
    let env: Envelope = serde_json::from_slice(bytes).map_err(|_| UpdateError::BadEnvelope)?;
    let payload = b64_decode(&env.payload).ok_or(UpdateError::BadEnvelope)?;
    let sig = b64_decode(&env.sig).ok_or(UpdateError::BadEnvelope)?;
    if sig.len() != 64 || payload.is_empty() {
        return Err(UpdateError::BadEnvelope);
    }
    // An all-zero key is a placeholder: it never verifies anything.
    let key = keys
        .iter()
        .find(|(id, k)| *id == env.key_id && k.iter().any(|b| *b != 0))
        .map(|(_, k)| k)
        .ok_or(UpdateError::UnknownKey)?;
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key)
        .verify(&payload, &sig)
        .map_err(|_| UpdateError::BadSignature)?;

    // Verified. The schema first, so a future schema is refused rather than half read.
    #[derive(Deserialize)]
    struct Schema {
        schema: u32,
    }
    let s: Schema = serde_json::from_slice(&payload).map_err(|_| UpdateError::BadManifest)?;
    if s.schema != 1 {
        return Err(UpdateError::BadManifest);
    }
    let m: Manifest = serde_json::from_slice(&payload).map_err(|_| UpdateError::BadManifest)?;
    if m.channel != channel.as_str() {
        return Err(UpdateError::WrongChannel);
    }
    if m.serial < highest_serial {
        return Err(UpdateError::Replay);
    }
    check_manifest(&m, channel, origin)?;
    Ok(m)
}

/// The rules every field must meet, whatever is offered later.
fn check_manifest(m: &Manifest, channel: Channel, origin: &Origin) -> Result<(), UpdateError> {
    let bad = Err(UpdateError::BadManifest);
    let Some(v) = Version::parse(&m.version) else { return bad };
    if let Some(min) = &m.min_auto_from {
        if Version::parse(min).is_none() {
            return bad;
        }
    }
    if !m.notes_url.starts_with(&origin.notes_prefix) || !origin.hop_allowed(&m.notes_url) {
        return bad;
    }
    if channel == Channel::Android && m.version_code.is_none() {
        return bad;
    }
    if let Some(s) = &m.signer_sha256 {
        if !is_sha256_hex(s) {
            return bad;
        }
    }
    for (key, a) in &m.assets {
        if !asset_url_ok(&a.url, &v, origin) || !is_sha256_hex(&a.sha256) || a.size == 0 {
            return bad;
        }
        if a.size > crate::policy::cap_for(key) {
            return Err(UpdateError::TooLarge);
        }
    }
    Ok(())
}

/// An asset URL is under the repo's release downloads (or the test origin), on an allowed host,
/// and names the versioned file, never the version-free copies kept for README links.
pub fn asset_url_ok(url: &str, version: &Version, origin: &Origin) -> bool {
    if !url.starts_with(&origin.asset_prefix) || !origin.hop_allowed(url) {
        return false;
    }
    let Some((_, _, path)) = split_url(url) else { return false };
    if path.contains("..") || path.contains('?') || path.contains('#') {
        return false;
    }
    let file = path.rsplit('/').next().unwrap_or("");
    names_version(file, version)
}

/// The file name carries exactly this version: `-1.0.1.` or `-1.0.1-` ("Voxa-Setup-1.0.1.exe",
/// "Voxa-1.0.1-mac.dmg", "Voxa-Android-1.0.1-arm64-v8a.apk"), never as part of another version
/// ("11.0.12" contains "1.0.1").
fn names_version(file: &str, version: &Version) -> bool {
    let v = version.to_string();
    file.contains(&format!("-{v}.")) || file.contains(&format!("-{v}-"))
}

fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{Channel, b64_encode};
    use ring::signature::{Ed25519KeyPair, KeyPair};

    // TEST KEYS ONLY. Fixed seeds so the tests are deterministic; never used outside tests.
    pub const TEST_SEED_K1: [u8; 32] = [0x11; 32];
    pub const TEST_SEED_K2: [u8; 32] = [0x22; 32];
    pub const TEST_SEED_OTHER: [u8; 32] = [0x33; 32];

    pub fn pair(seed: [u8; 32]) -> Ed25519KeyPair {
        Ed25519KeyPair::from_seed_unchecked(&seed).unwrap()
    }

    pub fn public(seed: [u8; 32]) -> [u8; 32] {
        pair(seed).public_key().as_ref().try_into().unwrap()
    }

    pub fn test_keys() -> Vec<(u8, [u8; 32])> {
        vec![(1, public(TEST_SEED_K1)), (2, public(TEST_SEED_K2))]
    }

    pub fn sign(seed: [u8; 32], key_id: u8, payload: &[u8]) -> Vec<u8> {
        let sig = pair(seed).sign(payload);
        serde_json::to_vec(&serde_json::json!({ "key_id": key_id, "payload": b64_encode(payload), "sig": b64_encode(sig.as_ref()) })).unwrap()
    }

    pub const H: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    pub fn desktop_payload(version: &str, serial: u64) -> serde_json::Value {
        serde_json::json!({
            "schema": 1, "channel": "desktop", "serial": serial, "version": version,
            "notes_url": format!("https://github.com/blackscythe123/voxa/releases/tag/v{version}"),
            "min_auto_from": "1.0.0", "paused": false,
            "assets": {
                "windows-x64": { "url": format!("https://github.com/blackscythe123/voxa/releases/download/v{version}/Voxa-Setup-{version}.exe"), "sha256": H, "size": 23456789 },
                "macos-universal": { "url": format!("https://github.com/blackscythe123/voxa/releases/download/v{version}/Voxa-{version}-mac.dmg"), "sha256": H, "size": 34567890 }
            }
        })
    }

    fn gh() -> Origin {
        Origin::github(Channel::Desktop)
    }

    fn check(bytes: &[u8]) -> Result<Manifest, UpdateError> {
        verify_feed(bytes, &test_keys(), Channel::Desktop, 0, &gh())
    }

    fn bytes(v: &serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(v).unwrap()
    }

    #[test]
    fn a_feed_signed_by_the_release_key_is_accepted() {
        let m = check(&sign(TEST_SEED_K1, 1, &bytes(&desktop_payload("1.0.1", 5)))).unwrap();
        assert_eq!(m.version, "1.0.1");
        assert_eq!(m.serial, 5);
        assert_eq!(m.assets["windows-x64"].size, 23456789);
    }

    #[test]
    fn a_feed_signed_by_the_backup_key_is_accepted() {
        assert!(check(&sign(TEST_SEED_K2, 2, &bytes(&desktop_payload("1.0.1", 5)))).is_ok());
        // The backup key under the active key's id is a bad signature, not a pass.
        assert_eq!(check(&sign(TEST_SEED_K2, 1, &bytes(&desktop_payload("1.0.1", 5)))), Err(UpdateError::BadSignature));
    }

    #[test]
    fn one_changed_byte_in_the_payload_is_rejected() {
        let payload = bytes(&desktop_payload("1.0.1", 5));
        let sig = pair(TEST_SEED_K1).sign(&payload);
        for i in [0, payload.len() / 2, payload.len() - 1] {
            let mut p = payload.clone();
            p[i] ^= 0x01;
            let env = serde_json::to_vec(&serde_json::json!({ "key_id": 1, "payload": b64_encode(&p), "sig": b64_encode(sig.as_ref()) })).unwrap();
            assert_eq!(check(&env), Err(UpdateError::BadSignature), "byte {i}");
        }
        let mut s = sig.as_ref().to_vec();
        s[10] ^= 0x80;
        let env = serde_json::to_vec(&serde_json::json!({ "key_id": 1, "payload": b64_encode(&payload), "sig": b64_encode(&s) })).unwrap();
        assert_eq!(check(&env), Err(UpdateError::BadSignature));
    }

    #[test]
    fn an_unsigned_or_truncated_envelope_is_rejected() {
        let good = sign(TEST_SEED_K1, 1, &bytes(&desktop_payload("1.0.1", 5)));
        for n in [0, 1, good.len() / 2, good.len() - 1] {
            assert_eq!(check(&good[..n]), Err(UpdateError::BadEnvelope), "truncated to {n}");
        }
        let payload = b64_encode(&bytes(&desktop_payload("1.0.1", 5)));
        for env in [
            serde_json::json!({ "key_id": 1, "payload": payload }),
            serde_json::json!({ "key_id": 1, "payload": payload, "sig": "" }),
            serde_json::json!({ "key_id": 1, "payload": payload, "sig": b64_encode(&[0u8; 63]) }),
            serde_json::json!({ "key_id": 1, "payload": "not base64!", "sig": b64_encode(&[0u8; 64]) }),
        ] {
            assert_eq!(check(&bytes(&env)), Err(UpdateError::BadEnvelope));
        }
        // A bare payload (no envelope at all).
        assert_eq!(check(&bytes(&desktop_payload("1.0.1", 5))), Err(UpdateError::BadEnvelope));
        let big = vec![b' '; MAX_FEED_BYTES as usize + 1];
        assert_eq!(check(&big), Err(UpdateError::TooLarge));
    }

    #[test]
    fn a_feed_signed_by_an_unknown_key_is_rejected() {
        let p = bytes(&desktop_payload("1.0.1", 5));
        assert_eq!(check(&sign(TEST_SEED_OTHER, 3, &p)), Err(UpdateError::UnknownKey));
        assert_eq!(check(&sign(TEST_SEED_OTHER, 1, &p)), Err(UpdateError::BadSignature));
        // Placeholder (all-zero) keys never verify, whatever the id.
        assert_eq!(verify_feed(&sign(TEST_SEED_K1, 1, &p), &[(1, [0; 32])], Channel::Desktop, 0, &gh()), Err(UpdateError::UnknownKey));
    }

    #[test]
    fn a_feed_for_the_other_channel_is_rejected() {
        let p = bytes(&desktop_payload("1.0.1", 5));
        assert_eq!(verify_feed(&sign(TEST_SEED_K1, 1, &p), &test_keys(), Channel::Android, 0, &gh()), Err(UpdateError::WrongChannel));
    }

    #[test]
    fn a_lower_serial_than_already_seen_is_rejected() {
        let p = bytes(&desktop_payload("1.0.1", 5));
        let env = sign(TEST_SEED_K1, 1, &p);
        assert_eq!(verify_feed(&env, &test_keys(), Channel::Desktop, 6, &gh()), Err(UpdateError::Replay));
        assert!(verify_feed(&env, &test_keys(), Channel::Desktop, 5, &gh()).is_ok(), "the same feed read again is fine");
    }

    #[test]
    fn an_unknown_schema_is_rejected_and_unknown_fields_are_ignored() {
        let mut v = desktop_payload("1.0.1", 5);
        v["schema"] = 2.into();
        assert_eq!(check(&sign(TEST_SEED_K1, 1, &bytes(&v))), Err(UpdateError::BadManifest));
        let mut v = desktop_payload("1.0.1", 5);
        v["something_new"] = serde_json::json!({ "x": [1, 2] });
        v["assets"]["windows-x64"]["also_new"] = true.into();
        assert!(check(&sign(TEST_SEED_K1, 1, &bytes(&v))).is_ok());
    }

    #[test]
    fn an_asset_url_outside_the_repo_releases_or_not_https_is_rejected() {
        let bad_urls = [
            "https://github.com/someone-else/voxa/releases/download/v1.0.1/Voxa-Setup-1.0.1.exe".to_string(),
            format!("{}://github.com/blackscythe123/voxa/releases/download/v1.0.1/Voxa-Setup-1.0.1.exe", "http"),
            "https://evil.example/blackscythe123/voxa/releases/download/v1.0.1/Voxa-Setup-1.0.1.exe".to_string(),
            // The version-free copy kept for README links.
            "https://github.com/blackscythe123/voxa/releases/download/v1.0.1/Voxa-Setup.exe".to_string(),
            "https://github.com/blackscythe123/voxa/releases/download/v1.0.1/../../x/Voxa-Setup-1.0.1.exe".to_string(),
            "https://github.com/blackscythe123/voxa/releases/latest/download/Voxa-Setup-1.0.1.exe".to_string(),
        ];
        for url in bad_urls {
            let mut v = desktop_payload("1.0.1", 5);
            v["assets"]["windows-x64"]["url"] = url.clone().into();
            assert_eq!(check(&sign(TEST_SEED_K1, 1, &bytes(&v))), Err(UpdateError::BadManifest), "{url}");
        }
        let mut v = desktop_payload("1.0.1", 5);
        v["notes_url"] = "https://evil.example/notes".into();
        assert_eq!(check(&sign(TEST_SEED_K1, 1, &bytes(&v))), Err(UpdateError::BadManifest));
        let mut v = desktop_payload("1.0.1", 5);
        v["assets"]["windows-x64"]["sha256"] = "abc".into();
        assert_eq!(check(&sign(TEST_SEED_K1, 1, &bytes(&v))), Err(UpdateError::BadManifest));
    }

    #[test]
    fn an_asset_name_must_carry_the_exact_version_not_a_substring() {
        let v = Version::parse("1.0.1").unwrap();
        let o = gh();
        let url = |f: &str| format!("https://github.com/blackscythe123/voxa/releases/download/v1.0.1/{f}");
        for good in ["Voxa-Setup-1.0.1.exe", "Voxa-1.0.1-mac.dmg", "Voxa-Android-1.0.1-arm64-v8a.apk", "Voxa-Android-1.0.1-universal.apk"] {
            assert!(asset_url_ok(&url(good), &v, &o), "{good}");
        }
        for bad in ["Voxa-Setup-11.0.12.exe", "Voxa-11.0.1-mac.dmg", "Voxa-Setup-1.0.10.exe", "Voxa-Setup-1.0.1x.exe", "Voxa1.0.1.exe"] {
            assert!(!asset_url_ok(&url(bad), &v, &o), "{bad}");
        }
    }

    #[test]
    fn a_declared_size_over_the_cap_is_refused_before_downloading() {
        let mut v = desktop_payload("1.0.1", 5);
        v["assets"]["windows-x64"]["size"] = (crate::MAX_WINDOWS_BYTES + 1).into();
        assert_eq!(check(&sign(TEST_SEED_K1, 1, &bytes(&v))), Err(UpdateError::TooLarge));
    }

    /// The known answer vector: a TEST key, a message and a signature made with OpenSSL 3
    /// (`openssl pkeyutl -sign -rawin`, exactly as CI signs), committed under tests/fixtures.
    /// Proves the shipped verifier accepts what CI's OpenSSL produces.
    #[test]
    fn a_known_answer_vector_verifies() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let pubhex = std::fs::read_to_string(dir.join("test-k1.pub.hex")).unwrap();
        let key: [u8; 32] = crate::hex_decode(pubhex.trim()).unwrap().try_into().unwrap();
        let env = std::fs::read(dir.join("test-desktop.json")).unwrap();
        let o = Origin::github(Channel::Desktop);
        let m = verify_feed(&env, &[(1, key)], Channel::Desktop, 0, &o).unwrap();
        assert_eq!(m.version, "1.0.1");
        // The raw vector too: message bytes and the 64-byte signature.
        let msg = std::fs::read(dir.join("kat.msg")).unwrap();
        let sig = std::fs::read(dir.join("kat.sig")).unwrap();
        assert_eq!(sig.len(), 64);
        ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, &key).verify(&msg, &sig).unwrap();
        let mut tampered = msg.clone();
        tampered[0] ^= 1;
        assert!(ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, &key).verify(&tampered, &sig).is_err());
    }
}
