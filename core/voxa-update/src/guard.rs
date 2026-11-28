// Shared by build.rs (include!) and the unit tests: when a build must refuse to compile.

/// A release build (VOXA_RELEASE_BUILD=1: CI tags, android/release.ps1, any Android release
/// build) refuses placeholder update keys: a copy built that way could never update itself.
/// Only an explicit VOXA_ALLOW_PLACEHOLDER_KEYS=1 (a local test build, never handed out) lets it
/// through, and that build then says "Updates are off in this build".
pub fn release_refused(placeholder_keys: bool, release_build: bool, allow_placeholder: bool) -> bool {
    placeholder_keys && release_build && !allow_placeholder
}

/// The env flag is set to exactly "1".
pub fn flag_on(value: Option<&str>) -> bool {
    value == Some("1")
}
