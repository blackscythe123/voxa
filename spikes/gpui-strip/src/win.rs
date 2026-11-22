//! Win32 pieces GPUI does not provide: a non-activating topmost overlay,
//! foreground-window inspection, and paste-with-clipboard-restore.

use std::ffi::c_void;
use std::io::Write;
use std::sync::OnceLock;
use std::time::Instant;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_BORDER_COLOR, DwmSetWindowAttribute};
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

/// Append a line to spike.log (next to the working directory) and stdout.
pub fn log(msg: &str) {
    let t = START.get_or_init(Instant::now).elapsed().as_millis();
    let line = format!("[{t:>7}ms] {msg}");
    println!("{line}");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("spike.log") {
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

/// Paste `text` into `target` and put the user's clipboard back afterwards.
/// Spike scope: restores plain text only (the real app must save every format).
pub fn paste_into(target: HWND, text: &str) {
    let before = with_clipboard(read_clipboard_text).flatten();
    log(&format!("clipboard before paste: {:?}", before));
    if with_clipboard(|| write_clipboard_text(text)) != Some(true) {
        log("FAIL could not write clipboard");
        return;
    }
    let fg = foreground();
    if fg != target {
        log(&format!("target not in front ({}), bringing it forward", describe(fg)));
        unsafe { SetForegroundWindow(target) };
        std::thread::sleep(std::time::Duration::from_millis(80));
    }
    let inputs = [key(VK_CONTROL, false), key(b'V' as u16, false), key(b'V' as u16, true), key(VK_CONTROL, true)];
    let sent = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
    log(&format!("sent Ctrl+V ({sent} events) to {}", describe(foreground())));
    std::thread::sleep(std::time::Duration::from_millis(250));
    let restored = with_clipboard(|| match &before {
        Some(s) => write_clipboard_text(s),
        None => unsafe { EmptyClipboard() != 0 },
    });
    let after = with_clipboard(read_clipboard_text).flatten();
    log(&format!("clipboard restored={:?}, now: {:?}", restored, after));
}

/// Show the overlay without activating it (the window is created hidden).
pub fn show_no_activate(hwnd: HWND) {
    unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
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
