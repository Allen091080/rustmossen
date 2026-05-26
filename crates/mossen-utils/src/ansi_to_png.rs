// ANSI text rendering to PNG (with font rasterization, CRC32, PNG encoding).

use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;

const GLYPH_W: usize = 24;
const GLYPH_H: usize = 48;
const GLYPH_BYTES: usize = GLYPH_W * GLYPH_H;

#[derive(Debug, Clone, Copy)]
pub struct AnsiColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

const DEFAULT_BG: AnsiColor = AnsiColor {
    r: 40,
    g: 42,
    b: 54,
};

#[derive(Debug, Clone)]
pub struct AnsiSpan {
    pub text: String,
    pub color: AnsiColor,
    pub bold: bool,
}

pub type ParsedLine = Vec<AnsiSpan>;

#[derive(Debug, Clone)]
pub struct AnsiToPngOptions {
    /// Integer zoom factor (nearest-neighbor). Default 1.
    pub scale: usize,
    /// Horizontal padding in 1x pixels. Default 48.
    pub padding_x: usize,
    /// Vertical padding in 1x pixels. Default 48.
    pub padding_y: usize,
    /// Corner radius in 1x pixels. Default 16.
    pub border_radius: usize,
    /// Background color.
    pub background: AnsiColor,
}

impl Default for AnsiToPngOptions {
    fn default() -> Self {
        Self {
            scale: 1,
            padding_x: 48,
            padding_y: 48,
            border_radius: 16,
            background: DEFAULT_BG,
        }
    }
}

/// Render ANSI-escaped text directly to a PNG buffer.
pub fn ansi_to_png(ansi_text: &str, options: AnsiToPngOptions) -> Vec<u8> {
    let scale = options.scale.max(1);
    let padding_x = options.padding_x;
    let padding_y = options.padding_y;
    let border_radius = options.border_radius;
    let background = options.background;

    let mut lines = parse_ansi(ansi_text);

    // Trim trailing blank lines
    while !lines.is_empty()
        && lines
            .last()
            .unwrap()
            .iter()
            .all(|span| span.text.trim().is_empty())
    {
        lines.pop();
    }
    if lines.is_empty() {
        lines.push(vec![AnsiSpan {
            text: String::new(),
            color: background,
            bold: false,
        }]);
    }

    let cols = lines.iter().map(line_width_cells).max().unwrap_or(1).max(1);
    let rows = lines.len();

    let width = (cols * GLYPH_W + padding_x * 2) * scale;
    let height = (rows * GLYPH_H + padding_y * 2) * scale;

    // RGBA buffer pre-filled with background
    let mut px = vec![0u8; width * height * 4];
    fill_background(&mut px, &background);
    if border_radius > 0 {
        round_corners(&mut px, width, height, border_radius * scale);
    }

    // Blit glyphs (using fallback glyph since we don't embed font data in Rust)
    let pad_x = padding_x * scale;
    let pad_y = padding_y * scale;
    let fallback_glyph = make_fallback_glyph();

    for (row, line) in lines.iter().enumerate() {
        let mut col = 0;
        for span in line {
            for ch in span.text.chars() {
                let cell_w = unicode_width(ch);
                if cell_w == 0 {
                    continue;
                }
                let x = pad_x + col * GLYPH_W * scale;
                let y = pad_y + row * GLYPH_H * scale;
                let cp = ch as u32;
                if let Some(&alpha) = SHADE_ALPHA.get(&cp) {
                    blit_shade(&mut px, width, x, y, &span.color, &background, alpha, scale);
                } else {
                    blit_glyph(
                        &mut px,
                        width,
                        x,
                        y,
                        &fallback_glyph,
                        &span.color,
                        span.bold,
                        scale,
                    );
                }
                col += cell_w;
            }
        }
    }

    encode_png(&px, width, height)
}

fn line_width_cells(line: &ParsedLine) -> usize {
    line.iter()
        .map(|span| span.text.chars().map(unicode_width).sum::<usize>())
        .sum()
}

fn unicode_width(ch: char) -> usize {
    if ch.is_control() {
        0
    } else if is_wide_char(ch) {
        2
    } else {
        1
    }
}

