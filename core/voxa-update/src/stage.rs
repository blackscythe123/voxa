//! Staging a download: stream into a unique `.part-<pid>-<n>` file while hashing, cut off at the
//! declared size (or the cap), fsync, check the SHA-256, then rename to the final name. Any
//! failure deletes the part file, so a half or wrong file is never left to be launched.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Offer, UpdateError, hex_encode, policy::cap_for};

static SEQ: AtomicU64 = AtomicU64::new(0);

/// ERROR_DISK_FULL / ERROR_HANDLE_DISK_FULL on Windows, ENOSPC elsewhere.
pub fn is_disk_full(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(code) if (cfg!(windows) && (code == 112 || code == 39)) || (!cfg!(windows) && code == 28))
        || e.kind() == std::io::ErrorKind::StorageFull
}

/// The final file name for an offer: the last path segment of its URL, made safe.
pub fn file_name(offer: &Offer) -> String {
    let raw = offer.url.rsplit('/').next().unwrap_or("");
    let safe: String = raw.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')).collect();
    let safe = safe.trim_start_matches('.').to_string();
    if safe.is_empty() { format!("voxa-update-{}", offer.version) } else { safe }
}

/// A part file name no other download in any process uses.
pub fn part_path(dir: &Path) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    dir.join(format!(".part-{}-{n}", std::process::id()))
}

/// Stream `body` into `dir`, verifying against `offer`. Returns the staged file's path.
pub fn stage_from(body: impl Read, dir: &Path, offer: &Offer) -> Result<PathBuf, UpdateError> {
    stage_with(body, dir, offer, |p| File::create(p))
}

