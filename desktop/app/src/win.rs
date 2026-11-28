//! Windows: the pieces GPUI does not provide (the `os` module on Windows). Shared code is in
//! common.rs. Win32 pieces GPUI does not provide: a non-activating topmost overlay,
//! foreground-window inspection, and paste-with-clipboard-restore.

use std::ffi::c_void;

pub use crate::common::*;

pub const START_AT_LOGIN: &str = "Start with Windows";
pub const START_AT_LOGIN_LONG: &str = "Start Voxa when I sign in to Windows";
pub use crate::tab::{coding_agent, domain_of};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DWMNCRP_DISABLED, DWMWA_BORDER_COLOR, DWMWA_NCRENDERING_POLICY, DwmSetWindowAttribute};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard,
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
        // Modern Standby only tells windows that ask about waking up (asked once: the rebuilt
        // strip isn't asked again; a closed window's registration ends with it).
        const DEVICE_NOTIFY_WINDOW_HANDLE: u32 = 0;
        windows_sys::Win32::System::Power::RegisterSuspendResumeNotification(hwnd, DEVICE_NOTIFY_WINDOW_HANDLE);
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
    // Woken from sleep or standby: the strip's graphics may be gone (it stayed invisible after
    // Modern Standby; a charger plugged in or out on a laptop with two graphics chips), so the
    // strip is rebuilt. These messages only set a flag: the rebuild happens at the user's next
    // shortcut press, never on its own (full-screen games change the display mode constantly).
    const WM_POWERBROADCAST: u32 = 0x0218;
    const WM_DISPLAYCHANGE: u32 = 0x007E;
    const PBT_APMRESUMESUSPEND: usize = 0x0007;
    const PBT_APMPOWERSTATUSCHANGE: usize = 0x000A;
    const PBT_APMRESUMEAUTOMATIC: usize = 0x0012;
    // GPUI answers a display change by showing (and activating) its windows, the hidden strip
    // too: undo that at once and hand focus back, so nothing pops up and no app loses focus.
    if msg == WM_DISPLAYCHANGE {
        WOKE.store(true, std::sync::atomic::Ordering::Relaxed);
        let was_visible = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd) } != 0;
        let fg = foreground();
        let r = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
        if !was_visible && unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd) } != 0 {
            unsafe { ShowWindow(hwnd, SW_HIDE) };
            log("display changed: GPUI showed the hidden strip; hid it again");
        }
        if foreground() == hwnd && !fg.is_null() && fg != hwnd {
            unsafe { SetForegroundWindow(fg) };
            log("display changed: gave focus back");
        }
        return r;
    }
    let why = match (msg, wparam) {
        (WM_POWERBROADCAST, PBT_APMRESUMESUSPEND | PBT_APMRESUMEAUTOMATIC) => Some("woke from sleep"),
        (WM_POWERBROADCAST, PBT_APMPOWERSTATUSCHANGE) => Some("power source changed"),
        (WM_DISPLAYCHANGE, _) => Some("display changed"),
        _ => None,
    };
    if let Some(why) = why {
        if !WOKE.swap(true, std::sync::atomic::Ordering::Relaxed) {
            log(&format!("strip will be rebuilt at the next shortcut: {why}"));
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// Everything on the clipboard, format by format, so it can be put back as it was: text,
/// images, copied files, rich text. Formats that aren't plain memory (bitmaps, metafiles,
/// palettes, owner-drawn) are left out: Windows makes them again from the memory ones.
struct Saved {
    formats: Vec<(u32, Vec<u8>)>,
}

/// Never hold more than this much of the user's clipboard in memory; bigger is left alone.
const SAVE_LIMIT: usize = 64 << 20;

fn is_memory_format(f: u32) -> bool {
    // CF_BITMAP 2, CF_METAFILEPICT 3, CF_PALETTE 9, CF_ENHMETAFILE 14, CF_OWNERDISPLAY 0x80,
    // CF_DSPBITMAP 0x82, CF_DSPMETAFILEPICT 0x83, CF_DSPENHMETAFILE 0x8E, GDI objects 0x300-0x3FF.
    !matches!(f, 2 | 3 | 9 | 14 | 0x80 | 0x82 | 0x83 | 0x8E | 0x300..=0x3FF)
}

/// Must run with the clipboard open. None when it can't all be saved (too big).
fn save_clipboard() -> Option<Saved> {
    use windows_sys::Win32::System::DataExchange::EnumClipboardFormats;
    use windows_sys::Win32::System::Memory::GlobalSize;
    let mut formats = Vec::new();
    let mut total = 0usize;
    let mut f = 0u32;
    unsafe {
        loop {
            f = EnumClipboardFormats(f);
            if f == 0 {
                break;
            }
            if !is_memory_format(f) {
                continue;
            }
            let h = GetClipboardData(f);
            if h.is_null() {
                continue;
            }
            let size = GlobalSize(h as _);
            if size == 0 {
                continue;
            }
            total += size;
            if total > SAVE_LIMIT {
                return None;
            }
            let p = GlobalLock(h as _) as *const u8;
            if p.is_null() {
                continue;
            }
            formats.push((f, std::slice::from_raw_parts(p, size).to_vec()));
            GlobalUnlock(h as _);
        }
    }
    Some(Saved { formats })
}

/// Must run with the clipboard open and emptied by this process.
fn put_format(format: u32, bytes: &[u8]) -> bool {
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
        if h.is_null() {
            return false;
        }
        let p = GlobalLock(h) as *mut u8;
        if p.is_null() {
            return false;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        GlobalUnlock(h);
        !SetClipboardData(format, h as *mut c_void).is_null()
    }
}

fn registered(name: &str) -> u32 {
    unsafe { windows_sys::Win32::System::DataExchange::RegisterClipboardFormatW(wide(name).as_ptr()) }
}

/// Keep what Voxa puts on the clipboard only to paste it out of Windows clipboard history
/// (Win+V), Cloud Clipboard sync and clipboard managers.
fn mark_private(and_monitors: bool) {
    let no = 0u32.to_le_bytes();
    put_format(registered("CanIncludeInClipboardHistory"), &no);
    put_format(registered("CanUploadToCloudClipboard"), &no);
    if and_monitors {
        put_format(registered("ExcludeClipboardContentFromMonitorProcessing"), &no);
    }
}

fn write_clipboard_text(text: &str, private: bool) -> bool {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes: Vec<u8> = wide.iter().flat_map(|c| c.to_le_bytes()).collect();
    unsafe {
        if EmptyClipboard() == 0 {
            return false;
        }
    }
    let ok = put_format(CF_UNICODETEXT as u32, &bytes);
    if ok && private {
        mark_private(true);
    }
    ok
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

fn clipboard_sequence() -> u32 {
    unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}

/// How long the app in front gets to read the clipboard before what was there is put back.
/// Not on the UI thread, so it can be generous: slow readers (Remote Desktop, VMs, busy
/// Electron apps) would otherwise paste the old clipboard.
const RESTORE_AFTER: std::time::Duration = std::time::Duration::from_millis(1500);

fn key(vk: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { 0 }, time: 0, dwExtraInfo: 0 },
        },
    }
}