fn is_wide_char(ch: char) -> bool {
    let cp = ch as u32;
    // CJK Unified Ideographs and other wide ranges
    (0x1100..=0x115F).contains(&cp)
        || (0x2E80..=0x303E).contains(&cp)
        || (0x3040..=0x33BF).contains(&cp)
        || (0x3400..=0x4DBF).contains(&cp)
        || (0x4E00..=0x9FFF).contains(&cp)
        || (0xA000..=0xA4CF).contains(&cp)
        || (0xAC00..=0xD7AF).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0xFE30..=0xFE6F).contains(&cp)
        || (0xFF01..=0xFF60).contains(&cp)
        || (0xFFE0..=0xFFE6).contains(&cp)
        || (0x20000..=0x2FFFD).contains(&cp)
        || (0x30000..=0x3FFFD).contains(&cp)
}

fn make_fallback_glyph() -> Vec<u8> {
    let mut glyph = vec![0u8; GLYPH_BYTES];
    // Simple box shape for fallback
    for y in 4..GLYPH_H - 4 {
        for x in 2..GLYPH_W - 2 {
            if y == 4 || y == GLYPH_H - 5 || x == 2 || x == GLYPH_W - 3 {
                glyph[y * GLYPH_W + x] = 180;
            }
        }
    }
    glyph
}

fn fill_background(px: &mut [u8], bg: &AnsiColor) {
    for i in (0..px.len()).step_by(4) {
        px[i] = bg.r;
        px[i + 1] = bg.g;
        px[i + 2] = bg.b;
        px[i + 3] = 255;
    }
}

use once_cell::sync::Lazy;
use std::collections::HashMap as LazyHashMap;

static SHADE_ALPHA: Lazy<LazyHashMap<u32, f64>> = Lazy::new(|| {
    let mut m = LazyHashMap::new();
    m.insert(0x2591, 0.25); // ░
    m.insert(0x2592, 0.5); // ▒
    m.insert(0x2593, 0.75); // ▓
    m.insert(0x2588, 1.0); // █
    m
});

fn blit_shade(
    px: &mut [u8],
    width: usize,
    x: usize,
    y: usize,
    fg: &AnsiColor,
    bg: &AnsiColor,
    alpha: f64,
    scale: usize,
) {
    let r = (fg.r as f64 * alpha + bg.r as f64 * (1.0 - alpha)).round() as u8;
    let g = (fg.g as f64 * alpha + bg.g as f64 * (1.0 - alpha)).round() as u8;
    let b = (fg.b as f64 * alpha + bg.b as f64 * (1.0 - alpha)).round() as u8;
    let cell_w = GLYPH_W * scale;
    let cell_h = GLYPH_H * scale;
    for dy in 0..cell_h {
        let row_base = ((y + dy) * width + x) * 4;
        for dx in 0..cell_w {
            let i = row_base + dx * 4;
            if i + 2 < px.len() {
                px[i] = r;
                px[i + 1] = g;
                px[i + 2] = b;
            }
        }
    }
}

fn blit_glyph(
    px: &mut [u8],
    width: usize,
    x: usize,
    y: usize,
    glyph: &[u8],
    color: &AnsiColor,
    bold: bool,
    scale: usize,
) {
    for gy in 0..GLYPH_H {
        for gx in 0..GLYPH_W {
            let mut a = glyph[gy * GLYPH_W + gx] as u32;
            if a == 0 {
                continue;
            }
            if bold {
                a = (a as f64 * 1.4).min(255.0) as u32;
            }
            let inv = 255 - a;
            for sy in 0..scale {
                let row_base = ((y + gy * scale + sy) * width + x + gx * scale) * 4;
                for sx in 0..scale {
                    let i = row_base + sx * 4;
                    if i + 2 < px.len() {
                        px[i] = ((color.r as u32 * a + px[i] as u32 * inv) >> 8) as u8;
                        px[i + 1] = ((color.g as u32 * a + px[i + 1] as u32 * inv) >> 8) as u8;
                        px[i + 2] = ((color.b as u32 * a + px[i + 2] as u32 * inv) >> 8) as u8;
                    }
                }
            }
        }
    }
}

fn round_corners(px: &mut [u8], width: usize, height: usize, r: usize) {
    let r2 = (r * r) as f64;
    for dy in 0..r {
        for dx in 0..r {
            let ox = r as f64 - dx as f64 - 0.5;
            let oy = r as f64 - dy as f64 - 0.5;
            if ox * ox + oy * oy <= r2 {
                continue;
            }
            // Top-left
            px[(dy * width + dx) * 4 + 3] = 0;
            // Top-right
            px[(dy * width + (width - 1 - dx)) * 4 + 3] = 0;
            // Bottom-left
            px[((height - 1 - dy) * width + dx) * 4 + 3] = 0;
            // Bottom-right
            px[((height - 1 - dy) * width + (width - 1 - dx)) * 4 + 3] = 0;
        }
    }
}

