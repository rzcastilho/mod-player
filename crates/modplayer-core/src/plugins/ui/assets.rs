// SPDX-License-Identifier: MIT OR Apache-2.0

//! Plugin icon/glyph assets (FR-014a, research R11): PNG-only, decoded
//! once at discovery, never fetched from a URL, never re-decoded per
//! frame. Every failure (missing file, wrong format, over the size or
//! dimension cap) is a console warning and the asset is simply omitted
//! — never a manifest error (`manifest.rs` Rule 3 only rejects a
//! malformed `[glyphs]` *table*, not a bad *file*).
//!
//! `load()`'s byte source is a package's embedded resources
//! (`BundledPackage`); no bundled or fixture package embeds any yet
//! (research R18's `ui-icons` fixture, a later user story's task, is the
//! first), so every declared `icon`/`glyphs` entry currently resolves to
//! "not found" — the warning path this module exists to prove, exactly
//! as FR-014a specifies for that case. [`decode_png_bytes`] itself (the
//! actual PNG→RGBA decode, size/dimension enforcement) is exercised
//! directly by this module's own tests.

use std::collections::BTreeMap;
use std::sync::Arc;

use modplayer_capability_gateway::manifest::Manifest;
use modplayer_capability_gateway::ui::limits::{
    GLYPH_MAX_BYTES, GLYPH_MAX_PX, ICON_MAX_BYTES, ICON_MAX_PX,
};

use super::super::BundledPackage;
use super::super::log::PluginLog;

/// One decoded PNG, held ready for `modplayer-ui` to upload as a
/// `TextureHandle` on first draw (data-model.md §4.6) — decoding never
/// happens again after [`load`].
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedPng {
    pub width: u32,
    pub height: u32,
    /// Straight (non-premultiplied) RGBA8, `width * height * 4` bytes,
    /// row-major. `Arc` so a texture-cache clone (per plugin, per frame
    /// lookup) never re-copies the pixel data.
    pub rgba: Arc<[u8]>,
}

/// A plugin's decoded icon and named glyphs (data-model.md §4.6),
/// carried on its [`super::super::PluginRecord`]. Empty — never an
/// error — for a plugin whose manifest declares no `icon`/`glyphs` at
/// all, or whose declared assets all failed to resolve.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PluginAssets {
    pub icon: Option<DecodedPng>,
    pub glyphs: BTreeMap<String, DecodedPng>,
}

/// Why a declared asset was omitted (console warning text only — never
/// surfaced to the plugin, never a manifest error).
enum AssetError {
    NotFound,
    Decode,
    TooLarge {
        limit: usize,
        actual: usize,
    },
    TooBig {
        limit: u32,
        actual_w: u32,
        actual_h: u32,
    },
}

impl AssetError {
    fn describe(&self) -> String {
        match self {
            AssetError::NotFound => "the file was not found in the package".to_string(),
            AssetError::Decode => "the file is not a valid PNG".to_string(),
            AssetError::TooLarge { limit, actual } => {
                format!("the file is {actual} bytes, over the {limit}-byte cap")
            }
            AssetError::TooBig {
                limit,
                actual_w,
                actual_h,
            } => format!("the image is {actual_w}x{actual_h}px, over the {limit}px cap"),
        }
    }
}

/// The real decode path (FR-014a): reject anything over `max_bytes`
/// before ever decoding, then reject anything whose *actual* pixel
/// dimensions are asymmetric-but-capped (`max_px` applies to both width
/// and height), then decode to straight RGBA8 via the `image` crate
/// (Constitution X: no new crate — `image` already gains the `png`
/// feature, T002).
fn decode_png_bytes(bytes: &[u8], max_px: u32, max_bytes: usize) -> Result<DecodedPng, AssetError> {
    if bytes.len() > max_bytes {
        return Err(AssetError::TooLarge {
            limit: max_bytes,
            actual: bytes.len(),
        });
    }
    let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .map_err(|_| AssetError::Decode)?;
    let rgba = decoded.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    if width > max_px || height > max_px {
        return Err(AssetError::TooBig {
            limit: max_px,
            actual_w: width,
            actual_h: height,
        });
    }
    Ok(DecodedPng {
        width,
        height,
        rgba: Arc::from(rgba.into_raw().into_boxed_slice()),
    })
}

/// `package`'s embedded resource bytes for `path` (research R11): a
/// linear scan of `BundledPackage::resources` (at most a handful of
/// entries per package — `MAX_GLYPHS` + one icon, never worth a map).
/// `None` for a package with no such entry — including every package
/// before the `ui-icons` fixture (US3 T091) that declares no `icon`/
/// `[glyphs]` at all, whose `resources` stays empty.
fn resource_bytes<'a>(package: &'a BundledPackage, path: &str) -> Option<&'a [u8]> {
    package
        .resources
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, bytes)| *bytes)
}

