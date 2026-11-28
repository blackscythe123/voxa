// Fixture for tools/lint (contrast): not compiled.

val LightVx = VxColors(
    base = Color(0xFFFFFFFF),
    surface = Color(0xFFF3F3F1),
    card = Color(0xFFFFFFFF),
    text = Color(0xFF141414),
    accent = Color(0xFF7A3F22),
    onAccent = Color(0xFFFFFFFF),
    good = Color(0xFF1F9D55),
    warn = Color(0xFFB26B00),
    danger = Color(0xFFC8281E),
    goodText = Color(0xFF17743F),
    warnText = Color(0xFF9A5B00),
    dangerText = Color(0xFFB3241B),
    // Bad: 4.5:1 secondary text, AAA needs 7:1 (on base, card and surface).
    muted = Color(0xFF767676),
    switchOff = Color(0xFF939393),
    dark = false,
)

val DarkVx = VxColors(
    base = Color(0xFF121212),
    surface = Color(0xFF232321),
    card = Color(0xFF1C1C1B),
    text = Color(0xFFF2F2F0),
    accent = Color(0xFFD9946D),
    onAccent = Color(0xFF24140C),
    good = Color(0xFF45C27C),
    warn = Color(0xFFE3A44A),
    danger = Color(0xFFF07A70),
    goodText = Color(0xFF45C27C),
    warnText = Color(0xFFE3A44A),
    // Bad: the light danger red on near-black (base, card, surface).
    dangerText = Color(0xFFB3241B),
    muted = Color(0xFFB8B8B6),
    switchOff = Color(0xFF939393),
    dark = true,
)
