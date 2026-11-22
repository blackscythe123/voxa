//! macOS: the pieces GPUI does not provide (the `os` module on macOS). Same functions as win.rs.
//!
//! A "window" handle here is the front app's process id carried in the pointer type the shared
//! code uses (`HWND`), except where a function says it takes a GPUI window pointer (an NSView).

#![allow(unexpected_cfgs)] // objc 0.2's macros

use std::ffi::{CStr, c_void};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use cocoa::base::{NO, YES, id, nil};
use cocoa::foundation::{NSAutoreleasePool, NSString};
use objc::{class, msg_send, sel, sel_impl};

pub use crate::common::*;

/// Process id (front app) or NSView pointer (GPUI window), carried as a pointer.
pub type HWND = *mut c_void;

pub const START_AT_LOGIN: &str = "Open at login";
pub const START_AT_LOGIN_LONG: &str = "Open Voxa when I log in";

fn pid(h: HWND) -> i32 {
    h as usize as i32
}

fn ns(s: &str) -> id {
    unsafe { NSString::alloc(nil).init_str(s) }
}

fn rust(s: id) -> String {
    if s.is_null() {
        return String::new();
    }
    unsafe {
        let c: *const std::os::raw::c_char = msg_send![s, UTF8String];
        if c.is_null() { String::new() } else { CStr::from_ptr(c).to_string_lossy().into_owned() }
    }
}

fn app(h: HWND) -> id {
    unsafe { msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier: pid(h)] }
}

fn bundle_id(h: HWND) -> String {
    let a = app(h);
    if a.is_null() { String::new() } else { rust(unsafe { msg_send![a, bundleIdentifier] }) }
}

// ── the app in front ─────────────────────────────────────────────────────

pub fn foreground() -> HWND {
    unsafe {
        let ws: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        let front: id = msg_send![ws, frontmostApplication];
        if front.is_null() {
            return std::ptr::null_mut();
        }
        let p: i32 = msg_send![front, processIdentifier];
        p as usize as HWND
    }
}

pub fn describe(h: HWND) -> String {
    format!("pid {} \"{}\" [{}]", pid(h), app_name(h), bundle_id(h))
}

pub fn exe_path(h: HWND) -> String {
    let a = app(h);
    if a.is_null() {
        return String::new();
    }
    unsafe {
        let url: id = msg_send![a, executableURL];
        if url.is_null() { String::new() } else { rust(msg_send![url, path]) }
    }
}

fn bundle_path(h: HWND) -> String {
    let a = app(h);
    if a.is_null() {
        return String::new();
    }
    unsafe {
        let url: id = msg_send![a, bundleURL];
        if url.is_null() { String::new() } else { rust(msg_send![url, path]) }
    }
}

const BROWSERS: &[(&str, &str)] = &[
    ("com.brave.Browser", "Brave"),
    ("com.google.Chrome", "Chrome"),
    ("com.microsoft.edgemac", "Edge"),
    ("org.mozilla.firefox", "Firefox"),
    ("com.apple.Safari", "Safari"),
    ("company.thebrowser.Browser", "Arc"),
    ("com.vivaldi.Vivaldi", "Vivaldi"),
    ("com.operasoftware.Opera", "Opera"),
];

/// Editors and terminals, by bundle id: (bundle, key used by coding_agent, display name).
const HOSTS: &[(&str, &str, &str)] = &[
    ("com.microsoft.VSCode", "code", "VS Code"),
    ("com.todesktop.230313mzl4w4u92", "cursor", "Cursor"),
    ("com.exafunction.windsurf", "windsurf", "Windsurf"),
    ("com.apple.Terminal", "terminal", "Terminal"),
    ("com.googlecode.iterm2", "iterm2", "iTerm"),
    ("dev.warp.Warp-Stable", "warp", "Warp"),
];

pub fn browser_name(h: HWND) -> Option<String> {
    let b = bundle_id(h);
    BROWSERS.iter().find(|(id, _)| *id == b).map(|(_, n)| n.to_string())
}

/// The site comes from the address bar (domain_of, read off the UI thread), not the title.
pub fn site_in_tab(_h: HWND) -> Option<String> {
    None
}

pub fn app_name(h: HWND) -> String {
    let b = bundle_id(h);
    if let Some((_, _, n)) = HOSTS.iter().find(|(id, _, _)| *id == b) {
        return n.to_string();
    }
    let a = app(h);
    let name = if a.is_null() { String::new() } else { rust(unsafe { msg_send![a, localizedName] }) };
    if name.is_empty() { "this app".into() } else { name }
}

