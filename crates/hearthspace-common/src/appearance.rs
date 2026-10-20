//! Persisted appearance choices shared by the desktop and its Linux adapters.
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Appearance {
    pub color_mode: ColorMode,
    pub accent_color: AccentColor,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ColorMode {
    #[default]
    Light,
    Dark,
}

impl ColorMode {
    pub const ALL: [Self; 2] = [Self::Light, Self::Dark];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// org.freedesktop.appearance color-scheme values.
    pub const fn portal_value(self) -> u32 {
        match self {
            Self::Dark => 1,
            Self::Light => 2,
        }
    }
}

impl fmt::Display for ColorMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ColorMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|mode| mode.as_str() == value)
            .ok_or_else(|| format!("invalid color mode: {value:?}"))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum AccentColor {
    #[default]
    Orange,
    Rose,
    Violet,
    Blue,
    Teal,
    Olive,
}

impl AccentColor {
    pub const ALL: [Self; 6] = [
        Self::Orange,
        Self::Rose,
        Self::Violet,
        Self::Blue,
        Self::Teal,
        Self::Olive,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Orange => "orange",
            Self::Rose => "rose",
            Self::Violet => "violet",
            Self::Blue => "blue",
            Self::Teal => "teal",
            Self::Olive => "olive",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Orange => "Orange",
            Self::Rose => "Rose",
            Self::Violet => "Violet",
            Self::Blue => "Blue",
            Self::Teal => "Teal",
            Self::Olive => "Olive",
        }
    }

    /// Base sRGB colors from the desktop reference, independent of light/dark.
    pub const fn rgb(self) -> [u8; 3] {
        match self {
            Self::Orange => [0xe9, 0x64, 0x2a],
            Self::Rose => [0xb7, 0x4e, 0x69],
            Self::Violet => [0x79, 0x57, 0xb5],
            Self::Blue => [0x36, 0x72, 0xb5],
            Self::Teal => [0x23, 0x7e, 0x78],
            Self::Olive => [0x6f, 0x7b, 0x38],
        }
    }

    pub fn srgb(self) -> [f64; 3] {
        self.rgb().map(|channel| f64::from(channel) / 255.0)
    }
}

impl fmt::Display for AccentColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AccentColor {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|accent| accent.as_str() == value)
            .ok_or_else(|| format!("invalid accent color: {value:?}"))
    }
}