/// The integrity level of a process token (0x2000 medium, 0x3000 high: "Run as administrator").
unsafe fn token_integrity(token: windows_sys::Win32::Foundation::HANDLE) -> Option<u32> {
    use windows_sys::Win32::Security::{GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL, TokenIntegrityLevel};
    let mut buf = [0u8; 128];
    let mut len = 0u32;
    if unsafe { GetTokenInformation(token, TokenIntegrityLevel, buf.as_mut_ptr() as _, buf.len() as u32, &mut len) } == 0 {
        return None;
    }
    let label = buf.as_ptr() as *const TOKEN_MANDATORY_LABEL;
    let sid = unsafe { (*label).Label.Sid };
    let count = unsafe { *GetSidSubAuthorityCount(sid) };
    if count == 0 {
        return None;
    }
    Some(unsafe { *GetSidSubAuthority(sid, count as u32 - 1) })
}

fn own_integrity() -> u32 {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::TOKEN_QUERY;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut t = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut t) == 0 {
            return 0x2000;
        }
        let l = token_integrity(t).unwrap_or(0x2000);
        CloseHandle(t);
        l
    }
}

/// The integrity of the process that owns a window, as far as Voxa may know it.
fn window_integrity(hwnd: HWND) -> crate::rules::Integrity {
    use crate::rules::Integrity;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::TOKEN_QUERY;
    use windows_sys::Win32::System::Threading::{OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return Integrity::Unknown;
        }
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if p.is_null() {
            return Integrity::Unknown;
        }
        let mut t = std::ptr::null_mut();
        let r = if OpenProcessToken(p, TOKEN_QUERY, &mut t) == 0 {
            // An elevated process's token can't be read from a normal one.
            Integrity::Hidden
        } else {
            let l = token_integrity(t);
            CloseHandle(t);
            l.map(Integrity::Level).unwrap_or(Integrity::Unknown)
        };
        CloseHandle(p);
        r
    }
}

