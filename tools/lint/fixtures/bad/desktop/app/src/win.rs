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
