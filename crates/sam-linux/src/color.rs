//! Colour values for the screen picker: plain sRGB, no dependencies.
//!
//! The picker samples one pixel. Everything after that is arithmetic, so the
//! result is deterministic and testable without a display.

/// An 8-bit sRGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    pub fn css_rgb(self) -> String {
        format!("rgb({}, {}, {})", self.r, self.g, self.b)
    }

    /// Hue in degrees, saturation and lightness in percent.
    pub fn hsl(self) -> (f64, f64, f64) {
        let r = f64::from(self.r) / 255.0;
        let g = f64::from(self.g) / 255.0;
        let b = f64::from(self.b) / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let lightness = (max + min) / 2.0;
        let delta = max - min;
        if delta == 0.0 {
            return (0.0, 0.0, lightness * 100.0);
        }
        let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
        let hue = if max == r {
            60.0 * (((g - b) / delta) % 6.0)
        } else if max == g {
            60.0 * (((b - r) / delta) + 2.0)
        } else {
            60.0 * (((r - g) / delta) + 4.0)
        };
        let hue = if hue < 0.0 { hue + 360.0 } else { hue };
        (hue, saturation * 100.0, lightness * 100.0)
    }

    /// The block the panel shows, ready to copy as text.
    pub fn describe(self) -> String {
        let (h, s, l) = self.hsl();
        format!(
            "Hex      {}\nRGB      {}\nHSL      {:.0}°, {:.0}%, {:.0}%\n",
            self.hex(),
            self.css_rgb(),
            h,
            s,
            l
        )
        .trim_end()
        .to_owned()
    }

    /// `#rrggbb` if the whole input is a colour, otherwise `None`.
    pub fn parse_hex(text: &str) -> Option<Rgb> {
        let hex = text.trim().strip_prefix('#').unwrap_or(text.trim());
        if hex.len() != 6 {
            return None;
        }
        let value = u32::from_str_radix(hex, 16).ok()?;
        Some(Rgb::new(
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_hex_and_rgb() {
        let red = Rgb::new(255, 0, 0);
        assert_eq!(red.hex(), "#ff0000");
        assert_eq!(red.css_rgb(), "rgb(255, 0, 0)");
        assert_eq!(red.hsl(), (0.0, 100.0, 50.0));
    }

    #[test]
    fn handles_grey_without_a_division_by_zero() {
        let grey = Rgb::new(128, 128, 128);
        let (h, s, l) = grey.hsl();
        assert_eq!(h, 0.0);
        assert_eq!(s, 0.0);
        assert!((l - 50.196).abs() < 0.01);
    }

    #[test]
    fn parses_six_digit_hex() {
        assert_eq!(Rgb::parse_hex("#1a2b3c"), Some(Rgb::new(0x1a, 0x2b, 0x3c)));
        assert_eq!(Rgb::parse_hex("1A2B3C"), Some(Rgb::new(0x1a, 0x2b, 0x3c)));
        assert_eq!(Rgb::parse_hex("#abc"), None);
        assert_eq!(Rgb::parse_hex("nope"), None);
    }
}