pub fn icon_owner(h: HWND) -> String {
    browser_name(h).unwrap_or_else(|| app_name(h))
}

/// (pid, key) where key identifies editors and terminals for coding_agent.
pub fn window_process(h: HWND) -> (u32, String) {
    let b = bundle_id(h);
    let key = HOSTS.iter().find(|(id, _, _)| *id == b).map(|(_, k, _)| k.to_string()).unwrap_or_default();
    (pid(h) as u32, key)
}

pub fn is_own_window(h: HWND) -> bool {
    pid(h) as u32 == std::process::id() || {
        let me = std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        let exe = exe_path(h);
        !exe.is_empty() && exe == me
    }
}

pub fn give_focus_back(h: HWND) {
    let a = app(h);
    if !a.is_null() {
        // NSApplicationActivateIgnoringOtherApps
        let _: bool = unsafe { msg_send![a, activateWithOptions: 2u64] };
        log(&format!("focus handed back to {}", describe(h)));
    }
}

pub fn switch_to(h: HWND) {
    give_focus_back(h);
    for _ in 0..15 {
        if foreground() == h {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(80));
}

// ── windows (GPUI's NSViews) ─────────────────────────────────────────────

pub fn window_ptr(w: &gpui::Window) -> Option<HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(w).map(|h| h.as_raw()) {
        Ok(RawWindowHandle::AppKit(h)) => Some(h.ns_view.as_ptr() as HWND),
        _ => None,
    }
}

fn ns_window(view: HWND) -> id {
    if view.is_null() {
        return nil;
    }
    unsafe { msg_send![view as id, window] }
}

/// GPUI already makes the strip a non-activating panel above other windows. Also show it on every
/// Space and over full-screen apps, and keep it when Voxa isn't the active app.
pub fn make_overlay(view: HWND) {
    let w = ns_window(view);
    if w.is_null() {
        return;
    }
    unsafe {
        // canJoinAllSpaces | stationary | ignoresCycle | fullScreenAuxiliary
        let behavior: u64 = 1 | 16 | 64 | 256;
        let _: () = msg_send![w, setCollectionBehavior: behavior];
        let _: () = msg_send![w, setHidesOnDeactivate: NO];
    }
    log("overlay set to show on every Space");
}

pub static STRIP: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub fn strip_visible(show: bool) {
    let view = STRIP.load(std::sync::atomic::Ordering::Relaxed) as HWND;
    let w = ns_window(view);
    if w.is_null() {
        return;
    }
    unsafe {
        if show {
            let _: () = msg_send![w, orderFrontRegardless];
        } else {
            let _: () = msg_send![w, orderOut: nil];
        }
    }
    log(&format!("strip {}", if show { "shown" } else { "hidden" }));
}

pub fn light_caption(_view: HWND) {}

pub fn bring_to_front(view: HWND) {
    unsafe {
        let ns_app: id = msg_send![class!(NSApplication), sharedApplication];
        let _: () = msg_send![ns_app, activateIgnoringOtherApps: YES];
        let w = ns_window(view);
        if !w.is_null() {
            let _: () = msg_send![w, makeKeyAndOrderFront: nil];
        }
    }
}

/// The window process shows in the Dock and app switcher while open; the tray process doesn't.
pub fn show_in_dock() {
    unsafe {
        let ns_app: id = msg_send![class!(NSApplication), sharedApplication];
        let _: bool = msg_send![ns_app, setActivationPolicy: 0i64]; // Regular
    }
}

// ── clipboard and paste ──────────────────────────────────────────────────

fn pasteboard_text() -> Option<String> {
    unsafe {
        let pb: id = msg_send![class!(NSPasteboard), generalPasteboard];
        let s: id = msg_send![pb, stringForType: ns("public.utf8-plain-text")];
        if s.is_null() { None } else { Some(rust(s)) }
    }
}

fn set_pasteboard_text(text: Option<&str>) -> bool {
    unsafe {
        let pb: id = msg_send![class!(NSPasteboard), generalPasteboard];
        let _: i64 = msg_send![pb, clearContents];
        match text {
            Some(t) => msg_send![pb, setString: ns(t) forType: ns("public.utf8-plain-text")],
            None => true,
        }
    }
}

