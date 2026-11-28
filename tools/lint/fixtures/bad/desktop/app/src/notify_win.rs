// Fixture for robust-toast-thread: toast calls on the UI thread.

// Bad: both run on the UI thread.
pub fn show(tag: &str, xml: &str) -> bool {
    post(tag, xml).is_ok()
}

pub fn remove(tag: &str) {
    let _ = ToastNotificationManager::History().and_then(|h| h.RemoveGroupedTagWithId(tag));
}

// Good: through off_ui; post itself may call the manager.
pub fn show_ok(tag: &str, xml: &str) -> bool {
    off_ui(|| post(tag, xml)).is_ok()
}

fn post(tag: &str, xml: &str) -> Result<()> {
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(tag)?;
    Ok(())
}
