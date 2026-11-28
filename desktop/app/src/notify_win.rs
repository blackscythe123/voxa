//! Windows toasts (WinRT ToastNotification through the existing `windows` crate).
//!
//! An unpackaged app needs an AppUserModelID to post toasts: the installer gives the Start menu
//! shortcut "Voxa.Desktop" (voxa.iss), and the tray process registers the same ID under
//! HKCU\Software\Classes\AppUserModelId with a name and an icon, then claims it with
//! SetCurrentProcessExplicitAppUserModelID. Clicks arrive through the toast's Activated event
//! (the tray process is always running), so no COM activator is needed. Every toast has a Tag
//! and Group so a newer one replaces the older and it can be removed.

use std::sync::Mutex;

use windows::Data::Xml::Dom::XmlDocument;
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::{
    NotificationSetting, ToastActivatedEventArgs, ToastDismissalReason, ToastDismissedEventArgs, ToastNotification, ToastNotificationManager,
};
use windows::core::{HSTRING, IInspectable, Interface};

const GROUP: &str = "voxa";

/// "Voxa.Desktop"; a test copy (VOXA_DATA set) uses its own ID so it never touches the real one.
fn aumid() -> &'static str {
    if std::env::var_os("VOXA_DATA").is_some() { "Voxa.Desktop.Test" } else { "Voxa.Desktop" }
}

/// Toasts shown and not yet removed, kept alive so their Activated events still arrive.
static LIVE: Mutex<Vec<(String, ToastNotification)>> = Mutex::new(Vec::new());

