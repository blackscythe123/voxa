// Fixture for tools/lint: not compiled.

fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Library") // Bad: isolation-env
}