/// Paste `text` into the window in front. With `keep`, the text stays on the clipboard (so
/// Ctrl+V pastes it again anywhere); without, whatever was on the clipboard (text, image,
/// files) is put back afterwards, unless something new was copied meanwhile, and the text
/// never reaches clipboard history or Cloud Clipboard.
/// An admin window drops keys from a normal process without a word (UIPI): then the text is
/// left on the clipboard for the user's own Ctrl+V and Voxa says so.
/// The caller has already checked the right window is in front.
pub fn paste_here(text: &str, keep: bool) -> crate::rules::PasteResult {
    use crate::rules::PasteResult;
    let fg = foreground();
    if crate::rules::paste_refused(own_integrity(), window_integrity(fg)) {
        let copied = with_clipboard(|| write_clipboard_text(text, false)) == Some(true);
        log(&format!("{} runs as administrator: not pasting, copied={copied}", describe(fg)));
        return if copied { PasteResult::Refused } else { PasteResult::Failed };
    }
    let before = if keep { None } else { with_clipboard(save_clipboard).flatten() };
    if with_clipboard(|| write_clipboard_text(text, !keep)) != Some(true) {
        log("FAIL could not write clipboard");
        return PasteResult::Failed;
    }
    let ours = clipboard_sequence();
    let inputs = [key(VK_CONTROL, false), key(b'V' as u16, false), key(b'V' as u16, true), key(VK_CONTROL, true)];
    let sent = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
    log(&format!("sent Ctrl+V ({sent} events) to {}", describe(fg)));
    if !keep {
        // Couldn't be saved (busy, or too big): leave the dictation there rather than wipe it.
        match before {
            Some(saved) => {
                std::thread::spawn(move || {
                    std::thread::sleep(RESTORE_AFTER);
                    let restored = with_clipboard(|| {
                        // Something new was copied meanwhile: that stays.
                        if clipboard_sequence() != ours {
                            return false;
                        }
                        if unsafe { EmptyClipboard() } == 0 {
                            return false;
                        }
                        let all = saved.formats.iter().all(|(f, b)| put_format(*f, b));
                        if !saved.formats.is_empty() {
                            mark_private(false);
                        }
                        all
                    });
                    log(&format!("clipboard restored={restored:?}"));
                });
            }
            None => log("clipboard couldn't be saved: the dictation stays on it"),
        }
    }
    if sent == 4 { PasteResult::Pasted } else { PasteResult::Failed }
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

/// Sites recognised from a browser tab's title: (text to find, name to show, domain for its icon).
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

pub fn window_process(hwnd: HWND) -> (u32, String) {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    let stem = std::path::Path::new(&exe_path(hwnd)).file_stem().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    (pid, stem)
}

/// Icon for a site that is not on the known list, straight from the site.
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
        "explorer" => Some("File Explorer"),
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

/// A window of Voxa itself (the strip, or the main window in its own process).
pub fn is_own_window(hwnd: HWND) -> bool {
    let exe = exe_path(hwnd);
    let me = std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    !exe.is_empty() && exe.eq_ignore_ascii_case(&me)
}

/// Bring another app's window to the front so a paste lands there. Windows only allows this
/// from the foreground app, which Voxa is when its own window was in front.
pub fn switch_to(hwnd: HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindow, SW_RESTORE};
    unsafe {
        if IsWindow(hwnd) == 0 {
            return;
        }
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd);
    }
    for _ in 0..15 {
        if foreground() == hwnd {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    std::thread::sleep(std::time::Duration::from_millis(80));
}

pub static STRIP: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static WOKE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// True once after the PC woke up or the screens changed (the strip should be rebuilt).
pub fn take_woke() -> bool {
    WOKE.swap(false, std::sync::atomic::Ordering::Relaxed)
}


/// Free space on the disk holding `path`, in bytes.
pub fn disk_free_bytes(path: &std::path::Path) -> Option<u64> {
    let w = wide(&path.to_string_lossy());
    let mut free = 0u64;
    let ok = unsafe { windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(w.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
    (ok != 0).then_some(free)
}

/// The strip's outer rectangle in physical pixels (x, y, width, height), frame included.
pub fn strip_rect() -> Option<(i32, i32, i32, i32)> {
    let h = STRIP.load(std::sync::atomic::Ordering::Relaxed) as HWND;
    let mut r = windows_sys::Win32::Foundation::RECT { left: 0, top: 0, right: 0, bottom: 0 };
    (!h.is_null() && unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect(h, &mut r) } != 0).then(|| (r.left, r.top, r.right - r.left, r.bottom - r.top))
}

/// Put the strip at the bottom centre of the main screen as it is now (resolution, scaling and
/// monitors change while Voxa runs). Keeps its size; never shows or activates it.
pub fn place_strip() {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint};
    let Some((_, _, w, h)) = strip_rect() else { return };
    let hw = STRIP.load(std::sync::atomic::Ordering::Relaxed) as HWND;
    unsafe {
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY), &mut mi) == 0 {
            return;
        }
        let m = mi.rcMonitor;
        let scale = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hw) as f32 / 96.0;
        // Same spot as at startup: 64 px (logical) above the bottom edge, centred.
        let inner_h = (84.0 * scale).round() as i32;
        let frame = ((h - inner_h) / 2).max(0);
        let x = m.left + (m.right - m.left - w) / 2;
        let y = m.bottom - (64.0 * scale).round() as i32 - inner_h - frame;
        // Only when it's really out of place (a screen change): a needless move makes GPUI log
        // "RefCell already borrowed" while it's busy with the shortcut press.
        let Some((cx_, cy_, _, _)) = strip_rect() else { return };
        if (cx_ - x).abs() > 8 || (cy_ - y).abs() > 8 {
            log(&format!("strip moved to the main screen's bottom centre ({cx_},{cy_} -> {x},{y})"));
            SetWindowPos(hw, HWND_TOPMOST, x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

/// Place the strip (physical pixels) without showing or activating it.
pub fn move_strip(x: i32, y: i32, w: i32, h: i32) {
    let hw = STRIP.load(std::sync::atomic::Ordering::Relaxed) as HWND;
    if !hw.is_null() {
        unsafe { SetWindowPos(hw, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE) };
    }
}

/// A dev/test instance (its own data folder) runs beside the installed Voxa.
fn instance_suffix() -> String {
    std::env::var("VOXA_DATA").map(|d| format!("-{:x}", d.bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)))).unwrap_or_default()
}

