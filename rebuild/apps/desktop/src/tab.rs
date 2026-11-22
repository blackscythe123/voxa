//! Which website a browser window is showing, read from its address bar with UI Automation.
//!
//! Runs on a worker thread while the user talks, so the strip still opens instantly. Only the
//! domain is kept ("mangafire.to"); the path and query never leave this module.

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationValuePattern, TreeScope_Descendants, UIA_ControlTypePropertyId, UIA_EditControlTypeId,
    UIA_ValuePatternId,
};

/// The domain in the address bar of a browser window, e.g. "mangafire.to".
pub fn domain_of(hwnd: usize) -> Option<String> {
    unsafe {
        let init = CoInitializeEx(None, COINIT_MULTITHREADED);
        let out = (|| -> windows::core::Result<Option<String>> {
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            let root = uia.ElementFromHandle(HWND(hwnd as _))?;
            // The address bar is the browser's first edit box (Chromium's omnibox, Firefox's urlbar).
            let cond = uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_EditControlTypeId.0))?;
            let edit = root.FindFirst(TreeScope_Descendants, &cond)?;
            let value: IUIAutomationValuePattern = edit.GetCurrentPatternAs(UIA_ValuePatternId)?;
            Ok(host_of(&value.CurrentValue()?.to_string()))
        })()
        .ok()
        .flatten();
        if init.is_ok() {
            CoUninitialize();
        }
        out
    }
}

/// "https://www.mangafire.to/home?x=1" -> "mangafire.to". None for search text or empty bars.
pub fn host_of(address: &str) -> Option<String> {
    let a = address.trim();
    let a = a.split_once("://").map(|(_, r)| r).unwrap_or(a);
    let host = a.split(['/', '?', '#']).next()?.split('@').next_back()?.split(':').next()?.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    // A domain has a dot, no spaces, and only domain characters (not "rust ownership").
    let ok = host.contains('.') && !host.starts_with('.') && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    ok.then_some(host)
}

#[cfg(test)]
mod tests {
    use super::host_of;

    #[test]
    fn hosts_from_address_bars() {
        assert_eq!(host_of("mangafire.to/home").as_deref(), Some("mangafire.to"));
        assert_eq!(host_of("https://www.youtube.com/watch?v=1").as_deref(), Some("youtube.com"));
        assert_eq!(host_of("chatgpt.com/c/abc").as_deref(), Some("chatgpt.com"));
        assert_eq!(host_of("http://localhost:3000/").as_deref(), None);
        assert_eq!(host_of("rust ownership rules"), None, "search text in the bar is not a site");
        assert_eq!(host_of(""), None);
    }
}

#[cfg(test)]
mod probe {
    /// Manual: VOXA_PROBE_HWND=<browser window handle> cargo test probe_real_window -- --ignored --nocapture
    #[test]
    #[ignore]
    fn probe_real_window() {
        let hwnd: usize = std::env::var("VOXA_PROBE_HWND").unwrap().parse().unwrap();
        let t = std::time::Instant::now();
        let d = super::domain_of(hwnd);
        eprintln!("domain: {d:?} in {:?}", t.elapsed());
        assert!(d.is_some());
    }
}
