// Fixture for the icons lint (tools/lint/voxa_lint/icons.py). The font in lint-fonts maps
// E81C, E721, E713, E946 and E88C only.
const I_HISTORY: char = '\u{E81C}'; // History
// Bad (icon-glyph): E88C is LanguageCht, a Chinese character, whatever the comment says.
const I_GIFT: char = '\u{E88C}'; // GiftboxOpen
// Bad (icon-glyph): the comment names a different glyph than the reviewed table.
const I_SEARCH: char = '\u{E721}'; // Magnifier
// Bad (icon-glyph): no glyph name.
const I_SETTINGS: char = '\u{E713}';
// Bad (icon-font): reviewed, but the font has no glyph there.
const I_GHOST: char = '\u{E123}'; // Ghost

fn bar() -> Div {
    // Bad (icon-glyph): a literal passed straight to icon().
    icon('\u{E946}', 9.)
}

#[cfg(test)]
mod tests {
    const REVIEWED_ICONS: &[(u32, &str)] = &[
        (0xE123, "Ghost"),
        (0xE713, "Settings"),
        (0xE721, "Search"),
        (0xE81C, "History"),
        (0xE88C, "GiftboxOpen"),
        (0xE946, "Info"),
    ];
    // Good: test code may name any code point.
    const X: char = '\u{E000}';
}
