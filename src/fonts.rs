//! Bundled CJK font (Noto Sans CJK TC subset, SIL OFL 1.1) and system fallbacks.
//! Same font and loading scheme as whitebox-video-storyboard.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Noto Sans CJK TC Regular, subset to ASCII/Latin-1, punctuation, symbols,
/// Bopomofo, all Big5 and GB2312 characters (~16.8k glyphs), embedded
/// Brotli-compressed (7.5 MB → 5.0 MB).
static CJK_FONT_BR: &[u8] = include_bytes!("../assets/fonts/NotoSansCJKtc-Subset.otf.br");

/// The bundled CJK font (decompressed once, ~0.1 s, on first use).
pub fn cjk_font() -> &'static [u8] {
    static FONT: OnceLock<Vec<u8>> = OnceLock::new();
    FONT.get_or_init(|| {
        let mut out = Vec::with_capacity(7_600_000);
        brotli_decompressor::BrotliDecompress(&mut &CJK_FONT_BR[..], &mut out).expect("bundled font is valid brotli");
        out
    })
}
pub static CJK_FONT_LICENSE: &str = include_str!("../assets/fonts/LICENSE-OFL.txt");

/// System fonts tried (in order) for characters missing from the bundled subset.
/// `(path, face index)`. Override with the `RSS_FALLBACK_FONT` environment variable.
pub fn system_fallback_candidates() -> Vec<(PathBuf, u32)> {
    let mut v = vec![];
    if let Ok(p) = std::env::var("RSS_FALLBACK_FONT") {
        v.push((PathBuf::from(p), 0));
    }
    for (p, i) in [
        // Linux (fonts-noto-cjk): face 3 = Noto Sans CJK TC
        ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 3),
        ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 3),
        ("/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc", 3),
        ("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", 0),
        // Windows
        ("C:\\Windows\\Fonts\\msjh.ttc", 0),
        ("C:\\Windows\\Fonts\\msyh.ttc", 0),
        ("C:\\Windows\\Fonts\\mingliu.ttc", 0),
        // macOS
        ("/System/Library/Fonts/PingFang.ttc", 0),
        ("/System/Library/Fonts/STHeiti Medium.ttc", 0),
        ("/Library/Fonts/Arial Unicode.ttf", 0),
    ] {
        v.push((PathBuf::from(p), i));
    }
    v
}

/// First existing system fallback font, loaded into memory.
pub fn load_system_fallback() -> Option<(Vec<u8>, u32)> {
    system_fallback_candidates().into_iter().find_map(|(p, i)| std::fs::read(&p).ok().map(|b| (b, i)))
}

#[cfg(test)]
mod tests {
    #[test]
    fn bundled_font_has_traditional_chinese() {
        use ab_glyph::Font;
        let f = ab_glyph::FontRef::try_from_slice(super::cjk_font()).unwrap();
        for c in "繁體中文分鏡角色場景".chars() {
            assert_ne!(f.glyph_id(c).0, 0, "{c} missing");
        }
    }
}