/// Paste `text` into the app in front with Cmd+V, then put the user's clipboard back.
pub fn paste_here(text: &str) -> bool {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    let pool = unsafe { NSAutoreleasePool::new(nil) };
    let before = pasteboard_text();
    if !set_pasteboard_text(Some(text)) {
        log("FAIL could not write the clipboard");
        return false;
    }
    let sent = (|| {
        let src = CGEventSource::new(CGEventSourceStateID::HIDSystemState).ok()?;
        const V: u16 = 9; // kVK_ANSI_V
        let down = CGEvent::new_keyboard_event(src.clone(), V, true).ok()?;
        down.set_flags(CGEventFlags::CGEventFlagCommand);
        let up = CGEvent::new_keyboard_event(src, V, false).ok()?;
        up.set_flags(CGEventFlags::CGEventFlagCommand);
        down.post(CGEventTapLocation::HID);
        up.post(CGEventTapLocation::HID);
        Some(())
    })()
    .is_some();
    log(&format!("sent Cmd+V ({sent}) to {}", describe(foreground())));
    std::thread::sleep(Duration::from_millis(250));
    let restored = set_pasteboard_text(before.as_deref());
    log(&format!("clipboard restored={restored}"));
    unsafe {
        let _: () = msg_send![pool, drain];
    }
    sent
}

/// Pasting into other apps needs the Accessibility permission. Asks for it (once) if missing.
pub fn can_paste() -> bool {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::CFString;
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    }
    let key = CFString::new("AXTrustedCheckOptionPrompt");
    let dict = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::true_value().as_CFType())]);
    unsafe { AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef()) }
}

// ── which site, which agent ──────────────────────────────────────────────

fn osascript(script: &str) -> Option<String> {
    let out = std::process::Command::new("/usr/bin/osascript").arg("-e").arg(script).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|s| !s.is_empty())
}

/// The domain in the front browser's address bar (asks the browser via AppleScript; macOS asks
/// the user once per browser). Only the domain is kept.
pub fn domain_of(h: usize) -> Option<String> {
    let b = bundle_id(h as HWND);
    let script = match b.as_str() {
        "com.apple.Safari" => format!("tell application id \"{b}\" to get URL of front document"),
        "org.mozilla.firefox" | "" => return None, // Firefox has no AppleScript URL
        _ if BROWSERS.iter().any(|(id, _)| *id == b) => format!("tell application id \"{b}\" to get URL of active tab of front window"),
        _ => return None,
    };
    url_host(&osascript(&script)?)
}

/// "Claude Code" when a terminal in front is running the claude CLI. (Editors on macOS stay
/// themselves for now: telling their terminal panel from the editor needs Accessibility queries.)
pub fn coding_agent(key: &str, pid: u32) -> Option<(String, String)> {
    let host = HOSTS.iter().find(|(_, k, _)| *k == key)?.2.to_string();
    if !matches!(key, "terminal" | "iterm2" | "warp") {
        return None;
    }
    claude_under(pid).then(|| ("Claude Code".to_string(), host))
}

fn claude_under(root: u32) -> bool {
    let Ok(out) = std::process::Command::new("/bin/ps").args(["-A", "-o", "pid=,ppid=,comm="]).output() else { return false };
    let procs: Vec<(u32, u32, String)> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.collect::<Vec<_>>().join(" ")))
        })
        .collect();
    procs.iter().filter(|(_, _, c)| c.ends_with("/claude") || c == "claude").any(|(p, _, _)| {
        let mut cur = *p;
        for _ in 0..16 {
            match procs.iter().find(|(q, _, _)| *q == cur) {
                Some((_, parent, _)) if *parent == root => return true,
                Some((_, parent, _)) if *parent > 1 && *parent != cur => cur = *parent,
                _ => return false,
            }
        }
        false
    })
}

// ── single instance ──────────────────────────────────────────────────────

fn hold_lock(name: &str) -> bool {
    let dir = crate::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let Ok(f) = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dir.join(name)) else { return true };
    match f.try_lock() {
        Ok(()) => {
            std::mem::forget(f); // held for the life of the process
            true
        }
        Err(_) => false,
    }
}

/// Another launch asks the running one to show its window by touching `file`.
fn watch_requests(file: &'static str) {
    let path = crate::data_dir().join(file);
    std::thread::spawn(move || {
        let mut last = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let now = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            if now.is_some() && now != last {
                OPEN_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
                last = now;
            }
        }
    });
}

fn request(file: &str) {
    let _ = std::fs::write(crate::data_dir().join(file), format!("{:?}", std::time::SystemTime::now()));
}

pub fn single_instance() -> bool {
    if !hold_lock("voxa.lock") {
        request("open-request");
        return false;
    }
    watch_requests("open-request");
    true
}

pub fn single_window_instance() -> bool {
    if !hold_lock("window.lock") {
        request("window-request");
        return false;
    }
    watch_requests("window-request");
    true
}

// ── open at login ────────────────────────────────────────────────────────

fn launch_agent() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Library/LaunchAgents/app.voxa.desktop.plist")
}

