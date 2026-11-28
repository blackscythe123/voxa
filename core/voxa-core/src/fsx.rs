//! Replacing a file safely on every platform.
//!
//! On Windows a rename over an existing file fails with "access denied" or "sharing violation"
//! while anything else has the target open for a moment: another save, Voxa's other process,
//! an antivirus scan or the search indexer. Trying once loses the save silently (a refreshed
//! sign-in, History). [`replace_file`] tries again with short, growing waits for up to about
//! two seconds before giving up. Every save that renames a temporary file into place uses it;
//! the `robust-rename` lint keeps it that way.

use std::io;
use std::path::Path;
use std::time::Duration;

/// Waits between tries: about two seconds in total, most of it only when something holds on.
const WAITS_MS: [u64; 12] = [2, 5, 10, 20, 40, 60, 100, 150, 200, 300, 400, 600];

/// Whether an error is Windows saying the file is busy right now (worth another try).
pub fn is_transient(e: &io::Error) -> bool {
    // Windows: ERROR_ACCESS_DENIED (5) shows up as PermissionDenied; ERROR_SHARING_VIOLATION (32)
    // and ERROR_LOCK_VIOLATION (33) by number. Elsewhere a real permission problem just costs the
    // two seconds before the error is returned.
    e.kind() == io::ErrorKind::PermissionDenied || (cfg!(windows) && matches!(e.raw_os_error(), Some(32 | 33)))
}

/// Rename `from` over `to`, retrying while the target is briefly busy. Other errors (missing
/// file, no such folder, disk errors) are returned at once.
pub fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    replace_with(from, to, |a, b| std::fs::rename(a, b), |d| std::thread::sleep(d))
}

/// [`replace_file`] with the rename and the wait passed in, so the retry rule can be tested.
pub fn replace_with(
    from: &Path,
    to: &Path,
    mut rename: impl FnMut(&Path, &Path) -> io::Result<()>,
    mut wait: impl FnMut(Duration),
) -> io::Result<()> {
    let mut result = rename(from, to);
    for ms in WAITS_MS {
        match &result {
            Err(e) if is_transient(e) && from.exists() => {
                wait(Duration::from_millis(ms));
                result = rename(from, to);
            }
            _ => break,
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn busy() -> io::Error {
        io::Error::new(io::ErrorKind::PermissionDenied, "busy")
    }

    #[test]
    fn a_target_busy_for_a_moment_is_still_replaced() {
        // An antivirus scan holds the file for the first three tries, then lets go.
        let dir = std::env::temp_dir().join(format!("voxa-fsx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let from = dir.join("a.tmp");
        std::fs::write(&from, b"x").unwrap();
        let tries = Cell::new(0);
        let r = replace_with(&from, &dir.join("a"), |_, _| {
            tries.set(tries.get() + 1);
            if tries.get() <= 3 { Err(busy()) } else { Ok(()) }
        }, |_| {});
        assert!(r.is_ok());
        assert_eq!(tries.get(), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_target_busy_for_good_gives_up_after_about_two_seconds() {
        let dir = std::env::temp_dir().join(format!("voxa-fsx-busy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let from = dir.join("b.tmp");
        std::fs::write(&from, b"x").unwrap();
        let waited = Cell::new(Duration::ZERO);
        let r = replace_with(&from, &dir.join("b"), |_, _| Err(busy()), |d| waited.set(waited.get() + d));
        assert!(r.is_err());
        assert!(waited.get() >= Duration::from_millis(1500) && waited.get() <= Duration::from_millis(2500), "{:?}", waited.get());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn other_errors_are_not_retried() {
        let tries = Cell::new(0);
        let r = replace_with(Path::new("missing.tmp"), Path::new("x"), |_, _| {
            tries.set(tries.get() + 1);
            Err(io::Error::new(io::ErrorKind::NotFound, "gone"))
        }, |_| {});
        assert!(r.is_err());
        assert_eq!(tries.get(), 1);
    }

    #[test]
    fn many_writers_replacing_one_file_all_succeed() {
        // The real thing: 8 threads renaming over one file at once (Windows' busy target case).
        let dir = std::env::temp_dir().join(format!("voxa-fsx-many-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("session.json");
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let (dir, target) = (dir.clone(), target.clone());
                std::thread::spawn(move || {
                    for j in 0..25 {
                        let tmp = dir.join(format!("{i}-{j}.tmp"));
                        std::fs::write(&tmp, format!("{i}-{j}")).unwrap();
                        replace_file(&tmp, &target).expect("replaced");
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert!(std::fs::read_to_string(&target).unwrap().contains('-'));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
