//! Win32 pieces GPUI does not provide: a non-activating topmost overlay,
//! foreground-window inspection, and paste-with-clipboard-restore.

use std::ffi::c_void;
use std::io::Write;
use std::sync::OnceLock;
use std::time::Instant;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DWMNCRP_DISABLED, DWMWA_BORDER_COLOR, DWMWA_NCRENDERING_POLICY, DwmSetWindowAttribute};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VK_CONTROL,
};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowTextW,
    HWND_TOPMOST, MA_NOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SW_HIDE, SW_SHOWNOACTIVATE, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_MOUSEACTIVATE, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
};

static START: OnceLock<Instant> = OnceLock::new();

/// The log file: %APPDATA%oxa-nextoxa-desktop.log (the working directory is System32 when
/// Windows starts Voxa at sign-in). VOXA_LOG overrides it for tests.
pub fn log_path() -> std::path::PathBuf {
    match std::env::var_os("VOXA_LOG") {
        Some(p) => p.into(),
        None => crate::data_dir().join("voxa-desktop.log"),
    }
}

/// Append a line to the log and stdout. Never logs secrets: callers pass states and sizes only.
pub fn log(msg: &str) {
    let t = START.get_or_init(Instant::now).elapsed().as_millis();
    let line = format!("[{t:>7}ms] {msg}");
    println!("{line}");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = writeln!(f, "{line}");
    }
}

pub fn foreground() -> HWND {
    unsafe { GetForegroundWindow() }
}

pub fn describe(hwnd: HWND) -> String {
    let mut title = [0u16; 256];
    let mut class = [0u16; 128];
    unsafe {
        let n = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
        let c = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
        format!(
            "{:?} \"{}\" [{}]",
            hwnd,
            String::from_utf16_lossy(&title[..n.max(0) as usize]),
            String::from_utf16_lossy(&class[..c.max(0) as usize])
        )
    }
}

/// Make the window a non-activating, topmost tool window, and swallow
/// activation on mouse clicks so clicking a strip button never steals focus.
pub fn make_overlay(hwnd: HWND) {
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let want = ex | (WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW) as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, want);
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        SetWindowSubclass(hwnd, Some(no_activate_proc), 0x5658_4131, 0);
        let none: u32 = 0xFFFF_FFFE; // DWMWA_COLOR_NONE: no Windows 11 frame border
        DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR as u32, &none as *const u32 as *const c_void, 4);
        let policy: i32 = DWMNCRP_DISABLED;
        DwmSetWindowAttribute(hwnd, DWMWA_NCRENDERING_POLICY as u32, &policy as *const i32 as *const c_void, 4);
    }
    log(&format!("overlay styles applied to {}", describe(hwnd)));
}

unsafe extern "system" fn no_activate_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_MOUSEACTIVATE {
        return MA_NOACTIVATE as LRESULT;
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

fn read_clipboard_text() -> Option<String> {
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT as u32) == 0 {
            return None;
        }
        let h = GetClipboardData(CF_UNICODETEXT as u32);
        if h.is_null() {
            return None;
        }
        let p = GlobalLock(h) as *const u16;
        if p.is_null() {
            return None;
        }
        let mut len = 0usize;
        while *p.add(len) != 0 {
            len += 1;
        }
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
        GlobalUnlock(h);
        Some(s)
    }
}

fn write_clipboard_text(text: &str) -> bool {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        if EmptyClipboard() == 0 {
            return false;
        }
        let h = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
        if h.is_null() {
            return false;
        }
        let p = GlobalLock(h) as *mut u16;
        std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
        GlobalUnlock(h);
        !SetClipboardData(CF_UNICODETEXT as u32, h as *mut c_void).is_null()
    }
}

fn with_clipboard<R>(f: impl FnOnce() -> R) -> Option<R> {
    for _ in 0..10 {
        if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
            let r = f();
            unsafe { CloseClipboard() };
            return Some(r);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    None
}

fn key(vk: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { 0 }, time: 0, dwExtraInfo: 0 },
        },
    }
}