/// Tags of toasts the user closed (or clicked), collected on the toast's thread.
static DISMISSED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Toasts the user closed since the last call: they are no longer on screen.
pub fn take_dismissed() -> Vec<String> {
    std::mem::take(&mut *DISMISSED.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Tests: the user closed the toast with this tag.
#[cfg(test)]
pub fn dismissed_for_test(tag: &str) {
    DISMISSED.lock().unwrap_or_else(|e| e.into_inner()).push(tag.to_string());
}

fn gone(toast: Option<&ToastNotification>) {
    if let Some(tag) = toast.and_then(|t| t.Tag().ok()) {
        DISMISSED.lock().unwrap_or_else(|e| e.into_inner()).push(tag.to_string());
    }
}

pub fn init() -> bool {
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let id: Vec<u16> = aumid().encode_utf16().chain(std::iter::once(0)).collect();
        windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(id.as_ptr());
    }
    register().map_err(|e| crate::os::log(&format!("FAIL registering the notification identity: {e}"))).is_ok()
}

/// The name and icon Windows shows on Voxa's toasts.
fn register() -> std::io::Result<()> {
    let icon = crate::data_dir().join("notify-icon.png");
    if refresh_icon(&icon, include_bytes!("../assets/icon.png"))? {
        crate::os::log("notification icon written");
    }
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(format!(r"Software\Classes\AppUserModelId\{}", aumid()))?;
    key.set_value("DisplayName", &if aumid() == "Voxa.Desktop" { "Voxa" } else { "Voxa (test)" })?;
    key.set_value("IconUri", &icon.to_string_lossy().to_string())?;
    Ok(())
}

/// The toast icon file holds exactly `want`: rewritten (whole or not at all) when it is missing or
/// differs, so a new app icon reaches the toasts after an update; left alone when it matches.
/// True when it was written.
fn refresh_icon(path: &std::path::Path, want: &[u8]) -> std::io::Result<bool> {
    if std::fs::read(path).is_ok_and(|have| have.len() == want.len() && have == want) {
        return Ok(false);
    }
    crate::store::write_atomic(path, want)?;
    Ok(true)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

/// Show (or replace) the toast with `tag`. `button`: (label, arguments). `active`: may make a
/// sound; otherwise it arrives silently (Focus and Do Not Disturb are always respected).
/// `update`: it replaces one already showing, quietly, in Action Center only (no new banner).
pub fn show(tag: &str, title: &str, body: &str, button: Option<(&str, &str)>, launch: &str, active: bool, update: bool) -> bool {
    let actions = button
        .map(|(label, args)| format!(r#"<actions><action content="{}" arguments="{}" activationType="foreground"/></actions>"#, esc(label), esc(args)))
        .unwrap_or_default();
    let audio = if active && !update { "" } else { r#"<audio silent="true"/>"# };
    let xml = format!(
        r#"<toast launch="{}"><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual>{actions}{audio}</toast>"#,
        esc(launch),
        esc(title),
        esc(body)
    );
    match off_ui(|| post(tag, &xml, update)) {
        Ok(()) => {
            crate::os::log(&format!("toast shown: {tag}"));
            true
        }
        Err(e) => {
            crate::os::log(&format!("FAIL toast {tag}: {e}"));
            false
        }
    }
}

/// Runs a toast call on a short-lived worker thread in the multithreaded apartment and waits for
/// it. Called on the UI thread (an STA) the call goes out of process, and COM's modal loop
/// dispatches the strip's window messages to GPUI while it is inside an update: GPUI logged
/// "RefCell already borrowed" at every start (the sign-in toast removed after the session check).
/// Waiting on a thread join pumps nothing.
fn off_ui<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    // The multithreaded apartment stays up for the whole run (toasts made on a worker keep their
    // Activated and Dismissed events after it ends); workers join it implicitly.
    static MTA: std::sync::Once = std::sync::Once::new();
    MTA.call_once(|| {
        if let Err(e) = unsafe { windows::Win32::System::Com::CoIncrementMTAUsage() } {
            crate::os::log(&format!("FAIL keeping the toast apartment: {e}"));
        }
    });
    std::thread::scope(|s| s.spawn(f).join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
}

fn post(tag: &str, xml: &str, update: bool) -> windows::core::Result<()> {
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(aumid()))?;
    // Turned off for Voxa in Windows settings: not shown, so it must not count as posted.
    if notifier.Setting()? != NotificationSetting::Enabled {
        return Err(windows::core::Error::new(windows::core::HRESULT(0x80070005u32 as i32), "notifications are off for Voxa"));
    }
    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(xml))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    toast.SetTag(&HSTRING::from(tag))?;
    toast.SetGroup(&HSTRING::from(GROUP))?;
    if update {
        toast.SetSuppressPopup(true)?;
    }
    toast.Dismissed(&TypedEventHandler::<ToastNotification, ToastDismissedEventArgs>::new(|t, args| {
        // Timed out means it moved to Action Center and is still there.
        if args.as_ref().and_then(|a| a.Reason().ok()) == Some(ToastDismissalReason::UserCanceled) {
            gone(t.as_ref());
        }
        Ok(())
    }))?;
    toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(|_, args| {
        let args = args.as_ref().and_then(|a| a.cast::<ToastActivatedEventArgs>().ok());
        let a = args.and_then(|a| a.Arguments().ok()).map(|s| s.to_string()).unwrap_or_default();
        if let Some(c) = super::parse(&a) {
            super::clicked(c);
        }
        Ok(())
    }))?;
    notifier.Show(&toast)?;
    let mut live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
    live.retain(|(t, _)| t != tag);
    live.push((tag.to_string(), toast));
    Ok(())
}

pub fn remove(tag: &str) {
    LIVE.lock().unwrap_or_else(|e| e.into_inner()).retain(|(t, _)| t != tag);
    let r = off_ui(|| ToastNotificationManager::History().and_then(|h| h.RemoveGroupedTagWithId(&HSTRING::from(tag), &HSTRING::from(GROUP), &HSTRING::from(aumid()))));
    if let Err(e) = r {
        crate::os::log(&format!("toast {tag} not removed: {e}"));
    }
}

/// Windows doesn't ask: toasts are on unless the user turned them off in Settings.
pub fn ask_permission() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::refresh_icon;

    /// After the 1.0.0 icon change toasts kept the old picture: the file was written only when missing.
    #[test]
    fn an_old_toast_icon_is_replaced_and_a_current_one_is_left_alone() {
        let dir = std::env::temp_dir().join(format!("voxa-icon-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("notify-icon.png");
        let new_icon = b"new icon bytes".as_slice();
        assert!(refresh_icon(&path, new_icon).unwrap(), "a missing icon is written");
        assert_eq!(std::fs::read(&path).unwrap(), new_icon);
        std::fs::write(&path, b"old icon").unwrap();
        assert!(refresh_icon(&path, new_icon).unwrap(), "an old icon is replaced");
        assert_eq!(std::fs::read(&path).unwrap(), new_icon);
        std::fs::write(&path, b"new icon byteZ").unwrap();
        assert!(refresh_icon(&path, new_icon).unwrap(), "same length, different bytes: replaced");
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(!refresh_icon(&path, new_icon).unwrap(), "an identical icon is left alone");
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// "RefCell already borrowed" at every start: toast calls never run on the UI thread, where
    /// COM's modal loop re-entered GPUI. Every WinRT toast call goes through off_ui.
    #[test]
    fn toast_calls_run_off_the_ui_thread() {
        let src = include_str!("notify_win.rs").replace("\r\n", "\n");
        let code = &src[..src.find("#[cfg(test)]\nmod tests").unwrap()];
        assert!(code.contains("off_ui(|| post("), "show posts off the UI thread");
        for line in code.lines().filter(|l| l.contains("ToastNotificationManager::History()")) {
            assert!(line.contains("off_ui("), "History() off the UI thread: {line}");
        }
        assert_eq!(code.matches("post(tag").count(), 2, "post is defined once and called once (through off_ui)");
    }
}
