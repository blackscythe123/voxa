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
    if !icon.exists() {
        std::fs::write(&icon, include_bytes!("../assets/icon.png"))?;
    }
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(format!(r"Software\Classes\AppUserModelId\{}", aumid()))?;
    key.set_value("DisplayName", &if aumid() == "Voxa.Desktop" { "Voxa" } else { "Voxa (test)" })?;
    key.set_value("IconUri", &icon.to_string_lossy().to_string())?;
    Ok(())
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
    match post(tag, &xml, update) {
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
    let r = ToastNotificationManager::History().and_then(|h| h.RemoveGroupedTagWithId(&HSTRING::from(tag), &HSTRING::from(GROUP), &HSTRING::from(aumid())));
    if let Err(e) = r {
        crate::os::log(&format!("toast {tag} not removed: {e}"));
    }
}

/// Windows doesn't ask: toasts are on unless the user turned them off in Settings.
pub fn ask_permission() -> bool {
    true
}
