// Fixture for tools/lint (update-hosts): not compiled.
const FEED: &str = "https://github.com/blackscythe123/voxa/releases/download/update-feed/desktop.json"; // Good
const CDN: &str = "https://objects.githubusercontent.com/x";                                         // Good
const PLAIN: &str = "http://github.com/blackscythe123/voxa/x";                                      // Bad: plain http
const OTHER: &str = "https://updates.example.com/feed.json";                                         // Bad: not an allowed host

fn client() {
    let c = reqwest::blocking::Client::builder().danger_accept_invalid_certs(true);                // Bad: certificate checks off
}

#[cfg(test)]
mod tests {
    const LOCAL: &str = "http://127.0.0.1:9/feed.json";                                             // Good: tests only
}
