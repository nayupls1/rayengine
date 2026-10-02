//! Reusable surface descriptions and cached, typed shader parameters.

use crate::assets::{ShaderId, TextureId};
use rayengine_core::glam::{Mat4, Vec2, Vec3, Vec4};
use raylib::prelude::Color;

/// How a surface handles alpha and depth writes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum AlphaMode {
    /// Disable blending and write depth. The built-in shader outputs alpha one.
    #[default]
    Opaque,
    /// Discard fragments below this threshold; surviving fragments write depth.
    Cutout(f32),
    /// Straight-alpha blending, with depth testing and no depth writes.
    /// Draw these surfaces after opaque/cutout geometry, from far to near.
    Blend,
}

impl AlphaMode {
    pub(crate) fn validate(self) -> Result<(), crate::Error> {
        if let Self::Cutout(cutoff) = self
            && (!cutoff.is_finite() || !(0.0..=1.0).contains(&cutoff))
        {
            return Err(crate::Error::Asset(
                "alpha cutoff must be finite and in 0..=1".into(),
            ));
        }
        Ok(())
    }
}

/// Cached uniform binding belonging to one shader in the current game run.
/// Unloading that shader permanently invalidates this binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UniformId {
    pub(crate) shader: ShaderId,
    pub(crate) index: usize,
}

/// Supported scalar, vector, and matrix shader values. Float components must be finite.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UniformValue {
    /// GLSL float.
    Float(f32),
    /// GLSL int.
    Int(i32),
    /// GLSL bool.
    Bool(bool),
    /// GLSL vec2.
    Vec2(Vec2),
    /// GLSL vec3.
    Vec3(Vec3),
    /// GLSL vec4.
    Vec4(Vec4),
    /// GLSL mat4, using glam's column-major convention.
    Mat4(Mat4),
}

impl UniformValue {
    pub(crate) fn gl_type(self) -> u32 {
        match self {
            Self::Float(_) => 0x1406,
            Self::Int(_) => 0x1404,
            Self::Bool(_) => 0x8B56,
            Self::Vec2(_) => 0x8B50,
            Self::Vec3(_) => 0x8B51,
            Self::Vec4(_) => 0x8B52,
            Self::Mat4(_) => 0x8B5C,
        }
    }

    pub(crate) fn validate(self) -> Result<(), crate::Error> {
        let finite = match self {
            Self::Float(v) => v.is_finite(),
            Self::Int(_) | Self::Bool(_) => true,
            Self::Vec2(v) => v.is_finite(),
            Self::Vec3(v) => v.is_finite(),
            Self::Vec4(v) => v.is_finite(),
            Self::Mat4(v) => v.is_finite(),
        };
        if finite {
            Ok(())
        } else {
            Err(crate::Error::Asset("shader values must be finite".into()))
        }
    }
}

/// Per-material override of a shader's registered default value.
#[derive(Clone, Copy, Debug)]
pub struct MaterialParam {
    /// Binding returned by InitContext::uniform or Frame::uniform.
    pub uniform: UniformId,
    /// Same GLSL type as the binding's registered value.
    pub value: UniformValue,
}

/// Built-in surface shading. Custom shaders require Unlit and implement their own lighting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shading {
    /// Texture, vertex color, and tint only (the default).
    #[default]
    Unlit,
    /// World-space ambient, directional, and point Lambert lighting.
    Lit,
}

/// CPU surface description. Materials borrow texture/shader handles, never own them.
///
/// Missing texture uses white; missing shader uses the built-in shader selected by shading.
/// Unloading a dependency makes drawing return false until the description is
/// replaced with live handles. Parameters override shader defaults per draw,
/// so surfaces sharing a shader do not inherit another surface's overrides.
#[derive(Clone, Debug)]
pub struct MaterialDesc {
    /// Optional albedo texture, sampled through texture0.
    pub texture: Option<TextureId>,
    /// Optional custom shader with raylib's standard mesh attributes/uniforms.
    pub shader: Option<ShaderId>,
    /// Built-in shading; Lit requires valid normals and an invertible affine transform.
    pub shading: Shading,
    /// Multiplied by vertex colors, texture pixels, and the draw tint.
    pub tint: Color,
    /// Blending, fragment discard, and depth-write policy.
    pub alpha: AlphaMode,
    /// Cached shader parameter overrides; allocated only when creating/editing material data.
    pub parameters: Vec<MaterialParam>,
}

impl Default for MaterialDesc {
    fn default() -> Self {
        Self {
            texture: None,
            shader: None,
            shading: Shading::Unlit,
            tint: Color::WHITE,
            alpha: AlphaMode::Opaque,
            parameters: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_cutoffs_and_nonfinite_parameters() {
        for cutoff in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
            assert!(AlphaMode::Cutout(cutoff).validate().is_err());
        }
        for cutoff in [0.0, 0.5, 1.0] {
            assert!(AlphaMode::Cutout(cutoff).validate().is_ok());
        }
        for value in [
            UniformValue::Float(f32::NAN),
            UniformValue::Vec2(Vec2::splat(f32::INFINITY)),
            UniformValue::Vec3(Vec3::splat(f32::NAN)),
            UniformValue::Vec4(Vec4::splat(f32::NEG_INFINITY)),
            UniformValue::Mat4(Mat4::from_cols_array(&[f32::NAN; 16])),
        ] {
            assert!(value.validate().is_err());
        }
        for value in [
            UniformValue::Float(1.0),
            UniformValue::Int(-3),
            UniformValue::Bool(true),
            UniformValue::Mat4(Mat4::IDENTITY),
        ] {
            assert!(value.validate().is_ok());
        }
    }
}
