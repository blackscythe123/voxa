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

/// Names of the focused element and its ancestors (nearest first), e.g. to tell Claude Code's
/// panel from the editor inside VS Code. Only element names, never their text content.
pub fn focus_path(max: usize) -> Vec<String> {
    use windows::Win32::UI::Accessibility::IUIAutomationTreeWalker;
    unsafe {
        let init = CoInitializeEx(None, COINIT_MULTITHREADED);
        let out = (|| -> windows::core::Result<Vec<String>> {
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            let walker: IUIAutomationTreeWalker = uia.ControlViewWalker()?;
            let mut el = uia.GetFocusedElement()?;
            let mut names = Vec::new();
            for _ in 0..max {
                let n = el.CurrentName().map(|b| b.to_string()).unwrap_or_default();
                if !n.trim().is_empty() {
                    names.push(n.chars().take(80).collect());
                }
                match walker.GetParentElement(&el) {
                    Ok(p) => el = p,
                    Err(_) => break,
                }
            }
            Ok(names)
        })()
        .unwrap_or_default();
        if init.is_ok() {
            CoUninitialize();
        }
        out
    }
}

#[cfg(test)]
mod focus_probe {
    #[test]
    #[ignore]
    fn probe_focus_path() {
        let t = std::time::Instant::now();
        let p = super::focus_path(25);
        eprintln!("{} names in {:?}", p.len(), t.elapsed());
        for n in p {
            eprintln!("  {n}");
        }
    }
}

/// Editors and terminals that can host Claude Code.
const HOSTS: &[(&str, &str)] = &[("code", "VS Code"), ("cursor", "Cursor"), ("windsurf", "Windsurf"), ("windowsterminal", "Terminal")];

/// "Claude Code" when the focused part of an editor or terminal is Claude Code: its extension
/// panel, or a terminal running the claude CLI. Returns (app, host), e.g. ("Claude Code", "VS Code").
pub fn coding_agent(exe_stem: &str, pid: u32) -> Option<(String, String)> {
    let host = HOSTS.iter().find(|(s, _)| *s == exe_stem)?.1.to_string();
    let path = focus_path(8);
    let first = path.first().map(|s| s.as_str()).unwrap_or("");
    let names = path.join(" | ").to_lowercase();
    let in_terminal = first.starts_with("Terminal") || exe_stem == "windowsterminal";
    // Claude Code titles its terminal with a status mark (idle or working).
    let claude_title = first.chars().take(24).any(|c| matches!(c, '✳' | '✻' | '✽' | '◐' | '◑' | '◒' | '◓' | '✶' | '✢' | '·'));
    let is_claude = names.contains("claude") || (in_terminal && (claude_title || claude_under(pid)));
    is_claude.then(|| ("Claude Code".to_string(), host))
}

/// Is a claude.exe running somewhere under process `root` (e.g. in one of VS Code's terminals)?
fn claude_under(root: u32) -> bool {
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS};
    let mut procs: Vec<(u32, u32, String)> = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return false };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(snap, &mut e).is_ok() {
            loop {
                let len = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
                procs.push((e.th32ProcessID, e.th32ParentProcessID, String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase()));
                if Process32NextW(snap, &mut e).is_err() {
                    break;
                }
            }
        }
        let _ = windows::Win32::Foundation::CloseHandle(snap);
    }
    procs.iter().filter(|(_, _, n)| n == "claude.exe").any(|(pid, _, _)| {
        let mut cur = *pid;
        for _ in 0..12 {
            match procs.iter().find(|(p, _, _)| *p == cur) {
                Some((_, parent, _)) if *parent == root => return true,
                Some((_, parent, _)) if *parent != 0 && *parent != cur => cur = *parent,
                _ => return false,
            }
        }
        false
    })
}

/// What the focused element looks like to UI Automation, for deciding whether a paste lands.
/// Only types, classes and flags; never the element's text.
#[allow(dead_code)] // used by the text-box check (in progress)
#[derive(Debug, Default, Clone)]
pub struct FocusInfo {
    pub control_type: i32,
    pub class: String,
    pub framework: String,
    /// Some(read_only) when the element has a Value pattern.
    pub value_read_only: Option<bool>,
    pub has_text_pattern: bool,
    pub keyboard_focusable: bool,
}

#[allow(dead_code)]
pub fn focus_info() -> Option<FocusInfo> {
    use windows::Win32::UI::Accessibility::UIA_TextPatternId;
    unsafe {
        let init = CoInitializeEx(None, COINIT_MULTITHREADED);
        let out = (|| -> windows::core::Result<FocusInfo> {
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            let el = uia.GetFocusedElement()?;
            let value_read_only = el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId).ok().and_then(|v| v.CurrentIsReadOnly().ok()).map(|b| b.as_bool());
            Ok(FocusInfo {
                control_type: el.CurrentControlType().map(|c| c.0).unwrap_or(0),
                class: el.CurrentClassName().map(|b| b.to_string()).unwrap_or_default(),
                framework: el.CurrentFrameworkId().map(|b| b.to_string()).unwrap_or_default(),
                value_read_only,
                has_text_pattern: el.GetCurrentPatternAs::<windows::Win32::UI::Accessibility::IUIAutomationTextPattern>(UIA_TextPatternId).is_ok(),
                keyboard_focusable: el.CurrentIsKeyboardFocusable().is_ok_and(|b| b.as_bool()),
            })
        })()
        .ok();
        if init.is_ok() {
            CoUninitialize();
        }
        out
    }
}

#[cfg(test)]
mod focus_info_probe {
    /// Manual: prints the focused element once a second for 40 s while you click around.
    #[test]
    #[ignore]
    fn probe_focus_info() {
        for i in 0..150 {
            let (_, exe) = crate::os::window_process(crate::os::foreground());
            eprintln!("{i:>3}s {exe:<20} {:?}", super::focus_info());
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}
