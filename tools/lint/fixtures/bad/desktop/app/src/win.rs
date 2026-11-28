// Fixture for privacy-log (describe): a window description that reads the title.

// Bad: the window title (an email subject, a chat name) would go into the log.
pub fn describe(hwnd: HWND) -> String {
    let mut title = [0u16; 256];
    let n = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) };
    format!("{:?} {}", hwnd, String::from_utf16_lossy(&title[..n as usize]))
}

// Good: other code may read a title (to recognise a website), it is just never logged here.
pub fn site_in_tab(hwnd: HWND) -> Option<String> {
    let mut title = [0u16; 256];
    let n = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) };
    Some(String::from_utf16_lossy(&title[..n as usize]))
}

// Fixture for robust-strip-sync: synchronous strip calls from code GPUI may be inside.
// Bad: moved and shown right away (1.0.0 "RefCell already borrowed" at start).
pub fn place_strip() {
    unsafe { SetWindowPos(hw, HWND_TOPMOST, x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE) };
}

pub fn strip_visible(show: bool) {
    unsafe { ShowWindowAsync(h, crate::strip_rules::show_command(show)) };
}

// Good: posted, carried out from the message loop.
fn place_now(hw: HWND) {
    unsafe { SetWindowPos(hw, HWND_TOPMOST, x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE) };
}
