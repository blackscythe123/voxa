// Fixture for tools/lint: not compiled.

pub fn ready(count: u32, app: Option<&str>) -> Notice {     // Good: an app name and a count
    Notice { title: format!("{count} dictations are ready"), body: String::new(), action: None }
}

pub fn ready_with_preview(count: u32, text: &str) -> Notice { // Bad: privacy-notify
    Notice { title: format!("Ready: {text}"), body: String::new(), action: None }
}
