//! Renders separator icons (vertical lines, dashes, dots…) and writes them as .ico files.

use serde::{Deserialize, Serialize};
use std::{io, path::Path};

/// Every size Windows might ask for across DPI settings, so the taskbar never has to
/// rescale (rescaling a 1–2 px line makes it blurry).
const ICO_SIZES: &[u32] = &[16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 96, 128, 256];

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Solid,
    Dashed,
    Dotted,
    Double,
    /// Transparent: just an empty gap in the taskbar.
    Spacer,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Style {
    pub kind: Kind,
    /// `#rrggbb`
    pub color: String,
    /// Line thickness in pixels at the 24 px taskbar icon size (scaled for other sizes).
    pub thickness: u8,
    /// Line length as a percentage of the icon height.
    pub length: u8,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            kind: Kind::Solid,
            color: "#8a8a8a".into(),
            thickness: 2,
            length: 75,
        }
    }
}

impl Style {
    /// Short stable hash used to give each style its own icon file name, so Windows'
    /// icon cache never shows a stale image.
    pub fn fingerprint(&self) -> String {
        // "v2": bump when the file format changes so stale files/cache entries are skipped.
        let s = format!(
            "v2|{:?}|{}|{}|{}",
            self.kind,
            self.color.to_lowercase(),
            self.thickness,
            self.length
        );
        let mut h: u64 = 0xcbf29ce484222325;
        for b in s.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        format!("{:08x}", h as u32)
    }
}

fn parse_color(s: &str) -> [u8; 3] {
    let s = s.trim().trim_start_matches('#');
    let v = u32::from_str_radix(s, 16).unwrap_or(0x8a8a8a);
    if s.len() == 3 {
        let (r, g, b) = ((v >> 8) & 0xf, (v >> 4) & 0xf, v & 0xf);
        [(r * 17) as u8, (g * 17) as u8, (b * 17) as u8]
    } else {
        [(v >> 16) as u8, (v >> 8) as u8, v as u8]
    }
}

/// Renders the style into a `size`×`size` RGBA buffer.
pub fn render(style: &Style, size: u32) -> Vec<u8> {
    let mut buf = vec![0u8; (size * size * 4) as usize];
    let rgb = parse_color(&style.color);
    let n = size as i32;
    let scale = size as f32 / 24.0;
    let px = |v: f32| (v * scale).round().max(1.0) as i32;

    let t = px(style.thickness.clamp(1, 12) as f32);
    let len = ((n as f32) * style.length.clamp(10, 100) as f32 / 100.0).round() as i32;
    let len = len.clamp(t, n);
    let y0 = (n - len) / 2;
    let y1 = y0 + len;

    let mut rect = |x: i32, y: i32, w: i32, h: i32| {
        for yy in y.max(0)..(y + h).min(n) {
            for xx in x.max(0)..(x + w).min(n) {
                let i = ((yy * n + xx) * 4) as usize;
                buf[i..i + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
    };

    match style.kind {
        Kind::Spacer => {}
        Kind::Solid => rect((n - t) / 2, y0, t, len),
        Kind::Dashed => {
            let dash = px(4.0).max(t + 1);
            let gap = px(2.5);
            let mut y = y0;
            while y < y1 {
                rect((n - t) / 2, y, t, dash.min(y1 - y));
                y += dash + gap;
            }
        }
        Kind::Dotted => {
            let d = t.max(px(1.5));
            let gap = d.max(px(2.0));
            let count = ((len + gap) / (d + gap)).max(1);
            // Centre the run of dots vertically.
            let used = count * d + (count - 1) * gap;
            let mut y = (n - used) / 2;
            for _ in 0..count {
                rect((n - d) / 2, y, d, d);
                y += d + gap;
            }
        }
        Kind::Double => {
            let gap = t.max(px(2.0));
            let x = (n - (2 * t + gap)) / 2;
            rect(x, y0, t, len);
            rect(x + t + gap, y0, t, len);
        }
    }
    buf
}

/// Writes an .ico whose entries are all 32-bit PNGs.
///
/// (`ico::IconDirEntry::encode` turns single-colour images into 1-bit BMPs, which the
/// taskbar refuses to show for pinned/relaunch icons — you get a blank page instead.)
pub fn write_ico(style: &Style, path: &Path) -> io::Result<()> {
    let pngs = ICO_SIZES
        .iter()
        .map(|&s| {
            let mut png = Vec::new();
            ico::IconImage::from_rgba_data(s, s, render(style, s)).write_png(&mut png)?;
            Ok((s, png))
        })
        .collect::<io::Result<Vec<_>>>()?;

    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(pngs.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * pngs.len() as u32;
    for (size, png) in &pngs {
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in &pngs {
        out.extend_from_slice(png);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, out)
}

/// PNG data URL for previews in the manager UI.
pub fn preview_data_url(style: &Style, size: u32) -> String {
    let img = ico::IconImage::from_rgba_data(size, size, render(style, size));
    let mut png = Vec::new();
    let _ = img.write_png(&mut png);
    format!("data:image/png;base64,{}", base64(&png))
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opaque_columns(buf: &[u8], size: u32) -> Vec<u32> {
        (0..size)
            .filter(|&x| (0..size).any(|y| buf[((y * size + x) * 4 + 3) as usize] == 255))
            .collect()
    }

    #[test]
    fn solid_line_is_centred_and_scaled() {
        let s = Style::default();
        assert_eq!(opaque_columns(&render(&s, 24), 24), vec![11, 12]);
        assert_eq!(opaque_columns(&render(&s, 48), 48), vec![22, 23, 24, 25]);
    }

    #[test]
    fn double_has_two_lines_and_spacer_is_empty() {
        let s = Style {
            kind: Kind::Double,
            ..Style::default()
        };
        assert_eq!(opaque_columns(&render(&s, 24), 24).len(), 4);
        let s = Style {
            kind: Kind::Spacer,
            ..Style::default()
        };
        assert!(render(&s, 24).iter().all(|&b| b == 0));
    }

    #[test]
    fn colors_and_base64() {
        assert_eq!(parse_color("#ff8000"), [255, 128, 0]);
        assert_eq!(parse_color("fff"), [255, 255, 255]);
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
    }

    #[test]
    fn writes_a_readable_ico() {
        let p = std::env::temp_dir().join("taskbar-separators-test.ico");
        write_ico(&Style::default(), &p).unwrap();
        let dir = ico::IconDir::read(std::fs::File::open(&p).unwrap()).unwrap();
        assert_eq!(dir.entries().len(), ICO_SIZES.len());
        for e in dir.entries() {
            assert!(e.is_png(), "every entry must be a full-colour PNG");
            assert_eq!(e.bits_per_pixel(), 32);
            assert_eq!(e.decode().unwrap().width(), e.width());
        }
        let _ = std::fs::remove_file(p);
    }
}