/// Paste `text` into the window in front, then put the user's clipboard back.
/// The caller has already checked the right window is in front (voxa-core decides what to do if not).
/// Restores plain text only for now; every clipboard format is a TODO before release.
pub fn paste_here(text: &str) -> bool {
    let before = with_clipboard(read_clipboard_text).flatten();
    if with_clipboard(|| write_clipboard_text(text)) != Some(true) {
        log("FAIL could not write clipboard");
        return false;
    }
    let inputs = [key(VK_CONTROL, false), key(b'V' as u16, false), key(b'V' as u16, true), key(VK_CONTROL, true)];
    let sent = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
    log(&format!("sent Ctrl+V ({sent} events) to {}", describe(foreground())));
    std::thread::sleep(std::time::Duration::from_millis(250));
    let restored = with_clipboard(|| match &before {
        Some(s) => write_clipboard_text(s),
        None => unsafe { EmptyClipboard() != 0 },
    });
    log(&format!("clipboard restored={restored:?}"));
    sent == 4
}

/// Friendly app name for a window: known apps by exe, otherwise the window title.
/// Full path of the exe that owns a window ("" if Windows won't say).
pub fn exe_path(hwnd: HWND) -> String {
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    use windows_sys::Win32::Foundation::CloseHandle;
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            String::new()
        } else {
            let mut buf = [0u16; 520];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
            CloseHandle(h);
            if ok != 0 { String::from_utf16_lossy(&buf[..len as usize]) } else { String::new() }
        }
    }
}

const BROWSERS: &[(&str, &str)] = &[
    ("brave", "Brave"), ("chrome", "Chrome"), ("msedge", "Edge"), ("firefox", "Firefox"),
    ("opera", "Opera"), ("vivaldi", "Vivaldi"), ("arc", "Arc"),
];

/// Sites recognised from a browser tab's title: (text to find, name to show).
const SITES: &[(&str, &str)] = &[
    ("chatgpt", "ChatGPT"), ("gmail", "Gmail"), ("whatsapp", "WhatsApp"), ("slack", "Slack"),
    ("claude", "Claude"), ("google docs", "Google Docs"), ("google sheets", "Google Sheets"),
    ("google meet", "Google Meet"), ("notion", "Notion"), ("github", "GitHub"), ("linkedin", "LinkedIn"),
    ("youtube", "YouTube"), ("outlook", "Outlook"), ("discord", "Discord"), ("telegram", "Telegram"),
    ("messenger", "Messenger"), ("instagram", "Instagram"), ("reddit", "Reddit"), ("figma", "Figma"),
    ("jira", "Jira"), ("microsoft teams", "Teams"), ("gemini", "Gemini"), ("perplexity", "Perplexity"),
    (" / x", "X"),
];

fn window_title(hwnd: HWND) -> String {
    let mut title = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) };
    String::from_utf16_lossy(&title[..n.max(0) as usize])
}

/// The browser that owns `hwnd`, if it is one ("Brave").
pub fn browser_name(hwnd: HWND) -> Option<String> {
    let stem = std::path::Path::new(&exe_path(hwnd)).file_stem().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    BROWSERS.iter().find(|(s, _)| *s == stem).map(|(_, n)| n.to_string())
}

/// The site in the browser's current tab, if it is one Voxa knows ("ChatGPT").
pub fn site_in_tab(hwnd: HWND) -> Option<String> {
    browser_name(hwnd)?;
    site_from_title(&window_title(hwnd))
}

pub fn site_from_title(title: &str) -> Option<String> {
    let title = title.to_lowercase();
    SITES.iter().find(|(k, _)| title.contains(k)).map(|(_, n)| n.to_string())
}

#[cfg(test)]
mod tests {
    use super::site_from_title;

    #[test]
    fn sites_from_real_browser_titles() {
        assert_eq!(site_from_title("ChatGPT - Brave").as_deref(), Some("ChatGPT"));
        assert_eq!(site_from_title("Inbox (12) - someone@gmail.com - Gmail - Brave").as_deref(), Some("Gmail"));
        assert_eq!(site_from_title("(3) WhatsApp - Google Chrome").as_deref(), Some("WhatsApp"));
        assert_eq!(site_from_title("Project plan - Google Docs - Profile 1 - Microsoft Edge").as_deref(), Some("Google Docs"));
        assert_eq!(site_from_title("Home / X - Brave").as_deref(), Some("X"));
        assert_eq!(site_from_title("Rust docs - Brave"), None, "unknown sites stay as the browser");
    }
}

/// The app whose own icon represents `hwnd`: the browser for a site, else the app itself.
pub fn icon_owner(hwnd: HWND) -> String {
    browser_name(hwnd).unwrap_or_else(|| app_name(hwnd))
}

