//! The colors of Tailwind's default palette the demo uses (Tailwind CSS
//! v4.1, as the core's `tailwind/palette.rs`: the typed API has no palette
//! of its own, so this copies the shades).

use nsh_core::ui::{Color, rgba};

/// The shades of one Tailwind color: 50, 100, 200, ..., 900, 950.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Hue([u32; 11]);

/// The shades, in the order of a [`Hue`]'s values.
const SHADES: [u16; 11] = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950];

impl Hue {
    /// Shade `shade` (`500`); black for a shade Tailwind has not.
    pub(crate) fn shade(self, shade: u16) -> Color {
        SHADES
            .iter()
            .position(|&s| s == shade)
            .map_or(rgba(0x000000ff), |i| rgba(self.0[i]))
    }
}

/// Tailwind's `red`.
pub(crate) const RED: Hue = Hue([
    0xfef2f2ff, 0xffe2e2ff, 0xffc9c9ff, 0xffa2a2ff, 0xff6467ff, 0xfb2c36ff, 0xe7000bff, 0xc10007ff,
    0x9f0712ff, 0x82181aff, 0x460809ff,
]);

/// Tailwind's `orange`.
pub(crate) const ORANGE: Hue = Hue([
    0xfff7edff, 0xffedd4ff, 0xffd6a7ff, 0xffb86aff, 0xff8904ff, 0xff6900ff, 0xf54900ff, 0xca3500ff,
    0x9f2d00ff, 0x7e2a0cff, 0x441306ff,
]);

/// Tailwind's `amber`.
pub(crate) const AMBER: Hue = Hue([
    0xfffbebff, 0xfef3c6ff, 0xfee685ff, 0xffd230ff, 0xffb900ff, 0xfe9a00ff, 0xe17100ff, 0xbb4d00ff,
    0x973c00ff, 0x7b3306ff, 0x461901ff,
]);

/// Tailwind's `yellow`.
pub(crate) const YELLOW: Hue = Hue([
    0xfefce8ff, 0xfef9c2ff, 0xfff085ff, 0xffdf20ff, 0xfdc700ff, 0xf0b100ff, 0xd08700ff, 0xa65f00ff,
    0x894b00ff, 0x733e0aff, 0x432004ff,
]);

/// Tailwind's `green`.
pub(crate) const GREEN: Hue = Hue([
    0xf0fdf4ff, 0xdcfce7ff, 0xb9f8cfff, 0x7bf1a8ff, 0x05df72ff, 0x00c950ff, 0x00a63eff, 0x008236ff,
    0x016630ff, 0x0d542bff, 0x032e15ff,
]);

/// Tailwind's `emerald`.
pub(crate) const EMERALD: Hue = Hue([
    0xecfdf5ff, 0xd0fae5ff, 0xa4f4cfff, 0x5ee9b5ff, 0x00d492ff, 0x00bc7dff, 0x009966ff, 0x007a55ff,
    0x006045ff, 0x004f3bff, 0x002c22ff,
]);

/// Tailwind's `sky`.
pub(crate) const SKY: Hue = Hue([
    0xf0f9ffff, 0xdff2feff, 0xb8e6feff, 0x74d4ffff, 0x00bcffff, 0x00a6f4ff, 0x0084d1ff, 0x0069a8ff,
    0x00598aff, 0x024a70ff, 0x052f4aff,
]);

/// Tailwind's `blue`.
pub(crate) const BLUE: Hue = Hue([
    0xeff6ffff, 0xdbeafeff, 0xbedbffff, 0x8ec5ffff, 0x51a2ffff, 0x2b7fffff, 0x155dfcff, 0x1447e6ff,
    0x193cb8ff, 0x1c398eff, 0x162456ff,
]);

/// Tailwind's `indigo`.
pub(crate) const INDIGO: Hue = Hue([
    0xeef2ffff, 0xe0e7ffff, 0xc6d2ffff, 0xa3b3ffff, 0x7c86ffff, 0x615fffff, 0x4f39f6ff, 0x432dd7ff,
    0x372aacff, 0x312c85ff, 0x1e1a4dff,
]);

/// Tailwind's `violet`.
pub(crate) const VIOLET: Hue = Hue([
    0xf5f3ffff, 0xede9feff, 0xddd6ffff, 0xc4b4ffff, 0xa684ffff, 0x8e51ffff, 0x7f22feff, 0x7008e7ff,
    0x5d0ec0ff, 0x4d179aff, 0x2f0d68ff,
]);

/// Tailwind's `fuchsia`.
pub(crate) const FUCHSIA: Hue = Hue([
    0xfdf4ffff, 0xfae8ffff, 0xf6cfffff, 0xf4a8ffff, 0xed6affff, 0xe12afbff, 0xc800deff, 0xa800b7ff,
    0x8a0194ff, 0x721378ff, 0x4b004fff,
]);

/// Tailwind's `pink`.
pub(crate) const PINK: Hue = Hue([
    0xfdf2f8ff, 0xfce7f3ff, 0xfccee8ff, 0xfda5d5ff, 0xfb64b6ff, 0xf6339aff, 0xe60076ff, 0xc6005cff,
    0xa3004cff, 0x861043ff, 0x510424ff,
]);

/// Tailwind's `rose`.
pub(crate) const ROSE: Hue = Hue([
    0xfff1f2ff, 0xffe4e6ff, 0xffccd3ff, 0xffa1adff, 0xff637eff, 0xff2056ff, 0xec003fff, 0xc70036ff,
    0xa50036ff, 0x8b0836ff, 0x4d0218ff,
]);

/// Tailwind's `zinc`.
pub(crate) const ZINC: Hue = Hue([
    0xfafafaff, 0xf4f4f5ff, 0xe4e4e7ff, 0xd4d4d8ff, 0x9f9fa9ff, 0x71717bff, 0x52525cff, 0x3f3f46ff,
    0x27272aff, 0x18181bff, 0x09090bff,
]);
