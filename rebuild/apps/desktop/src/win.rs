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
pub fn app_name(hwnd: HWND) -> String {
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    use windows_sys::Win32::Foundation::CloseHandle;
    let mut pid = 0u32;
    let exe = unsafe {
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
    };
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
