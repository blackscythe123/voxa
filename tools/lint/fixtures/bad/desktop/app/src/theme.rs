// Fixture for tools/lint (contrast): not compiled. Each palette breaks a rule on purpose.

pub const LIGHT: Palette = Palette {
    dark: false,
    text: 0x1A1A1A,
    // Bad: secondary text at 4.6:1 on the page, AAA needs 7:1 (muted/page, muted/card).
    muted: 0x767676,
    body: 0x3D3D3D,
    page: 0xFFFFFF,
    nav: 0xF1EEEA,
    card: 0xFFFFFF,
    // Bad: not the brand sepia #7A3F22 in light mode.
    accent: 0x8A4F32,
    on_accent: 0xFFFFFF,
};

pub const DARK: Palette = Palette {
    dark: true,
    text: 0xF2F0ED,
    muted: 0xC2BCB5,
    body: 0xDAD5CF,
    page: 0x242120,
    nav: 0x1C1A18,
    card: 0x2D2A28,
    // Bad: the light sepia on dark surfaces (accent/page, accent/card, accent/nav).
    accent: 0x7A3F22,
    on_accent: 0xFFFFFF,
};

pub const INK: u32 = 0x7A3F22;

fn badges() {
    // Good: white on the brand sepia is 8.2:1.
    let mark = div().bg(rgb(INK)).text_color(rgb(0xFFFFFF)).child("v");
    // Bad: white on a light green is under 4.5:1.
    let chip = div().bg(rgb(0x25D366)).text_color(rgb(0xFFFFFF)).child("W");
}
