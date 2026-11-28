//! A release build refuses to compile while the update keys are still placeholders.
//!
//! VOXA_RELEASE_BUILD=1 marks a release: CI sets it for tagged releases (desktop.yml), and the
//! Android build sets it for every release build (assembleRelease and friends). Only
//! VOXA_ALLOW_PLACEHOLDER_KEYS=1 lets a release build through with placeholders (a local test
//! build; it shows "Updates are off in this build"). Everyday debug builds and tests only warn.
//! The desktop installer has its own check in desktop/windows/voxa.iss.

include!("src/guard.rs");

fn main() {
    println!("cargo:rerun-if-changed=src/keys.rs");
    println!("cargo:rerun-if-changed=src/guard.rs");
    println!("cargo:rerun-if-env-changed=VOXA_RELEASE_BUILD");
    println!("cargo:rerun-if-env-changed=VOXA_ALLOW_PLACEHOLDER_KEYS");
    let keys = std::fs::read_to_string("src/keys.rs").unwrap_or_default();
    let placeholder = keys.contains(&"0".repeat(64));
    let release = flag_on(std::env::var("VOXA_RELEASE_BUILD").ok().as_deref());
    let allow = flag_on(std::env::var("VOXA_ALLOW_PLACEHOLDER_KEYS").ok().as_deref());
    if release_refused(placeholder, release, allow) {
        panic!(
            "voxa-update: the update public keys in core/voxa-update/src/keys.rs are still placeholders. \
             A release build needs the real K1 and K2 public keys; see docs/update-keys.md. \
             (VOXA_ALLOW_PLACEHOLDER_KEYS=1 builds a local test copy that can never update.)"
        );
    }
    if placeholder {
        println!("cargo:warning=voxa-update: placeholder update keys; this build can never install an update (docs/update-keys.md)");
    }
}