/// What History calls the place the user dictated into: the site for a known browser tab, else the app.
pub fn app_name(hwnd: HWND) -> String {
    if let Some(site) = site_in_tab(hwnd) {
        return site;
    }
    let exe = exe_path(hwnd);
    let stem = std::path::Path::new(&exe).file_stem().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    let known = match stem.as_str() {
        "whatsapp" | "whatsapp.root" => Some("WhatsApp"),
        "slack" => Some("Slack"),
        "chrome" => Some("Chrome"),
        "msedge" => Some("Edge"),
        "brave" => Some("Brave"),
        "firefox" => Some("Firefox"),
        "notepad" => Some("Notepad"),
        "code" => Some("VS Code"),
        "outlook" | "olk" => Some("Outlook"),
        "winword" => Some("Word"),
        "teams" | "ms-teams" => Some("Teams"),
        "discord" => Some("Discord"),
        "telegram" => Some("Telegram"),
        _ => None,
    };
    if let Some(k) = known {
        return k.to_string();
    }
    let mut title = [0u16; 256];
    let n = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) };
    let t = String::from_utf16_lossy(&title[..n.max(0) as usize]);
    if !t.is_empty() { t } else if !stem.is_empty() { stem } else { "this app".into() }
}

/// GPUI activates a window when it first shows it. Hand focus straight back to the app that had it.
pub fn give_focus_back(prev: HWND) {
    if !prev.is_null() && foreground() != prev {
        unsafe { SetForegroundWindow(prev) };
        log(&format!("focus handed back to {}", describe(prev)));
    }
}

pub static STRIP: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub fn strip_visible(show: bool) {
    let h = STRIP.load(std::sync::atomic::Ordering::Relaxed) as HWND;
    if h.is_null() { return; }
    unsafe { ShowWindow(h, if show { SW_SHOWNOACTIVATE } else { SW_HIDE }) };
    log(&format!("strip {}; foreground = {}", if show { "shown (no activate)" } else { "hidden" }, describe(foreground())));
}

/// Light caption bar in the nav colour, so the window reads as one surface (the default follows
/// the system's dark mode and looks like a different app).
pub fn light_caption(hwnd: HWND) {
    unsafe {
        let off: i32 = 0;
        DwmSetWindowAttribute(hwnd, 20, &off as *const i32 as *const c_void, 4);
        let caption: u32 = 0x00EAEEF1; // #F1EEEA as COLORREF (BGR)
        DwmSetWindowAttribute(hwnd, 35, &caption as *const u32 as *const c_void, 4);
        let text: u32 = 0x001A1A1A;
        DwmSetWindowAttribute(hwnd, 36, &text as *const u32 as *const c_void, 4);
    }
}

// ── single instance ──────────────────────────────────────────────────────

/// Set when a second launch (or the Start menu shortcut) asks this instance to show its window.
pub static OPEN_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// True if this is the only Voxa running. Otherwise asks the running one to open its window and
/// returns false, and the caller exits.
pub fn single_instance() -> bool {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::{CreateEventW, CreateMutexW, INFINITE, SetEvent, WaitForSingleObject};
    unsafe {
        let mutex = CreateMutexW(std::ptr::null(), 0, wide(r"Local\VoxaNextSingleInstance").as_ptr());
        let already = GetLastError() == ERROR_ALREADY_EXISTS;
        let event = CreateEventW(std::ptr::null(), 0, 0, wide(r"Local\VoxaNextOpenWindow").as_ptr());
        if already {
            if !event.is_null() {
                SetEvent(event);
            }
            return false;
        }
        let _ = mutex; // held for the life of the process
        let event = event as usize;
        std::thread::spawn(move || loop {
            if WaitForSingleObject(event as _, INFINITE) != 0 {
                break;
            }
            OPEN_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
        });
        true
    }
}

/// Bring a normal window to the front (used when the tray or a second launch opens Voxa).
pub fn bring_to_front(hwnd: HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, SW_RESTORE};
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd);
    }
}

// ── start with Windows ───────────────────────────────────────────────────

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Voxa Next";

pub fn starts_with_windows() -> bool {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    hkcu.open_subkey(RUN_KEY).and_then(|k| k.get_value::<String, _>(RUN_VALUE)).is_ok()
}

pub fn set_start_with_windows(on: bool) -> std::io::Result<()> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(RUN_KEY)?;
    if on {
        let exe = std::env::current_exe()?;
        key.set_value(RUN_VALUE, &format!("\"{}\" --background", exe.display()))
    } else {
        match key.delete_value(RUN_VALUE) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            r => r,
        }
    }
}

// ── playback ─────────────────────────────────────────────────────────────