/// L1/FR-014a: resolve every asset a plugin's manifest declares — its
/// `icon` and each `[glyphs]` entry — against `package`'s embedded
/// bytes, decoding what resolves and warning (via `log`) about anything
/// that doesn't. Called once, at discovery; the result never changes
/// for the life of the record (a plugin cannot re-declare its manifest
/// without a reinstall, out of this slice's scope).
pub fn load(package: &BundledPackage, manifest: &Manifest, log: &mut PluginLog) -> PluginAssets {
    let mut assets = PluginAssets::default();

    if let Some(icon_path) = &manifest.icon {
        match resource_bytes(package, icon_path) {
            None => warn(log, manifest, "icon", icon_path, &AssetError::NotFound),
            Some(bytes) => match decode_png_bytes(bytes, ICON_MAX_PX, ICON_MAX_BYTES) {
                Ok(png) => assets.icon = Some(png),
                Err(err) => warn(log, manifest, "icon", icon_path, &err),
            },
        }
    }

    for (key, glyph_path) in &manifest.glyphs {
        match resource_bytes(package, glyph_path) {
            None => warn(log, manifest, "glyph", glyph_path, &AssetError::NotFound),
            Some(bytes) => match decode_png_bytes(bytes, GLYPH_MAX_PX, GLYPH_MAX_BYTES) {
                Ok(png) => {
                    assets.glyphs.insert(key.clone(), png);
                }
                Err(err) => warn(log, manifest, "glyph", glyph_path, &err),
            },
        }
    }

    assets
}

fn warn(log: &mut PluginLog, manifest: &Manifest, kind: &str, path: &str, err: &AssetError) {
    log.push(
        manifest.identifier.clone(),
        log::Level::Warn,
        format!(
            "{kind} '{path}' was not loaded: {desc}; a generic glyph is used instead.",
            desc = err.describe()
        ),
    );
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    /// A valid, freshly encoded NxN PNG — encoded with the `image` crate
    /// itself (its `png` feature covers both directions, T002) rather
    /// than checked in as opaque bytes, so the fixture can never drift
    /// from whatever `decode_png_bytes` actually expects.
    fn make_png(side: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(side, side, image::Rgba([255, 0, 0, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap_or_else(|e| unreachable!("encoding the test fixture PNG must succeed: {e}"));
        bytes
    }

    #[test]
    fn decodes_a_valid_png_to_straight_rgba() {
        let bytes = make_png(1);
        let png = decode_png_bytes(&bytes, ICON_MAX_PX, ICON_MAX_BYTES)
            .unwrap_or_else(|_| unreachable!("the fixture PNG must decode"));
        assert_eq!((png.width, png.height), (1, 1));
        assert_eq!(png.rgba.len(), 4);
        assert_eq!(&*png.rgba, &[255, 0, 0, 255]);
    }

    #[test]
    fn rejects_bytes_over_the_size_cap() {
        let bytes = make_png(1);
        let err = decode_png_bytes(&bytes, ICON_MAX_PX, 4)
            .err()
            .unwrap_or_else(|| unreachable!("an over-cap file must be rejected"));
        assert!(matches!(err, AssetError::TooLarge { .. }));
    }

    #[test]
    fn rejects_dimensions_over_the_pixel_cap() {
        let bytes = make_png(4);
        let err = decode_png_bytes(&bytes, 2, ICON_MAX_BYTES)
            .err()
            .unwrap_or_else(|| unreachable!("an over-cap dimension must be rejected"));
        assert!(matches!(err, AssetError::TooBig { .. }));
    }

    #[test]
    fn rejects_non_png_bytes() {
        let err = decode_png_bytes(b"not a png", ICON_MAX_PX, ICON_MAX_BYTES)
            .err()
            .unwrap_or_else(|| unreachable!("non-PNG bytes must be rejected"));
        assert!(matches!(err, AssetError::Decode));
    }

    #[test]
    fn load_warns_and_omits_when_nothing_is_declared() {
        let manifest_toml = "identifier = \"org.modplayer.test.assets\"\n\
             name = \"Assets test\"\n\
             version = \"1.0.0\"\n\
             api = \"1.0\"\n\
             author = \"Test\"\n\
             license = \"MIT\"\n\
             source = \"test\"\n";
        let manifest =
            modplayer_capability_gateway::manifest::parse_and_validate(manifest_toml, true)
                .unwrap_or_else(|e| unreachable!("test manifest must be valid: {e}"));
        let package = BundledPackage {
            identifier: "org.modplayer.test.assets",
            manifest_toml,
            entry: "",
            readme: "",
            fixture: true,
            resources: &[],
        };
        let mut log = PluginLog::new();
        let assets = load(&package, &manifest, &mut log);
        assert!(assets.icon.is_none());
        assert!(assets.glyphs.is_empty());
        assert!(
            log.is_empty(),
            "no icon/glyphs declared, nothing to warn about"
        );
    }
}
