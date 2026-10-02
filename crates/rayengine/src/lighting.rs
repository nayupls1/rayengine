//! Basic world-space Lambert lighting shared by built-in lit materials.

use crate::Error;
use rayengine_core::glam::{Mat4, Vec3};

/// Maximum point lights evaluated per fragment. Extra lights are rejected.
pub const MAX_POINT_LIGHTS: usize = 4;

/// A light whose rays travel in `direction` in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirectionalLight {
    /// Finite, nonzero ray direction. Normalized when submitted.
    pub direction: Vec3,
    /// Nonnegative finite RGB irradiance; values above one are allowed.
    pub color: Vec3,
}

/// A world-space point light with a finite, smooth radius of influence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointLight {
    /// Finite world-space position.
    pub position: Vec3,
    /// Nonnegative finite RGB irradiance at the light.
    pub color: Vec3,
    /// Positive finite radius. Attenuation is `max(1 - distance/range, 0)^2`.
    pub range: f32,
}

/// Shared light configuration owned by Assets, updated before a camera pass.
///
/// RGB is used directly without gamma conversion. Diffuse irradiance is ambient
/// plus Lambert directional/point contributions. There are no shadows/specular.
#[derive(Clone, Debug, PartialEq)]
pub struct Lighting {
    /// Nonnegative finite RGB ambient irradiance.
    pub ambient: Vec3,
    /// Optional single directional light.
    pub directional: Option<DirectionalLight>,
    /// Up to MAX_POINT_LIGHTS point lights, in stable submission order.
    pub points: Vec<PointLight>,
}

impl Default for Lighting {
    fn default() -> Self {
        Self {
            ambient: Vec3::splat(0.15),
            directional: None,
            points: Vec::new(),
        }
    }
}

impl Lighting {
    /// Validates configuration without requiring a graphics context.
    pub fn validate(&self) -> Result<(), Error> {
        fn color(value: Vec3, name: &str) -> Result<(), Error> {
            if !value.is_finite() || value.min_element() < 0.0 {
                return Err(Error::Asset(format!(
                    "{name} must be finite, nonnegative RGB"
                )));
            }
            Ok(())
        }
        color(self.ambient, "ambient light")?;
        if let Some(light) = self.directional {
            if !valid_normal(light.direction) {
                return Err(Error::Asset("directional light direction must be finite, nonzero, with representable squared length".into()));
            }
            color(light.color, "directional light color")?;
        }
        if self.points.len() > MAX_POINT_LIGHTS {
            return Err(Error::Asset(format!(
                "at most {MAX_POINT_LIGHTS} point lights are supported; got {}",
                self.points.len()
            )));
        }
        for (index, light) in self.points.iter().enumerate() {
            if !light.position.is_finite() {
                return Err(Error::Asset(format!(
                    "point light {index} position must be finite"
                )));
            }
            color(light.color, &format!("point light {index} color"))?;
            if !light.range.is_finite() || light.range <= 0.0 || !light.range.recip().is_finite() {
                return Err(Error::Asset(format!(
                    "point light {index} range must be positive, finite, with a finite reciprocal"
                )));
            }
        }
        Ok(())
    }
}

pub(crate) fn valid_normal(normal: Vec3) -> bool {
    let length = normal.length_squared();
    normal.is_finite() && length.is_finite() && length > 0.0 && length.sqrt().recip().is_finite()
}

pub(crate) fn validate_transform(transform: Mat4) -> Result<(), Error> {
    let affine = transform.x_axis.w == 0.0
        && transform.y_axis.w == 0.0
        && transform.z_axis.w == 0.0
        && transform.w_axis.w == 1.0;
    if !transform.is_finite() || !affine || !transform.inverse().is_finite() {
        return Err(Error::Asset("lit draw requires a finite, invertible affine transform; remove zero scales or invalid matrix components".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_lights_and_limits() {
        let mut lights = Lighting::default();
        assert!(lights.validate().is_ok());
        for ambient in [
            Vec3::splat(-1.0),
            Vec3::splat(f32::NAN),
            Vec3::splat(f32::INFINITY),
        ] {
            lights.ambient = ambient;
            assert!(lights.validate().is_err());
        }
        lights.ambient = Vec3::ONE;
        for direction in [
            Vec3::ZERO,
            Vec3::splat(f32::NAN),
            Vec3::splat(f32::MAX),
            Vec3::splat(f32::MIN_POSITIVE),
        ] {
            lights.directional = Some(DirectionalLight {
                direction,
                color: Vec3::ONE,
            });
            assert!(lights.validate().is_err());
        }
        lights.directional = Some(DirectionalLight {
            direction: Vec3::Y * 2.0,
            color: Vec3::ONE,
        });
        let point = PointLight {
            position: Vec3::ZERO,
            color: Vec3::ONE,
            range: 2.0,
        };
        lights.points = vec![point; MAX_POINT_LIGHTS];
        assert!(lights.validate().is_ok());
        lights.points.push(point);
        assert!(
            lights
                .validate()
                .unwrap_err()
                .to_string()
                .contains("at most 4")
        );
        lights.points = vec![point];
        for range in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::from_bits(1)] {
            lights.points[0].range = range;
            assert!(lights.validate().is_err());
        }
        lights.points[0] = point;
        lights.points[0].position.x = f32::NAN;
        assert!(lights.validate().is_err());
        lights.points[0] = point;
        lights.points[0].color.y = -0.1;
        assert!(lights.validate().is_err());
    }

    #[test]
    fn normals_and_inverse_transpose_requirements() {
        assert!(valid_normal(Vec3::Z * 3.0));
        assert!(!valid_normal(Vec3::ZERO));
        assert!(!valid_normal(Vec3::splat(f32::INFINITY)));
        let transform = Mat4::from_scale_rotation_translation(
            Vec3::new(2.0, 0.5, 3.0),
            rayengine_core::glam::Quat::from_rotation_y(0.7),
            Vec3::ONE,
        );
        validate_transform(transform).unwrap();
        let normal = Vec3::new(1.0, 1.0, 0.0).normalize();
        let tangent = Vec3::new(1.0, -1.0, 0.0);
        let transformed_normal = transform
            .inverse()
            .transpose()
            .transform_vector3(normal)
            .normalize();
        assert!(
            transformed_normal
                .dot(transform.transform_vector3(tangent))
                .abs()
                < 1e-6
        );
        for transform in [
            Mat4::from_scale(Vec3::new(0.0, 1.0, 1.0)),
            Mat4::from_cols_array(&[f32::NAN; 16]),
            Mat4::perspective_rh_gl(1.0, 1.0, 0.1, 100.0),
        ] {
            assert!(validate_transform(transform).is_err());
        }
    }
}

#[cfg(test)]
mod native_tests;