/// The strip is on screen now (the update gate waits while it is).
pub fn strip_shown() -> bool {
    let h = STRIP.load(std::sync::atomic::Ordering::Relaxed) as HWND;
    !h.is_null() && unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(h) } != 0
}

pub fn strip_visible(show: bool) {
    let h = STRIP.load(std::sync::atomic::Ordering::Relaxed) as HWND;
    if h.is_null() { return; }
    if show && unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(h) } == 0 {
        place_strip();
    }
    // Async: GPUI may be mid-way through handling the shortcut press; a synchronous show makes it
    // log "RefCell already borrowed" and skip the size and position messages.
    unsafe { windows_sys::Win32::UI::WindowsAndMessaging::ShowWindowAsync(h, if show { SW_SHOWNOACTIVATE } else { SW_HIDE }) };
    log(&format!("strip {}; foreground = {}", if show { "shown (no activate)" } else { "hidden" }, describe(foreground())));
}

/// The caption bar in the nav colour (0xRRGGBB), so the window reads as one surface; `dark` also
/// turns on the dark caption buttons (DWMWA_USE_IMMERSIVE_DARK_MODE). Without this the caption
/// follows the system's mode and can disagree with Voxa's Appearance setting.
pub fn window_caption(hwnd: HWND, dark: bool, caption: u32, text: u32) {
    // COLORREF is 0x00BBGGRR.
    let bgr = |c: u32| (c & 0xFF) << 16 | (c & 0xFF00) | (c >> 16 & 0xFF);
    unsafe {
        let on: i32 = dark as i32;
        DwmSetWindowAttribute(hwnd, 20, &on as *const i32 as *const c_void, 4);
        let caption = bgr(caption);
        DwmSetWindowAttribute(hwnd, 35, &caption as *const u32 as *const c_void, 4);
        let text = bgr(text);
        DwmSetWindowAttribute(hwnd, 36, &text as *const u32 as *const c_void, 4);
    }
}

