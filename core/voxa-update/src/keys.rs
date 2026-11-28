//! The public keys an update feed may be signed with. Public keys are safe to commit.
//!
//! K1 is the active key (its private half is the "release" environment secret VOXA_UPDATE_KEY).
//! K2 is the offline backup (never in CI). docs/update-keys.md has the owner commands that make
//! them and the rotation plan.
//!
//! PLACEHOLDERS: until the owner pastes the real keys, both are all zeros. An all-zero key never
//! verifies anything, the build prints a warning, and a release build (VOXA_RELEASE_BUILD=1)
//! fails in build.rs. The lint `update-key-present` reports them too.

/// K1, active (64 hex characters).
pub const K1_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000000";
/// K2, offline backup (64 hex characters).
pub const K2_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// `(key_id, Ed25519 public key)`. The envelope's `key_id` picks one.
pub const KEYS: [(u8, [u8; 32]); 2] = [(1, hex32(K1_HEX)), (2, hex32(K2_HEX))];

/// True while any compiled key is still the all-zero placeholder.
pub fn placeholders() -> bool {
    KEYS.iter().any(|(_, k)| k.iter().all(|b| *b == 0))
}

const fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("update key: not a hex digit"),
    }
}

/// 64 hex characters to 32 bytes, at compile time (a bad constant fails the build).
pub const fn hex32(s: &str) -> [u8; 32] {
    let b = s.as_bytes();
    if b.len() != 64 {
        panic!("update key: must be 64 hex characters");
    }
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = nibble(b[2 * i]) << 4 | nibble(b[2 * i + 1]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_compiled_key_is_32_bytes_and_ids_are_unique() {
        assert_eq!(KEYS.len(), 2, "K1 active and K2 backup");
        for (_, k) in KEYS {
            assert_eq!(k.len(), 32);
        }
        assert_ne!(KEYS[0].0, KEYS[1].0, "key ids are unique");
        assert_eq!(K1_HEX.len(), 64);
        assert_eq!(K2_HEX.len(), 64);
        if !placeholders() {
            assert_ne!(KEYS[0].1, KEYS[1].1, "K1 and K2 are different keys");
        }
    }
}