/// [`stage_from`] with the file opener injected (tests simulate a full disk with it).
pub fn stage_with<W: Write>(mut body: impl Read, dir: &Path, offer: &Offer, open: impl FnOnce(&Path) -> std::io::Result<W>) -> Result<PathBuf, UpdateError>
where
    W: Syncable,
{
    let limit = offer.size.min(cap_for(&offer.asset_key));
    if offer.size > cap_for(&offer.asset_key) {
        return Err(UpdateError::TooLarge);
    }
    std::fs::create_dir_all(dir)?;
    let part = part_path(dir);
    let result = (|| -> Result<(), UpdateError> {
        let mut out = open(&part)?;
        let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
        let mut buf = vec![0u8; 64 * 1024];
        let mut total: u64 = 0;
        loop {
            let n = match body.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(net_or_io(e)),
            };
            total += n as u64;
            if total > limit {
                return Err(UpdateError::TooLarge);
            }
            ctx.update(&buf[..n]);
            out.write_all(&buf[..n])?;
        }
        if total != offer.size {
            return Err(UpdateError::HashMismatch);
        }
        out.flush()?;
        out.sync()?;
        drop(out);
        if hex_encode(ctx.finish().as_ref()) != offer.sha256.to_ascii_lowercase() {
            return Err(UpdateError::HashMismatch);
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    let dest = dir.join(file_name(offer));
    let _ = std::fs::remove_file(&dest);
    if let Err(e) = std::fs::rename(&part, &dest) {
        let _ = std::fs::remove_file(&part);
        return Err(e.into());
    }
    Ok(dest)
}

/// A body read error: a dropped connection is "offline" (try again later), not a disk error.
fn net_or_io(e: std::io::Error) -> UpdateError {
    use std::io::ErrorKind::*;
    match e.kind() {
        TimedOut | ConnectionReset | ConnectionAborted | UnexpectedEof | BrokenPipe | NotConnected => UpdateError::Offline,
        _ if is_disk_full(&e) => UpdateError::DiskFull,
        _ => UpdateError::Offline,
    }
}

/// What a staged file needs besides Write: a way to reach the disk before the rename.
pub trait Syncable {
    fn sync(&mut self) -> std::io::Result<()>;
}

impl Syncable for File {
    fn sync(&mut self) -> std::io::Result<()> {
        self.sync_all()
    }
}

/// Check a staged file again against the stored offer, immediately before it is launched or
/// handed to the installer (the folder is user-writable). Size first, then the SHA-256.
pub fn verify_file(path: &Path, offer: &Offer) -> Result<(), UpdateError> {
    let meta = std::fs::metadata(path)?;
    if meta.len() != offer.size {
        return Err(UpdateError::HashMismatch);
    }
    let mut f = File::open(path)?;
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        ctx.update(&buf[..n]);
    }
    if hex_encode(ctx.finish().as_ref()) == offer.sha256.to_ascii_lowercase() { Ok(()) } else { Err(UpdateError::HashMismatch) }
}

/// The SHA-256 of some bytes, lowercase hex (tests and the tool).
pub fn sha256_hex(data: &[u8]) -> String {
    hex_encode(ring::digest::digest(&ring::digest::SHA256, data).as_ref())
}

/// Delete everything in `dir` except `keep` (one staged update is kept, older ones go).
pub fn clean_except(dir: &Path, keep: Option<&Path>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        let is_payload = name.starts_with(".part-") || name.ends_with(".exe") || name.ends_with(".apk") || name.ends_with(".dmg");
        if is_payload && p.is_file() && Some(p.as_path()) != keep {
            let _ = std::fs::remove_file(&p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("voxa-update-test-{tag}-{}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn offer_for(data: &[u8]) -> Offer {
        Offer {
            version: "1.0.1".into(),
            asset_key: "windows-x64".into(),
            url: "https://github.com/blackscythe123/voxa/releases/download/v1.0.1/Voxa-Setup-1.0.1.exe".into(),
            sha256: sha256_hex(data),
            size: data.len() as u64,
            auto: true,
            ..Offer::default()
        }
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect()
    }

    #[test]
    fn a_good_download_is_staged_under_its_versioned_name() {
        let d = tmp("good");
        let data = vec![7u8; 200_000];
        let p = stage_from(&data[..], &d, &offer_for(&data)).unwrap();
        assert_eq!(p.file_name().unwrap(), "Voxa-Setup-1.0.1.exe");
        assert_eq!(std::fs::read(&p).unwrap(), data);
        assert_eq!(leftovers(&d), vec!["Voxa-Setup-1.0.1.exe"]);
        assert!(verify_file(&p, &offer_for(&data)).is_ok());
    }

    #[test]
    fn a_download_longer_than_declared_is_cut_off_and_deleted() {
        let d = tmp("long");
        let data = vec![1u8; 10_000];
        let mut o = offer_for(&data);
        o.size = 5_000;
        let longer = vec![1u8; 10_000];
        assert_eq!(stage_from(&longer[..], &d, &o), Err(UpdateError::TooLarge));
        assert!(leftovers(&d).is_empty());
    }

    #[test]
    fn a_download_with_the_wrong_sha256_is_deleted() {
        let d = tmp("hash");
        let data = vec![2u8; 10_000];
        let mut o = offer_for(&data);
        o.sha256 = sha256_hex(b"something else");
        assert_eq!(stage_from(&data[..], &d, &o), Err(UpdateError::HashMismatch));
        assert!(leftovers(&d).is_empty());
    }

    #[test]
    fn a_truncated_download_leaves_no_file_behind() {
        let d = tmp("trunc");
        let data = vec![3u8; 10_000];
        let o = offer_for(&data);
        assert_eq!(stage_from(&data[..6_000], &d, &o), Err(UpdateError::HashMismatch));
        assert!(leftovers(&d).is_empty());
        // A connection dropped mid-stream.
        struct Dropping(usize);
        impl Read for Dropping {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.0 == 0 {
                    return Err(std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset"));
                }
                let n = buf.len().min(self.0);
                self.0 -= n;
                Ok(n)
            }
        }
        assert_eq!(stage_from(Dropping(4_000), &d, &o), Err(UpdateError::Offline));
        assert!(leftovers(&d).is_empty());
    }

    #[test]
    fn two_downloads_never_share_a_temp_file() {
        let d = tmp("two");
        let names: std::collections::HashSet<_> = (0..100).map(|_| part_path(&d)).collect();
        assert_eq!(names.len(), 100);
        assert!(part_path(&d).file_name().unwrap().to_string_lossy().starts_with(&format!(".part-{}-", std::process::id())));
        // Two threads staging the same offer at once both end with one whole, correct file.
        let data = vec![9u8; 300_000];
        let o = offer_for(&data);
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let (d, data, o) = (d.clone(), data.clone(), o.clone());
                std::thread::spawn(move || stage_from(&data[..], &d, &o))
            })
            .collect();
        let ok = handles.into_iter().map(|h| h.join().unwrap()).filter(|r| r.is_ok()).count();
        assert!(ok >= 1);
        assert!(verify_file(&d.join("Voxa-Setup-1.0.1.exe"), &o).is_ok());
        assert!(leftovers(&d).iter().all(|n| !n.starts_with(".part-")));
    }

    #[test]
    fn a_full_disk_leaves_no_part_file() {
        struct Full(File, usize);
        impl Write for Full {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                if self.1 < buf.len() {
                    return Err(std::io::Error::from_raw_os_error(if cfg!(windows) { 112 } else { 28 }));
                }
                self.1 -= buf.len();
                self.0.write(buf)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.0.flush()
            }
        }
        impl Syncable for Full {
            fn sync(&mut self) -> std::io::Result<()> {
                self.0.sync_all()
            }
        }
        let d = tmp("full");
        let data = vec![4u8; 200_000];
        let r = stage_with(&data[..], &d, &offer_for(&data), |p| Ok(Full(File::create(p)?, 70_000)));
        assert_eq!(r, Err(UpdateError::DiskFull));
        assert!(leftovers(&d).is_empty());
    }

    #[test]
    fn the_staged_file_is_checked_again_before_use() {
        let d = tmp("tamper");
        let data = vec![5u8; 50_000];
        let o = offer_for(&data);
        let p = stage_from(&data[..], &d, &o).unwrap();
        let mut bytes = std::fs::read(&p).unwrap();
        bytes[100] ^= 1;
        std::fs::write(&p, &bytes).unwrap();
        assert_eq!(verify_file(&p, &o), Err(UpdateError::HashMismatch));
        std::fs::write(&p, &bytes[..10]).unwrap();
        assert_eq!(verify_file(&p, &o), Err(UpdateError::HashMismatch));
    }

    #[test]
    fn a_hostile_url_name_stays_inside_the_folder() {
        let mut o = offer_for(b"x");
        o.url = "https://github.com/blackscythe123/voxa/releases/download/v1/..%2F..%2Fevil.exe".into();
        assert!(!file_name(&o).contains('/'));
        o.url = "https://github.com/blackscythe123/voxa/releases/download/v1/...".into();
        assert_eq!(file_name(&o), "voxa-update-1.0.1");
    }
}
