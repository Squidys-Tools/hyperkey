//! Light and dark design tokens, chosen from the window appearance.
use gpui::{Hsla, Pixels, WindowAppearance, px, rgb};

/// Spacing scale, in logical pixels.
pub mod space {
    use super::{Pixels, px};
    pub const XS: Pixels = px(4.0);
    pub const SM: Pixels = px(8.0);
    pub const MD: Pixels = px(12.0);
    pub const LG: Pixels = px(16.0);
    pub const XL: Pixels = px(24.0);
}

pub const CARD_RADIUS: Pixels = px(8.0);
pub const CONTROL_RADIUS: Pixels = px(6.0);
pub const FONT: &str = "Segoe UI";

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Theme {
    pub canvas: Hsla,
    pub card: Hsla,
    pub control: Hsla,
    pub control_hover: Hsla,
    pub control_active: Hsla,
    pub hairline: Hsla,
    pub text: Hsla,
    pub secondary: Hsla,
    pub accent: Hsla,
    pub accent_hover: Hsla,
    pub accent_active: Hsla,
    pub on_accent: Hsla,
    pub danger: Hsla,
    pub success: Hsla,
    pub warning: Hsla,
}

impl Theme {
    pub fn for_appearance(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::dark(),
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::light(),
        }
    }

    fn dark() -> Self {
        Self {
            canvas: hex(0x1B1C20),
            card: hex(0x24252A),
            control: hex(0x303136),
            control_hover: hex(0x3A3B40),
            control_active: hex(0x45464C),
            hairline: hex(0x3A3B40),
            text: hex(0xF2F2F4),
            secondary: hex(0xA5A6AC),
            accent: hex(0x4C8DF6),
            accent_hover: hex(0x6199F7),
            accent_active: hex(0x3B74D8),
            on_accent: hex(0xFFFFFF),
            danger: hex(0xE0706B),
            success: hex(0x4CC38A),
            warning: hex(0xE5B454),
        }
    }

    fn light() -> Self {
        Self {
            canvas: hex(0xF3F3F3),
            card: hex(0xFFFFFF),
            control: hex(0xF0F0F2),
            control_hover: hex(0xE6E6E9),
            control_active: hex(0xDADADE),
            hairline: hex(0xE5E5E5),
            text: hex(0x1B1C20),
            secondary: hex(0x5F6066),
            accent: hex(0x3B74D8),
            accent_hover: hex(0x3267C3),
            accent_active: hex(0x2B59AA),
            on_accent: hex(0xFFFFFF),
            danger: hex(0xC0443F),
            success: hex(0x2E9A66),
            warning: hex(0xA8701A),
        }
    }
}

fn hex(value: u32) -> Hsla {
    rgb(value).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appearance_selects_the_matching_palette() {
        let dark = Theme::for_appearance(WindowAppearance::Dark);
        let light = Theme::for_appearance(WindowAppearance::Light);
        assert_eq!(dark, Theme::for_appearance(WindowAppearance::VibrantDark));
        assert_eq!(light, Theme::for_appearance(WindowAppearance::VibrantLight));
        assert!(dark.canvas.l < light.canvas.l);
        assert_eq!(dark.accent, hex(0x4C8DF6));
        assert_eq!(light.accent, hex(0x3B74D8));
    }
}
