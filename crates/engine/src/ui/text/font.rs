//! Parsed worker-side faces. Public handles hide the rasterizer implementation.

use super::{rasterizer::Rasterizer, resource::FontMetrics};
use crate::ui::resource::{Resource, ResourceIdentity};
use pixui_base::{PixuiResult, pixui_error};
use std::sync::OnceLock;

/// Immutable parsed face. Cloning retains its identity; separately loaded bytes
/// create a different identity even when equal. Only the worker uses this handle.
/// The initial character repertoire is Latin U+0020–024F plus U+FFFD.
#[derive(Clone)]
pub struct FontFace(pub(crate) Resource<Rasterizer>);

/// Static font bytes prepared by Cargo's build script from the pinned tool-tool
/// download. The executable never needs a cache path or runtime font files.
pub const GEIST_REGULAR_TTF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/Geist-Regular.ttf"));

/// Embedded monospace face from the same pinned Geist download.
pub const GEIST_MONO_REGULAR_TTF: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/GeistMono-Regular.ttf"));

impl FontFace {
    /// Parses a static TTF/OTF face. Collection indices are validated by fontdue.
    /// Variable axes, shaping, kerning, and system fallback are not supported.
    pub fn from_bytes(bytes: &[u8], face_index: u32) -> PixuiResult<Self> {
        Ok(Self(Resource::from_value(Rasterizer::new(
            bytes, face_index,
        )?)))
    }

    /// The pinned, embedded Geist Regular TTF. No runtime file access is needed.
    pub fn geist() -> PixuiResult<Self> {
        static FACE: OnceLock<Result<FontFace, String>> = OnceLock::new();
        FACE.get_or_init(|| {
            Self::from_bytes(GEIST_REGULAR_TTF, 0).map_err(|error| error.to_string())
        })
        .clone()
        .map_err(|error| pixui_error!("loading Geist: {error}"))
    }

    /// Embedded Geist Mono Regular, suitable for aligned diagnostic columns.
    pub fn geist_mono() -> PixuiResult<Self> {
        static FACE: OnceLock<Result<FontFace, String>> = OnceLock::new();
        FACE.get_or_init(|| {
            Self::from_bytes(GEIST_MONO_REGULAR_TTF, 0).map_err(|error| error.to_string())
        })
        .clone()
        .map_err(|error| pixui_error!("loading Geist Mono: {error}"))
    }

    pub(crate) fn identity(&self) -> ResourceIdentity<Rasterizer> {
        self.0.identity()
    }
}

/// Face, logical size, and physical rasterization scale. Validated before metrics
/// or allocation. Color and position deliberately do not affect cache identity.
#[derive(Clone)]
pub struct FontConfig {
    pub(crate) face: FontFace,
    pub(crate) size: f32,
    pub(crate) scale: f32,
}

impl FontConfig {
    pub fn new(face: FontFace, size: f32, scale: f32) -> PixuiResult<Self> {
        if !size.is_finite()
            || !scale.is_finite()
            || size <= 0.0
            || !(0.1..=16.0).contains(&scale)
            || !(1.0..=256.0).contains(&(size * scale))
        {
            return Err(pixui_error!("invalid font size or rasterization scale"));
        }
        Ok(Self { face, size, scale })
    }

    pub fn metrics(&self) -> FontMetrics {
        self.face.0.line_metrics(self.size, self.scale)
    }

    pub(crate) fn key(&self) -> FontKey {
        FontKey(
            self.face.identity(),
            self.size.to_bits(),
            self.scale.to_bits(),
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FontKey(ResourceIdentity<Rasterizer>, u32, u32);

/// A shared normalization policy for measurement and drawing. Four spaces per
/// tab; CRLF and CR become LF. Other controls are rejected, not rendered as tofu.
pub fn normalize(text: &str) -> PixuiResult<String> {
    let mut result = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                result.push('\n');
            }
            '\t' => result.push_str("    "),
            '\n' => result.push('\n'),
            character if character.is_control() => {
                return Err(pixui_error!(
                    "unsupported text control U+{:04X}",
                    character as u32
                ));
            }
            character => result.push(character),
        }
    }
    Ok(result)
}
