// Fixture for tools/lint: not compiled. Every "Bad" line must be caught, every "Good" line must not.

fn copy() {
    let a = button("ok", "Done \u{2705}");                 // Bad: copy-emoji
    let b = label("Saved - ready to paste");                // Bad: copy-dash
    let c = label("Paused \u{2014} tap to resume");         // Bad: copy-dash
    let d = button("open", "Open Voice Settings");          // Bad: copy-case
    let e = title("Happy Diwali from Voxa");                // Bad: copy-occasion
    let f = label("Open Voxa");                             // Good: product name
    let g = label("Ready. Press Ctrl+Space");               // Good: sentence then key name
    let h = format!("{ts}-{}.wav", 1);                      // Good: a file name, not a dash as punctuation
    os::log("Saved - ready");                               // Good: logs are not UI copy
}

fn logs(text: &str, ev: &Event, cookie_header: &str) {
    os::log(&format!("pasted {text}"));                     // Bad: privacy-log
    os::log(&format!("event {ev:?}"));                      // Bad: privacy-log (whole event)
    eprintln!("cookies: {}", cookie_header);                // Bad: privacy-log
    os::log(&format!("pasted {}", redact::text(text)));    // Good: redacted
    os::log(&format!("pasted {} chars", text.len()));      // Good: a length
    os::log(&format!("event {}", redact::event(ev)));      // Good: redacted
}

fn show_strip(hwnd: HWND) {
    unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };         // Good: never activates
    unsafe { ShowWindow(hwnd, SW_SHOW) };                   // Bad: focus-steal
    unsafe { SetForegroundWindow(hwnd) };                   // Bad: focus-steal
    window.activate_window();                               // Bad: focus-steal
}

fn categories() {
    let cats = [
        category(TAG_SIGNIN, &signin, true),                // Good: Sign in may open Voxa
        category(TAG_DICTATIONS, &copy, false),             // Good: background
        category(TAG_DICTATIONS, "Send now", true),         // Bad: focus-notify
    ];
}

fn test_hooks() {
    let secs = std::env::var("VOXA_TEST_DICTATE").ok();     // Bad: isolation-test-hook
}

fn guarded_hook() {
    let secs = std::env::var("VOXA_TEST_PAUSE").ok().filter(|_| std::env::var_os("VOXA_DATA").is_some()); // Good
}

fn real_root() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_default())   // Bad: isolation-env
}

fn guarded_root() -> PathBuf {
    if let Some(d) = std::env::var_os("VOXA_DATA") {
        return d.into();
    }
    PathBuf::from(std::env::var("APPDATA").unwrap_or_default())   // Good: VOXA_DATA first
}

fn save_settings(dir: &Path, b: &[u8]) {
    let path = dir.join("settings.json");
    let _ = std::fs::write(&path, b);                       // Bad: robust-atomic-write
    let _ = std::fs::write(dir.join("icons").join("a.png"), b); // Good: not a store file
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");                   // Bad: robust-atomic-write (fixed temp name)
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}

fn highlight(text: &str, word: &str) -> String {
    let lower = text.to_lowercase();
    let i = lower.find(word).unwrap_or(0);
    text[..i].to_string()                                   // Bad: robust-case-slice
}

fn ascii_only(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    lower[..1].to_string()                                  // Good: ASCII case keeps byte lengths
}

#[cfg(test)]
mod tests {
    fn t() {
        let x = label("Test - Only \u{1F600}");             // Good: tests are not UI
        std::fs::write(dir.join("settings.json"), b"{}").unwrap();
    }
}
