//! Fonts: a small bundled set (Latin/Greek/Cyrillic text, monospace, UI icons and modern monochrome
//! emoji) plus *lazily loaded* system fonts for any other script that appears (CJK, Arabic, Indic,
//! Thai, symbols…). A system font is only read into memory once text actually needs it.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontTweak};
use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};
use std::collections::HashSet;
#[cfg_attr(windows, allow(unused_imports))]
use std::path::{Path, PathBuf};
use std::sync::Arc;

const UBUNTU: &[u8] = include_bytes!("../assets/fonts/Ubuntu-Light.ttf");
const HACK: &[u8] = include_bytes!("../assets/fonts/Hack-Regular.ttf");
const ICONS: &[u8] = include_bytes!("../assets/fonts/emoji-icon-font.ttf");
const EMOJI: &[u8] = include_bytes!("../assets/fonts/NotoEmoji-Regular.ttf");

/// Largest system font we are willing to hold in memory.
const MAX_FONT_BYTES: u64 = 40 * 1024 * 1024;

pub struct Fonts {
    defs: FontDefinitions,
    /// Characters we already looked for (found or not), so each is searched at most once.
    tried: HashSet<char>,
    /// System font files already loaded or rejected.
    seen: HashSet<(PathBuf, u32)>,
    /// System fonts loaded so far: (bytes, collection index).
    loaded: Vec<(Vec<u8>, u32)>,
}

fn bundled() -> FontDefinitions {
    let mut d = FontDefinitions::empty();
    d.font_data.insert("Ubuntu-Light".into(), Arc::new(FontData::from_static(UBUNTU)));
    d.font_data.insert("Hack".into(), Arc::new(FontData::from_static(HACK)));
    d.font_data.insert("NotoEmoji".into(), Arc::new(FontData::from_static(EMOJI).tweak(FontTweak { scale: 0.85, ..Default::default() })));
    d.font_data.insert("emoji-icon-font".into(), Arc::new(FontData::from_static(ICONS).tweak(FontTweak { scale: 0.90, ..Default::default() })));
    // Same fallback order as egui's own defaults.
    d.families.insert(FontFamily::Proportional, vec!["Ubuntu-Light".into(), "NotoEmoji".into(), "emoji-icon-font".into()]);
    d.families.insert(FontFamily::Monospace, vec!["Hack".into(), "Ubuntu-Light".into(), "NotoEmoji".into(), "emoji-icon-font".into()]);
    d
}

impl Fonts {
    pub fn install(ctx: &egui::Context) -> Fonts {
        let defs = bundled();
        ctx.set_fonts(defs.clone());
        Fonts { defs, tried: HashSet::new(), seen: HashSet::new(), loaded: Vec::new() }
    }

    /// True if a bundled or already-loaded font can draw `c`.
    /// (egui's own `has_glyph` gives false negatives for fonts that contain its replacement glyph.)
    fn covered(&self, c: char) -> bool {
        [UBUNTU, EMOJI, ICONS, HACK].iter().any(|f| covers(f, 0, c)) || self.loaded.iter().any(|(b, i)| covers(b, *i, c))
    }

    /// Diagnostics: can `c` be drawn with the fonts loaded so far?
    pub fn can_draw(&self, c: char) -> bool {
        (c as u32) < 0x80 || c.is_whitespace() || self.covered(c)
    }

    /// Diagnostics: names of the system fonts loaded so far.
    pub fn loaded_names(&self) -> Vec<String> {
        self.defs.font_data.keys().filter(|k| k.starts_with("system:")).cloned().collect()
    }

    /// Make sure every character in `texts` can be drawn, loading system fonts as needed.
    pub fn ensure<'a>(&mut self, ctx: &egui::Context, texts: impl IntoIterator<Item = &'a str>) {
        let mut candidates: Vec<char> = Vec::new();
        for t in texts {
            for c in t.chars() {
                if (c as u32) < 0x80 || c.is_control() || c.is_whitespace() || matches!(c, '\u{200b}'..='\u{200f}' | '\u{fe00}'..='\u{fe0f}' | '\u{e0000}'..='\u{e007f}') {
                    continue; // ASCII, controls and invisible joiners/variation selectors
                }
                if self.tried.insert(c) {
                    candidates.push(c);
                }
            }
        }
        let mut changed = false;
        for c in candidates {
            if self.covered(c) {
                continue;
            }
            if let Some((path, index, bytes)) = self.find(c) {
                let name = format!("system:{}#{index}", path.display());
                let mut fd = FontData::from_owned(bytes.clone());
                fd.index = index;
                self.defs.font_data.insert(name.clone(), Arc::new(fd));
                for fam in [FontFamily::Proportional, FontFamily::Monospace] {
                    self.defs.families.entry(fam).or_default().push(name.clone());
                }
                self.loaded.push((bytes, index));
                changed = true;
            }
        }
        if changed {
            ctx.set_fonts(self.defs.clone());
        }
    }

    /// Find a system font with a drawable outline for `c`.
    fn find(&mut self, c: char) -> Option<(PathBuf, u32, Vec<u8>)> {
        for (path, index) in candidates(c) {
            if !self.seen.insert((path.clone(), index)) {
                continue;
            }
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            if meta.len() > MAX_FONT_BYTES {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            if covers(&bytes, index, c) {
                return Some((path, index, bytes));
            }
            // Not useful for this character; allow a retry for other characters later.
            self.seen.remove(&(path, index));
        }
        None
    }
}

