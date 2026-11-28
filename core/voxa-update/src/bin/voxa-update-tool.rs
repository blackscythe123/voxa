//! voxa-update-tool: for CI and tests only, never shipped in an app.
//!
//!   voxa-update-tool verify <feed.json> --channel desktop|android [--pub <64 hex>]
//!       verify a signed feed with the shipped verifier (the compiled keys, or one given key)
//!   voxa-update-tool sign --key <pkcs8.pem> --key-id <n> --payload <payload.json> --out <feed.json>
//!       wrap and sign a payload (local TEST feeds; CI signs with OpenSSL)
//!   voxa-update-tool fetch-check --channel desktop|android [--asset <key>]
//!       fetch the published feed and one asset through the real client policy

use std::process::ExitCode;

use voxa_update::{Channel, Origin, b64_decode, b64_encode, hex_decode, keys};

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn channel(args: &[String]) -> Result<Channel, String> {
    match arg(args, "--channel").as_deref() {
        Some("desktop") => Ok(Channel::Desktop),
        Some("android") => Ok(Channel::Android),
        _ => Err("--channel desktop|android is required".into()),
    }
}

fn key_set(args: &[String]) -> Result<Vec<(u8, [u8; 32])>, String> {
    match arg(args, "--pub") {
        Some(hex) => {
            let k: [u8; 32] = hex_decode(hex.trim()).and_then(|v| v.try_into().ok()).ok_or("--pub must be 64 hex characters")?;
            Ok(vec![(1, k), (2, k)])
        }
        None => Ok(keys::KEYS.to_vec()),
    }
}

/// The 32-byte seed from an OpenSSL Ed25519 PKCS#8 PEM, loaded through ring.
fn load_pem(path: &str) -> Result<ring::signature::Ed25519KeyPair, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let b64: String = text.lines().filter(|l| !l.starts_with("-----")).map(str::trim).collect();
    let der = b64_decode(&b64).ok_or("not a PEM file")?;
    ring::signature::Ed25519KeyPair::from_pkcs8_maybe_unchecked(&der).map_err(|_| "not an Ed25519 private key".to_string())
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("verify") => {
            let file = args.get(1).ok_or("verify <feed.json>")?;
            let bytes = std::fs::read(file).map_err(|e| format!("{file}: {e}"))?;
            let ch = channel(args)?;
            let origin = match arg(args, "--test-origin") {
                Some(u) => Origin::test(&u).ok_or("bad --test-origin")?,
                None => Origin::github(ch),
            };
            let m = voxa_update::verify_feed(&bytes, &key_set(args)?, ch, 0, &origin).map_err(|e| format!("refused: {e}"))?;
            println!("ok: {} {} serial {} ({} assets)", m.channel, m.version, m.serial, m.assets.len());
            Ok(())
        }
        Some("sign") => {
            let pair = load_pem(&arg(args, "--key").ok_or("--key")?)?;
            let id: u8 = arg(args, "--key-id").and_then(|s| s.parse().ok()).ok_or("--key-id")?;
            let payload = std::fs::read(arg(args, "--payload").ok_or("--payload")?).map_err(|e| e.to_string())?;
            let sig = pair.sign(&payload);
            let env = serde_json::json!({ "key_id": id, "payload": b64_encode(&payload), "sig": b64_encode(sig.as_ref()) });
            std::fs::write(arg(args, "--out").ok_or("--out")?, env.to_string()).map_err(|e| e.to_string())?;
            Ok(())
        }
        Some("fetch-check") => {
            let ch = channel(args)?;
            let c = voxa_update::fetch::Client::new(Origin::github(ch), env!("CARGO_PKG_VERSION"), "CI").map_err(|e| e.to_string())?;
            let m = c.check(&key_set(args)?, ch, 0).map_err(|e| format!("feed: {e}"))?;
            let key = arg(args, "--asset").unwrap_or_else(|| match ch {
                Channel::Desktop => voxa_update::ASSET_WINDOWS.into(),
                Channel::Android => "android-arm64-v8a".into(),
            });
            let a = m.assets.get(&key).ok_or(format!("the feed has no {key}"))?;
            let offer = voxa_update::Offer {
                version: m.version.clone(),
                asset_key: key.clone(),
                url: a.url.clone(),
                sha256: a.sha256.clone(),
                size: a.size,
                ..Default::default()
            };
            let dir = std::env::temp_dir().join(format!("voxa-fetch-check-{}", std::process::id()));
            let path = c.download(&offer, &dir).map_err(|e| format!("{key}: {e}"))?;
            let _ = std::fs::remove_dir_all(&dir);
            println!("ok: {} {} and {key} ({} bytes) fetched through the client policy", m.channel, m.version, a.size);
            drop(path);
            Ok(())
        }
        _ => Err("usage: voxa-update-tool verify|sign|fetch-check (see the source header)".into()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("voxa-update-tool: {e}");
            ExitCode::FAILURE
        }
    }
}