/// macOS only (the window's own appearance); Windows draws the caption with `window_caption`.
pub fn window_follows(_hwnd: HWND, _system: bool, _dark: bool) {}

// ── single instance ──────────────────────────────────────────────────────

/// Set when a second launch (or the Start menu shortcut) asks this instance to show its window.

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// True if this is the only Voxa running. Otherwise asks the running one to open its window and
/// returns false, and the caller exits.
pub fn single_instance() -> bool {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::{CreateEventW, CreateMutexW, INFINITE, SetEvent, WaitForSingleObject};
    unsafe {
        let sfx = instance_suffix();
        let mutex = CreateMutexW(std::ptr::null(), 0, wide(&format!(r"Local\VoxaNextSingleInstance{sfx}")).as_ptr());
        let already = GetLastError() == ERROR_ALREADY_EXISTS;
        let event = CreateEventW(std::ptr::null(), 0, 0, wide(&format!(r"Local\VoxaNextOpenWindow{sfx}")).as_ptr());
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

/// The event the window process sets after it drops a command file, so the tray process reads
/// the commands folder only when there is something in it (not 25 times a second).
fn command_event() -> windows_sys::Win32::Foundation::HANDLE {
    use windows_sys::Win32::System::Threading::CreateEventW;
    static EVENT: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *EVENT.get_or_init(|| unsafe { CreateEventW(std::ptr::null(), 0, 0, wide(&format!(r"Local\VoxaNextCommand{}", instance_suffix())).as_ptr()) as usize }) as _
}

/// Window process: a command was written.
pub fn commands_signal() {
    let e = command_event();
    if !e.is_null() {
        unsafe { windows_sys::Win32::System::Threading::SetEvent(e) };
    }
}

/// Tray process: whether a command was written since the last look (no waiting).
pub fn commands_signalled() -> bool {
    let e = command_event();
    !e.is_null() && unsafe { windows_sys::Win32::System::Threading::WaitForSingleObject(e, 0) } == 0
}

/// The window process's own single instance. False if the window is already open (it is then
/// asked to come to the front). Suffixed like the tray's, so a test copy never meets the real one.
pub fn single_window_instance() -> bool {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::{CreateEventW, CreateMutexW, INFINITE, SetEvent, WaitForSingleObject};
    unsafe {
        // A test copy (VOXA_DATA) has its own names, so it never raises or answers the real window.
        let test = if std::env::var_os("VOXA_DATA").is_some() { "Test" } else { "" };
        let mutex = CreateMutexW(std::ptr::null(), 0, wide(&format!(r"Local\VoxaNextWindow{test}")).as_ptr());
        let already = GetLastError() == ERROR_ALREADY_EXISTS;
        let event = CreateEventW(std::ptr::null(), 0, 0, wide(&format!(r"Local\VoxaNextWindowFront{test}")).as_ptr());
        if already {
            if !event.is_null() {
                SetEvent(event);
            }
            return false;
        }
        let _ = mutex;
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

// ── opening things ───────────────────────────────────────────────────────

/// Show a file selected in Explorer.
pub fn reveal(path: &std::path::Path) {
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
}

/// Open a file, folder or settings page with its default handler.
pub fn open(target: &std::ffi::OsStr) {
    let _ = std::process::Command::new("explorer").arg(target).spawn();
}

pub fn open_mic_privacy() {
    open(std::ffi::OsStr::new("ms-settings:privacy-microphone"));
}

pub fn open_sound_settings() {
    open(std::ffi::OsStr::new("ms-settings:sound"));
}

/// Pointer for a GPUI window, for the functions above that take one.
pub fn window_ptr(w: &gpui::Window) -> Option<HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(w).map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(h)) => Some(h.hwnd.get() as HWND),
        _ => None,
    }
}

/// The window process shows in the taskbar on its own; nothing to do on Windows.
pub fn show_in_dock() {}

/// Windows can always paste with SendInput; nothing to ask for.
pub fn can_paste() -> bool {
    true
}

// ── the update gate (crate::update) ──────────────────────────────────────

/// Milliseconds since the last keyboard or mouse input in this session.
pub fn idle_ms() -> u64 {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    if unsafe { GetLastInputInfo(&mut info) } == 0 {
        return 0;
    }
    u64::from(unsafe { GetTickCount() }.wrapping_sub(info.dwTime))
}

/// The session is locked (the lock screen owns the input desktop).
pub fn session_locked() -> bool {
    use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, DESKTOP_SWITCHDESKTOP, OpenInputDesktop, SwitchDesktop};
    unsafe {
        let d = OpenInputDesktop(0, 0, DESKTOP_SWITCHDESKTOP);
        if d.is_null() {
            return true;
        }
        let switched = SwitchDesktop(d) != 0;
        CloseDesktop(d);
        !switched
    }
}

/// The window process (`--window`) is running: its mutex exists.
pub fn window_process_running() -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::OpenMutexW;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    let test = if std::env::var_os("VOXA_DATA").is_some() { "Test" } else { "" };
    unsafe {
        let h = OpenMutexW(SYNCHRONIZE, 0, wide(&format!(r"Local\VoxaNextWindow{test}")).as_ptr());
        if h.is_null() {
            return false;
        }
        CloseHandle(h);
        true
    }
}

/// A process with this exe name (case-insensitive) is running, e.g. "voxa-signin.exe".
pub fn process_running(exe: &str) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS};
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return false };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut found = false;
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            let len = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
            if String::from_utf16_lossy(&e.szExeFile[..len]).eq_ignore_ascii_case(exe) {
                found = true;
                break;
            }
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
        found
    }
}

/// Start a program detached from Voxa, with no window, passing `args` exactly as written (the
/// installer's own quoting, e.g. /LOG="C:\a b\install.log").
pub fn spawn_detached(program: &std::path::Path, args: &[String]) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut c = std::process::Command::new(program);
    for a in args {
        c.raw_arg(a);
    }
    c.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW).spawn().map(|_| ())
}