/// True if the font has a visible outline (not just a colour bitmap) for `c`.
/// Some colour-bitmap emoji fonts also carry an outline table full of *empty* glyphs, so the
/// outline must have a non-empty bounding box.
fn covers(bytes: &[u8], index: u32, c: char) -> bool {
    let Ok(font) = FontRef::from_index(bytes, index) else { return false };
    let Some(gid) = font.charmap().map(c).filter(|g| g.to_u32() != 0) else { return false };
    if font.outline_glyphs().get(gid).is_none() {
        return false;
    }
    match font.glyph_metrics(Size::unscaled(), LocationRef::default()).bounds(gid) {
        Some(b) => b.x_max > b.x_min && b.y_max > b.y_min,
        None => false,
    }
}

/// Candidate (file, collection index) pairs, best first.
fn candidates(c: char) -> Vec<(PathBuf, u32)> {
    let mut v = Vec::new();
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // fontconfig knows which installed font covers the character.
        let pattern = format!(":charset={:x}", c as u32);
        for args in [vec!["-f", "%{file}\\n%{index}", pattern.as_str()], vec!["-s", "-f", "%{file}\\n%{index}\\n", pattern.as_str()]] {
            if let Ok(out) = std::process::Command::new("fc-match").args(&args).output() {
                let text = String::from_utf8_lossy(&out.stdout);
                let lines: Vec<&str> = text.lines().collect();
                for pair in lines.chunks(2).take(12) {
                    if let [file, idx] = pair {
                        v.push((PathBuf::from(file), idx.trim().parse().unwrap_or(0)));
                    }
                }
            }
        }
        for dir in font_dirs() {
            for name in ["DejaVuSans.ttf", "NotoSansSymbols2-Regular.ttf", "NotoSansSymbols-Regular.ttf", "Symbola.ttf", "DroidSansFallbackFull.ttf", "NotoSansCJK-Regular.ttc", "wqy-microhei.ttc"] {
                v.extend(find_file(&dir, name, 4).map(|p| (p, 0)));
            }
        }
    }
    #[cfg(windows)]
    {
        let dir = PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into())).join("Fonts");
        let cjk = matches!(c as u32, 0x3000..=0x9fff | 0xf900..=0xfaff | 0xff00..=0xffef | 0x20000..=0x3ffff);
        let hangul = matches!(c as u32, 0x1100..=0x11ff | 0x3130..=0x318f | 0xac00..=0xd7af);
        let mut names = vec!["seguisym.ttf", "segoeui.ttf", "seguiemj.ttf", "Nirmala.ttc", "Nirmala.ttf", "LeelawUI.ttf", "ebrima.ttf", "himalaya.ttf", "monbaiti.ttf", "taile.ttf", "ntailu.ttf", "phagspa.ttf", "msyi.ttf", "gadugi.ttf", "javatext.ttf", "mmrtext.ttf"];
        if hangul {
            names.insert(0, "malgun.ttf");
        }
        if cjk {
            names.splice(0..0, ["msyh.ttc", "YuGothR.ttc", "msjh.ttc", "malgun.ttf"]);
        }
        v.extend(names.into_iter().map(|n| (dir.join(n), 0)));
    }
    #[cfg(target_os = "macos")]
    {
        let _ = c;
        let home = std::env::var("HOME").unwrap_or_default();
        for dir in ["/System/Library/Fonts", "/System/Library/Fonts/Supplemental", "/Library/Fonts", &format!("{home}/Library/Fonts")] {
            for name in [
                "NotoEmoji-Regular.ttf",
                "Symbola.ttf",
                "Apple Symbols.ttf",
                "Hiragino Sans GB.ttc",
                "AppleSDGothicNeo.ttc",
                "Arial Unicode.ttf",
                "GeezaPro.ttc",
                "Thonburi.ttc",
                "Kohinoor.ttc",
                "STHeiti Light.ttc",
            ] {
                let p = Path::new(dir).join(name);
                for i in 0..if name.ends_with(".ttc") { 4 } else { 1 } {
                    v.push((p.clone(), i));
                }
            }
        }
    }
    v.retain(|(p, _)| p.is_file());
    let mut seen = HashSet::new();
    v.retain(|e| seen.insert(e.clone()));
    v
}

