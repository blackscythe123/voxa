// Fixture for tools/lint: not compiled.

pub fn parse(body: &str) -> u32 {
    body.trim().parse().unwrap()                            // Bad: robust-unwrap
}

pub fn load(path: &Path) -> Vec<u8> {
    std::fs::read(path).expect("read")                      // Bad: robust-unwrap
}

pub fn fine(path: &Path) -> Vec<u8> {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner()); // Good: no panic
    std::fs::read(path).unwrap_or_default()                 // Good
}

pub fn save(tmp: &Path, path: &Path) -> std::io::Result<()> {
    std::fs::rename(tmp, path)                              // Bad: robust-rename
}

pub fn save_safely(tmp: &Path, path: &Path) -> std::io::Result<()> {
    voxa_core::fsx::replace_file(tmp, path)                 // Good
}

#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        super::parse("1").checked_add(1).unwrap();          // Good: tests may unwrap
    }
}
