//! A release build refuses to compile while the update keys are still placeholders.
//!
//! CI sets VOXA_RELEASE_BUILD=1 for tagged releases (desktop.yml) and the Android release build
//! sets it when built with -Pvoxa.release=true. Everyday builds and tests only warn.

fn main() {
    println!("cargo:rerun-if-changed=src/keys.rs");
    println!("cargo:rerun-if-env-changed=VOXA_RELEASE_BUILD");
    let keys = std::fs::read_to_string("src/keys.rs").unwrap_or_default();
    let placeholder = keys.contains(&"0".repeat(64));
    let release = std::env::var("VOXA_RELEASE_BUILD").map(|v| v == "1").unwrap_or(false);
    if placeholder && release {
        panic!(
            "voxa-update: the update public keys in core/voxa-update/src/keys.rs are still placeholders. \
             A release build needs the real K1 and K2 public keys; see docs/update-keys.md."
        );
    }
    if placeholder {
        println!("cargo:warning=voxa-update: placeholder update keys; this build can never install an update (docs/update-keys.md)");
    }
}