#[cfg(all(unix, not(target_os = "macos")))]
fn font_dirs() -> Vec<PathBuf> {
    let mut d: Vec<PathBuf> = ["/usr/share/fonts", "/usr/local/share/fonts", "/run/host/fonts", "/run/host/user-fonts"].iter().map(PathBuf::from).collect();
    if let Some(h) = std::env::var_os("HOME") {
        d.push(Path::new(&h).join(".local/share/fonts"));
        d.push(Path::new(&h).join(".fonts"));
    }
    d
}

#[cfg(all(unix, not(target_os = "macos")))]
fn find_file(dir: &Path, name: &str, depth: u32) -> Option<PathBuf> {
    let p = dir.join(name);
    if p.is_file() {
        return Some(p);
    }
    if depth == 0 {
        return None;
    }
    std::fs::read_dir(dir).ok()?.flatten().filter(|e| e.path().is_dir()).find_map(|e| find_file(&e.path(), name, depth - 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::FontId;

    #[test]
    fn bundled_emoji_cover_new_animals() {
        // 🦊 fox (Unicode 9) and 🦡 badger (Unicode 11) were missing from egui's old emoji font.
        for c in ['🦊', '🦡', '🥾', '🫠'] {
            assert!(covers(EMOJI, 0, c), "{c} missing");
        }
        // egui's UI symbols still come from the icon font.
        assert!(covers(ICONS, 0, '⟳'));
    }

    #[test]
    fn rejects_fonts_whose_outlines_are_empty() {
        // Older colour-bitmap emoji fonts map the character to an *empty* outline glyph.
        let b = include_bytes!("../tests/fixtures/empty-outline-fox.ttf");
        let font = FontRef::from_index(b, 0).unwrap();
        let gid = font.charmap().map('🦊').unwrap();
        assert!(font.outline_glyphs().get(gid).is_some(), "fixture should fool an outline-presence check");
        assert!(!covers(b, 0, '🦊'));
    }

    #[test]
    fn rejects_bitmap_only_colour_fonts() {
        // A colour-bitmap emoji font has the character mapped but no outline to draw.
        for p in ["/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf", "/usr/share/fonts/noto-emoji/NotoColorEmoji.ttf"] {
            if let Ok(b) = std::fs::read(p) {
                assert!(!covers(&b, 0, '🦊'));
            }
        }
    }

    #[test]
    fn loads_system_font_only_when_needed() {
        let ctx = egui::Context::default();
        let mut fonts = Fonts::install(&ctx);
        fonts.ensure(&ctx, ["Hello 🦊 Привет Ελλάδα"]); // all covered by the bundled fonts
        assert!(fonts.loaded.is_empty(), "loaded a system font unnecessarily");
        let t = std::time::Instant::now();
        fonts.ensure(&ctx, ["你好 世界", "مرحبا"]);
        let took = t.elapsed();
        if fonts.loaded.is_empty() {
            eprintln!("no CJK/Arabic system font on this machine; skipping");
            return;
        }
        eprintln!("loaded {} system font(s) in {took:?}", fonts.loaded.len());
        assert!(fonts.covered('你') && fonts.covered('界'));
        let n = fonts.loaded.len();
        fonts.ensure(&ctx, ["你好", "世界"]); // already handled: no further loading
        assert_eq!(fonts.loaded.len(), n);
        let _ = ctx.run_ui(Default::default(), |_| {});
        let uv = |c: char| ctx.fonts_mut(|f| f.layout_no_wrap(c.to_string(), FontId::proportional(14.0), egui::Color32::WHITE).rows[0].row.glyphs[0].uv_rect);
        assert_ne!(uv('你'), uv('\u{10fffd}'));
    }

    #[test]
    fn egui_draws_fox_and_badger() {
        let ctx = egui::Context::default();
        let fonts = Fonts::install(&ctx);
        assert!(fonts.covered('🦊') && fonts.covered('🦡') && fonts.covered('⚙'));
        let _ = ctx.run_ui(Default::default(), |_| {});
        // Lay out each character; a missing one is drawn as the replacement glyph, so a real glyph
        // has different texture coordinates from an unassigned code point.
        let uv = |c: char| {
            ctx.fonts_mut(|f| {
                let g = f.layout_no_wrap(c.to_string(), FontId::proportional(14.0), egui::Color32::WHITE);
                g.rows[0].row.glyphs[0].uv_rect
            })
        };
        let replacement = uv('\u{10fffd}');
        for c in ['🦊', '🦡', '⚙', 'B'] {
            assert_ne!(uv(c), replacement, "{c} renders as the replacement glyph");
        }
    }
}
