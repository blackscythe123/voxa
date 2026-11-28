// Fixture for tools/lint (update-key-present): not compiled.
pub const K1_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000000"; // Bad (release): placeholder
pub const K2_HEX: &str = "abc123";                                                           // Bad: not 64 hex
pub const KEYS: [(u8, [u8; 32]); 2] = [(1, hex32(K1_HEX)), (2, hex32(K2_HEX))];