// --- PNG encoding ---

const PNG_SIG: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

fn make_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    for n in 0..256 {
        let mut c = n as u32;
        for _ in 0..8 {
            if c & 1 != 0 {
                c = 0xEDB88320 ^ (c >> 1);
            } else {
                c >>= 1;
            }
        }
        table[n] = c;
    }
    table
}

fn crc32(data: &[u8]) -> u32 {
    let table = make_crc_table();
    let mut c = 0xFFFFFFFF_u32;
    for &byte in data {
        c = table[((c ^ byte as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFFFFFF
}

fn png_chunk(chunk_type: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(4 + data.len());
    body.extend_from_slice(chunk_type);
    body.extend_from_slice(data);

    let mut out = Vec::with_capacity(12 + data.len());
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
    out
}

fn encode_png(px: &[u8], width: usize, height: usize) -> Vec<u8> {
    // IHDR
    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&(width as u32).to_be_bytes());
    ihdr[4..8].copy_from_slice(&(height as u32).to_be_bytes());
    ihdr[8] = 8; // bit depth
    ihdr[9] = 6; // color type: RGBA
    ihdr[10] = 0; // compression
    ihdr[11] = 0; // filter
    ihdr[12] = 0; // interlace

    // IDAT: each scanline prefixed with filter byte 0
    let stride = width * 4;
    let mut raw = Vec::with_capacity(height * (stride + 1));
    for y in 0..height {
        raw.push(0); // filter byte
        raw.extend_from_slice(&px[y * stride..(y + 1) * stride]);
    }

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&raw).unwrap_or_default();
    let idat = encoder.finish().unwrap_or_default();

    let mut result = Vec::new();
    result.extend_from_slice(&PNG_SIG);
    result.extend(png_chunk(b"IHDR", &ihdr));
    result.extend(png_chunk(b"IDAT", &idat));
    result.extend(png_chunk(b"IEND", &[]));
    result
}

/// Parse ANSI text into structured lines.
/// This is a simplified parser for basic ANSI escape sequences.
pub fn parse_ansi(text: &str) -> Vec<ParsedLine> {
    let mut lines: Vec<ParsedLine> = Vec::new();
    let mut current_line: ParsedLine = Vec::new();
    let mut current_text = String::new();
    let mut current_color = AnsiColor {
        r: 204,
        g: 204,
        b: 204,
    }; // default fg
    let mut current_bold = false;

    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\x1B' && i + 1 < chars.len() && chars[i + 1] == '[' {
            // Flush current text
            if !current_text.is_empty() {
                current_line.push(AnsiSpan {
                    text: current_text.clone(),
                    color: current_color,
                    bold: current_bold,
                });
                current_text.clear();
            }

            // Parse CSI sequence
            i += 2;
            let mut params = String::new();
            while i < chars.len()
                && chars[i] != 'm'
                && chars[i].is_ascii()
                && !chars[i].is_alphabetic()
            {
                params.push(chars[i]);
                i += 1;
            }
            if i < chars.len() && chars[i] == 'm' {
                // Process SGR parameters
                let codes: Vec<u32> = params.split(';').filter_map(|s| s.parse().ok()).collect();
                process_sgr(&codes, &mut current_color, &mut current_bold);
                i += 1;
            } else if i < chars.len() {
                i += 1; // skip unknown terminator
            }
        } else if chars[i] == '\n' {
            if !current_text.is_empty() {
                current_line.push(AnsiSpan {
                    text: current_text.clone(),
                    color: current_color,
                    bold: current_bold,
                });
                current_text.clear();
            }
            lines.push(current_line);
            current_line = Vec::new();
            i += 1;
        } else {
            current_text.push(chars[i]);
            i += 1;
        }
    }

    // Flush remaining
    if !current_text.is_empty() {
        current_line.push(AnsiSpan {
            text: current_text,
            color: current_color,
            bold: current_bold,
        });
    }
    lines.push(current_line);
    lines
}

fn process_sgr(codes: &[u32], color: &mut AnsiColor, bold: &mut bool) {
    if codes.is_empty() || codes == [0] {
        *color = AnsiColor {
            r: 204,
            g: 204,
            b: 204,
        };
        *bold = false;
        return;
    }

    let mut i = 0;
    while i < codes.len() {
        match codes[i] {
            0 => {
                *color = AnsiColor {
                    r: 204,
                    g: 204,
                    b: 204,
                };
                *bold = false;
            }
            1 => *bold = true,
            22 => *bold = false,
            30 => *color = AnsiColor { r: 0, g: 0, b: 0 },
            31 => {
                *color = AnsiColor {
                    r: 205,
                    g: 49,
                    b: 49,
                }
            }
            32 => {
                *color = AnsiColor {
                    r: 13,
                    g: 188,
                    b: 121,
                }
            }
            33 => {
                *color = AnsiColor {
                    r: 229,
                    g: 229,
                    b: 16,
                }
            }
            34 => {
                *color = AnsiColor {
                    r: 36,
                    g: 114,
                    b: 200,
                }
            }
            35 => {
                *color = AnsiColor {
                    r: 188,
                    g: 63,
                    b: 188,
                }
            }
            36 => {
                *color = AnsiColor {
                    r: 17,
                    g: 168,
                    b: 205,
                }
            }
            37 => {
                *color = AnsiColor {
                    r: 204,
                    g: 204,
                    b: 204,
                }
            }
            39 => {
                *color = AnsiColor {
                    r: 204,
                    g: 204,
                    b: 204,
                }
            }
            90 => {
                *color = AnsiColor {
                    r: 118,
                    g: 118,
                    b: 118,
                }
            }
            91 => {
                *color = AnsiColor {
                    r: 241,
                    g: 76,
                    b: 76,
                }
            }
            92 => {
                *color = AnsiColor {
                    r: 35,
                    g: 209,
                    b: 139,
                }
            }
            93 => {
                *color = AnsiColor {
                    r: 245,
                    g: 245,
                    b: 67,
                }
            }
            94 => {
                *color = AnsiColor {
                    r: 59,
                    g: 142,
                    b: 234,
                }
            }
            95 => {
                *color = AnsiColor {
                    r: 214,
                    g: 112,
                    b: 214,
                }
            }
            96 => {
                *color = AnsiColor {
                    r: 41,
                    g: 184,
                    b: 219,
                }
            }
            97 => {
                *color = AnsiColor {
                    r: 229,
                    g: 229,
                    b: 229,
                }
            }
            38 => {
                // Extended foreground color
                if i + 1 < codes.len() && codes[i + 1] == 5 && i + 2 < codes.len() {
                    let idx = codes[i + 2] as usize;
                    *color = ansi_256_color(idx);
                    i += 2;
                } else if i + 1 < codes.len() && codes[i + 1] == 2 && i + 4 < codes.len() {
                    *color = AnsiColor {
                        r: codes[i + 2] as u8,
                        g: codes[i + 3] as u8,
                        b: codes[i + 4] as u8,
                    };
                    i += 4;
                }
            }
            _ => {}
        }
        i += 1;
    }
}

fn ansi_256_color(idx: usize) -> AnsiColor {
    if idx < 16 {
        // Standard colors
        let colors: [(u8, u8, u8); 16] = [
            (0, 0, 0),
            (205, 49, 49),
            (13, 188, 121),
            (229, 229, 16),
            (36, 114, 200),
            (188, 63, 188),
            (17, 168, 205),
            (204, 204, 204),
            (118, 118, 118),
            (241, 76, 76),
            (35, 209, 139),
            (245, 245, 67),
            (59, 142, 234),
            (214, 112, 214),
            (41, 184, 219),
            (229, 229, 229),
        ];
        let (r, g, b) = colors[idx];
        AnsiColor { r, g, b }
    } else if idx < 232 {
        // 6x6x6 color cube
        let idx = idx - 16;
        let r = (idx / 36) as u8;
        let g = ((idx % 36) / 6) as u8;
        let b = (idx % 6) as u8;
        AnsiColor {
            r: if r > 0 { r * 40 + 55 } else { 0 },
            g: if g > 0 { g * 40 + 55 } else { 0 },
            b: if b > 0 { b * 40 + 55 } else { 0 },
        }
    } else {
        // Grayscale
        let v = ((idx - 232) * 10 + 8) as u8;
        AnsiColor { r: v, g: v, b: v }
    }
}
