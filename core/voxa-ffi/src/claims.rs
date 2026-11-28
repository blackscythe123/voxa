//! Which History entries are being uploaded right now, across the whole process: the keyboard's
//! own upload, History's "Send now" and the background send all claim an entry before uploading
//! it, so one recording is never uploaded twice at once, whoever starts it.
//!
//! A claim normally ends when its upload answers. One whose answer never came (a coroutine that
//! was cancelled before it ran) lapses after [`CLAIM_LAPSES_AFTER`], longer than any upload may
//! take, so an entry is never locked out for the life of the process.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Longer than the longest upload: two 20 MB parts at the slowest uplink `upload_timeout` assumes.
pub(crate) const CLAIM_LAPSES_AFTER: Duration = Duration::from_secs(70 * 60);

static CLAIMS: Mutex<Vec<(PathBuf, u64, Instant)>> = Mutex::new(Vec::new());

fn with<R>(f: impl FnOnce(&mut Vec<(PathBuf, u64, Instant)>) -> R) -> R {
    let mut g = CLAIMS.lock().unwrap_or_else(|e| e.into_inner());
    g.retain(|(_, _, at)| at.elapsed() < CLAIM_LAPSES_AFTER);
    f(&mut g)
}

/// Claim an entry of the History in `dir`; false when someone else holds it.
pub(crate) fn try_claim(dir: &Path, id: u64) -> bool {
    with(|c| {
        if c.iter().any(|(d, i, _)| d == dir && *i == id) {
            false
        } else {
            c.push((dir.to_path_buf(), id, Instant::now()));
            true
        }
    })
}

pub(crate) fn release(dir: &Path, id: u64) {
    with(|c| c.retain(|(d, i, _)| !(d == dir && *i == id)));
}

pub(crate) fn is_claimed(dir: &Path, id: u64) -> bool {
    with(|c| c.iter().any(|(d, i, _)| d == dir && *i == id))
}

/// Recordings an upload is reading now (incognito takes have no entry to claim): the orphan
/// sweep of a keyboard that restarted mid-upload leaves them alone.
static READING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Marks a recording as being read until dropped.
pub(crate) struct Reading(PathBuf);

impl Drop for Reading {
    fn drop(&mut self) {
        let mut g = READING.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = g.iter().position(|p| *p == self.0) {
            g.remove(i);
        }
    }
}

pub(crate) fn reading(path: &Path) -> Reading {
    READING.lock().unwrap_or_else(|e| e.into_inner()).push(path.to_path_buf());
    Reading(path.to_path_buf())
}

pub(crate) fn is_being_read(path: &Path) -> bool {
    READING.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|p| p == path)
}