pub fn starts_with_windows() -> bool {
    launch_agent().exists()
}

pub fn set_start_with_windows(on: bool) -> std::io::Result<()> {
    let path = launch_agent();
    if !on {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            r => r,
        };
    }
    let exe = std::env::current_exe()?;
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(
        &path,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>app.voxa.desktop</string>
  <key>ProgramArguments</key><array><string>{}</string><string>--background</string></array>
  <key>RunAtLoad</key><true/>
</dict></plist>
"#,
            exe.display()
        ),
    )
}

// ── playback ─────────────────────────────────────────────────────────────

static PLAYER: Mutex<Option<std::process::Child>> = Mutex::new(None);

pub fn play_audio(path: &Path) -> bool {
    stop_audio();
    match std::process::Command::new("/usr/bin/afplay").arg(path).spawn() {
        Ok(c) => {
            *PLAYER.lock().unwrap() = Some(c);
            true
        }
        Err(_) => false,
    }
}

pub fn stop_audio() {
    if let Some(mut c) = PLAYER.lock().unwrap().take() {
        let _ = c.kill();
    }
}

// ── opening things ───────────────────────────────────────────────────────

pub fn reveal(path: &Path) {
    let _ = std::process::Command::new("/usr/bin/open").arg("-R").arg(path).spawn();
}

pub fn open(target: &std::ffi::OsStr) {
    let _ = std::process::Command::new("/usr/bin/open").arg(target).spawn();
}

pub fn open_mic_privacy() {
    open(std::ffi::OsStr::new("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"));
}

pub fn open_sound_settings() {
    open(std::ffi::OsStr::new("x-apple.systempreferences:com.apple.preference.sound"));
}

// ── app icons ────────────────────────────────────────────────────────────

/// Save an app's real icon (64 px PNG), once per app.
pub fn cache_app_icon(h: HWND, app: String) {
    let out = icon_file(&app);
    if out.exists() || app.is_empty() {
        return;
    }
    let bundle = bundle_path(h);
    if bundle.is_empty() {
        return;
    }
    std::thread::spawn(move || match bundle_icon_png(&bundle) {
        Some(png) => match image::load_from_memory(&png) {
            Ok(img) => {
                let img = img.resize(64, 64, image::imageops::FilterType::Lanczos3).to_rgba8();
                if write_png(&out, img.as_raw(), img.width()).is_ok() {
                    log(&format!("icon cached for {app}"));
                }
            }
            Err(_) => log(&format!("no icon for {app}")),
        },
        None => log(&format!("no icon for {app}")),
    });
}

fn bundle_icon_png(bundle: &str) -> Option<Vec<u8>> {
    unsafe {
        let pool = NSAutoreleasePool::new(nil);
        let ws: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        let image: id = msg_send![ws, iconForFile: ns(bundle)];
        let mut out = None;
        if !image.is_null() {
            let size = cocoa::foundation::NSSize::new(64.0, 64.0);
            let _: () = msg_send![image, setSize: size];
            let tiff: id = msg_send![image, TIFFRepresentation];
            if !tiff.is_null() {
                let rep: id = msg_send![class!(NSBitmapImageRep), imageRepWithData: tiff];
                if !rep.is_null() {
                    let props: id = msg_send![class!(NSDictionary), dictionary];
                    let png: id = msg_send![rep, representationUsingType: 4u64 properties: props]; // NSBitmapImageFileTypePNG
                    if !png.is_null() {
                        let len: usize = msg_send![png, length];
                        let bytes: *const u8 = msg_send![png, bytes];
                        if !bytes.is_null() && len > 0 {
                            out = Some(std::slice::from_raw_parts(bytes, len).to_vec());
                        }
                    }
                }
            }
        }
        let _: () = msg_send![pool, drain];
        out
    }
}

/// Icons for apps already in History that are running now.
pub fn cache_open_app_icons(wanted: std::collections::HashSet<String>) {
    let missing: Vec<String> = wanted.into_iter().filter(|a| !icon_file(a).exists()).collect();
    if missing.is_empty() {
        return;
    }
    unsafe {
        let pool = NSAutoreleasePool::new(nil);
        let ws: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        let apps: id = msg_send![ws, runningApplications];
        let n: usize = msg_send![apps, count];
        for i in 0..n {
            let a: id = msg_send![apps, objectAtIndex: i];
            let p: i32 = msg_send![a, processIdentifier];
            let h = p as usize as HWND;
            let name = icon_owner(h);
            if missing.contains(&name) {
                cache_app_icon(h, name);
            }
        }
        let _: () = msg_send![pool, drain];
    }
}