/// Play a WAV file without blocking. Returns false if Windows could not start it.
pub fn play_audio(path: &std::path::Path) -> bool {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT};
    let w = wide(&path.to_string_lossy());
    unsafe { PlaySoundW(w.as_ptr(), std::ptr::null_mut(), SND_ASYNC | SND_FILENAME | SND_NODEFAULT) != 0 }
}

pub fn stop_audio() {
    use windows_sys::Win32::Media::Audio::PlaySoundW;
    unsafe {
        PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0);
    }
}

// ── app icons ────────────────────────────────────────────────────────────

/// Where an app's icon is cached, keyed by the name History shows.
pub fn icon_file(app: &str) -> std::path::PathBuf {
    let slug: String = app.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    crate::data_dir().join("icons").join(format!("{}.png", slug.trim_matches('-')))
}

/// Save the real icon of the app that owns `hwnd` (64 px PNG), once per app. Runs off the UI thread.
pub fn cache_app_icon(hwnd: HWND, app: String) {
    let out = icon_file(&app);
    if out.exists() || app.is_empty() {
        return;
    }
    let exe = exe_path(hwnd);
    if exe.is_empty() {
        return;
    }
    std::thread::spawn(move || match exe_icon_rgba(&exe, 64) {
        Some((rgba, n)) => {
            if write_png(&out, &rgba, n).is_ok() {
                log(&format!("icon cached for {app}"));
            }
        }
        None => log(&format!("no icon for {app}")),
    });
}

fn exe_icon_rgba(exe: &str, size: u32) -> Option<(Vec<u8>, u32)> {
    use windows_sys::Win32::Graphics::Gdi::{BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDIBits};
    use windows_sys::Win32::UI::Shell::SHDefExtractIconW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
    unsafe {
        let mut icon: HICON = std::ptr::null_mut();
        let path = wide(exe);
        if SHDefExtractIconW(path.as_ptr(), 0, 0, &mut icon, std::ptr::null_mut(), size) != 0 || icon.is_null() {
            return None;
        }
        let mut info: ICONINFO = std::mem::zeroed();
        let ok = GetIconInfo(icon, &mut info) != 0;
        let mut out = None;
        if ok && !info.hbmColor.is_null() {
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size as i32,
                biHeight: -(size as i32), // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            };
            let mut px = vec![0u8; (size * size * 4) as usize];
            if GetDIBits(dc, info.hbmColor, 0, size, px.as_mut_ptr() as _, &mut bmi, DIB_RGB_COLORS) > 0 {
                // BGRA -> RGBA. Old-style icons have no alpha: treat non-black as opaque.
                let has_alpha = px.chunks(4).any(|p| p[3] != 0);
                for p in px.chunks_mut(4) {
                    p.swap(0, 2);
                    if !has_alpha {
                        p[3] = if p[0] | p[1] | p[2] != 0 { 255 } else { 0 };
                    }
                }
                out = Some((px, size));
            }
            DeleteDC(dc);
        }
        if !info.hbmColor.is_null() {
            DeleteObject(info.hbmColor as _);
        }
        if !info.hbmMask.is_null() {
            DeleteObject(info.hbmMask as _);
        }
        DestroyIcon(icon);
        out
    }
}

fn write_png(out: &std::path::Path, rgba: &[u8], n: u32) -> std::io::Result<()> {
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = out.with_extension("tmp");
    {
        let f = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
        let mut enc = png::Encoder::new(f, n, n);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(std::io::Error::other)?;
        w.write_image_data(rgba).map_err(std::io::Error::other)?;
    }
    std::fs::rename(tmp, out)
}

/// Cache icons for apps in `wanted` (names already in History) that have a window open now, so
/// rows saved before icons existed get theirs too.
pub fn cache_open_app_icons(wanted: std::collections::HashSet<String>) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, IsWindowVisible};
    unsafe extern "system" fn each(h: HWND, l: LPARAM) -> i32 {
        unsafe {
            let wanted = &*(l as *const std::collections::HashSet<String>);
            if IsWindowVisible(h) != 0 {
                let name = icon_owner(h);
                if wanted.contains(&name) {
                    cache_app_icon(h, name);
                }
            }
        }
        1
    }
    let missing: std::collections::HashSet<String> = wanted.into_iter().filter(|a| !icon_file(a).exists()).collect();
    if missing.is_empty() {
        return;
    }
    unsafe {
        EnumWindows(Some(each), &missing as *const _ as LPARAM);
    }
}
