//! GPU shader ownership and CPU material descriptions, resolved before each draw.

use super::{MaterialId, ShaderId, TextureId};
use crate::{
    Error,
    lighting::{Lighting, MAX_POINT_LIGHTS},
    material::{AlphaMode, MaterialDesc, Shading, UniformId, UniformValue},
    render::{matrix, v2, v3},
};
use raylib::prelude::*;
use std::collections::HashMap;

mod gpu;
pub(crate) use gpu::SurfaceGuard;

pub(super) struct ShaderAsset {
    native: Shader,
    mode: i32,
    cutoff: i32,
    last_alpha: Option<(i32, f32)>,
    uniforms: Vec<Uniform>,
    names: HashMap<String, usize>,
}

struct Uniform {
    location: i32,
    value: UniformValue,
    uploaded: Option<UniformValue>,
}

impl ShaderAsset {
    fn compile(
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        vertex: Option<&str>,
        fragment: &str,
    ) -> Result<Self, Error> {
        if fragment.trim().is_empty()
            || fragment.contains('\0')
            || vertex.is_some_and(|v| v.trim().is_empty() || v.contains('\0'))
        {
            return Err(Error::Asset(
                "shader source must be nonempty without NUL".into(),
            ));
        }
        let native = raylib.load_shader_from_memory(thread, vertex, Some(fragment));
        if !gpu::is_custom(&native) {
            return Err(Error::Asset(
                "shader compilation/link failed (raylib fallback rejected)".into(),
            ));
        }
        if native.get_shader_location_attribute("vertexPosition") != 0
            || native.get_shader_location("mvp") < 0
        {
            return Err(Error::Asset(
                "mesh shaders require vertexPosition at location 0 and an active mvp uniform"
                    .into(),
            ));
        }
        for (name, expected) in [
            ("vertexTexCoord", 1),
            ("vertexNormal", 2),
            ("vertexColor", 3),
        ] {
            let actual = native.get_shader_location_attribute(name);
            if actual >= 0 && actual != expected {
                return Err(Error::Asset(format!(
                    "{name} must use attribute location {expected}"
                )));
            }
        }
        let mode = native.get_shader_location("rayengineAlphaMode");
        let cutoff = native.get_shader_location("rayengineAlphaCutoff");
        for (name, location, kind) in [
            ("rayengineAlphaMode", mode, 0x1404),
            ("rayengineAlphaCutoff", cutoff, 0x1406),
        ] {
            if location >= 0 && gpu::uniform_type(&native, name)? != kind {
                return Err(Error::Asset(format!("{name} has an invalid GLSL type")));
            }
        }
        Ok(Self {
            native,
            mode,
            cutoff,
            last_alpha: None,
            uniforms: Vec::new(),
            names: HashMap::new(),
        })
    }

    pub(super) fn register(
        &mut self,
        shader: ShaderId,
        name: &str,
        value: UniformValue,
    ) -> Result<UniformId, Error> {
        value.validate()?;
        if name.is_empty()
            || name.contains('\0')
            || matches!(
                name,
                "mvp"
                    | "matModel"
                    | "matView"
                    | "matProjection"
                    | "matNormal"
                    | "colDiffuse"
                    | "texture0"
                    | "rayengineAlphaMode"
                    | "rayengineAlphaCutoff"
            )
        {
            return Err(Error::Asset(
                "uniform name is invalid or reserved by the renderer".into(),
            ));
        }
        if let Some(&index) = self.names.get(name) {
            let uniform = &mut self.uniforms[index];
            if uniform.value.gl_type() != value.gl_type() {
                return Err(Error::Asset("uniform value has wrong GLSL type".into()));
            }
            uniform.value = value;
            return Ok(UniformId { shader, index });
        }
        if gpu::uniform_type(&self.native, name)? != value.gl_type() {
            return Err(Error::Asset("uniform value has wrong GLSL type".into()));
        }
        let location = self.native.get_shader_location(name);
        if location < 0 {
            return Err(Error::Asset("uniform is missing or optimized out".into()));
        }
        let index = self.uniforms.len();
        self.uniforms.push(Uniform {
            location,
            value,
            uploaded: None,
        });
        self.names.insert(name.to_owned(), index);
        Ok(UniformId { shader, index })
    }

    pub(super) fn set(&mut self, binding: UniformId, value: UniformValue) -> Result<(), Error> {
        value.validate()?;
        let slot = self
            .uniforms
            .get_mut(binding.index)
            .ok_or_else(|| Error::Asset("invalid uniform binding".into()))?;
        if value.gl_type() != slot.value.gl_type() {
            return Err(Error::Asset("uniform value has wrong GLSL type".into()));
        }
        slot.value = value;
        Ok(())
    }
}

pub(super) struct MaterialAssets {
    pub(super) shaders: Vec<Option<ShaderAsset>>,
    pub(super) materials: Vec<Option<MaterialDesc>>,
    backend: Option<Backend>,
    lighting: Lighting,
}
struct Backend {
    builtin: ShaderAsset,
    lit: ShaderAsset,
    lights: LightUniforms,
    lighting_dirty: bool,
    state: gpu::RenderState,
}

impl MaterialAssets {
    pub(super) fn resource_counts(&self) -> (u64, u64) {
        (
            self.shaders.iter().flatten().count() as u64 + 2 * u64::from(self.backend.is_some()),
            self.materials.iter().flatten().count() as u64,
        )
    }
    pub(super) fn new() -> Self {
        Self {
            shaders: Vec::new(),
            materials: Vec::new(),
            backend: None,
            lighting: Lighting::default(),
        }
    }
    pub(super) fn initialize(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
    ) -> Result<(), Error> {
        if self.backend.is_none() {
            let lit = ShaderAsset::compile(
                raylib,
                thread,
                Some(include_str!("materials/lit.vs")),
                include_str!("materials/lit.fs"),
            )?;
            let lights = LightUniforms::new(&lit.native)?;
            self.backend = Some(Backend {
                lit,
                lights,
                lighting_dirty: true,
                state: gpu::RenderState::load(thread)?,
                builtin: ShaderAsset::compile(
                    raylib,
                    thread,
                    None,
                    include_str!("materials/default.fs"),
                )?,
            });
        }
        Ok(())
    }
    pub(super) fn shader(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        vertex: Option<&str>,
        fragment: &str,
    ) -> Result<ShaderId, Error> {
        let shader = ShaderAsset::compile(raylib, thread, vertex, fragment)?;
        let id = ShaderId(self.shaders.len());
        self.shaders.push(Some(shader));
        Ok(id)
    }
    pub(super) fn descriptor(&self, id: MaterialId) -> Option<&MaterialDesc> {
        self.materials.get(id.0).and_then(Option::as_ref)
    }
    pub(super) fn validate(
        &self,
        desc: &MaterialDesc,
        textures: &[Option<Texture2D>],
    ) -> Result<(), Error> {
        desc.alpha.validate()?;
        if desc.shading == Shading::Lit && desc.shader.is_some() {
            return Err(Error::Asset("Shading::Lit uses the built-in lighting shader; remove the custom shader or use Shading::Unlit".into()));
        }
        if desc
            .texture
            .is_some_and(|id| textures.get(id.0).and_then(Option::as_ref).is_none())
        {
            return Err(Error::Asset("material texture is unloaded".into()));
        }
        let shader = match desc.shader {
            Some(id) => self
                .shaders
                .get(id.0)
                .and_then(Option::as_ref)
                .ok_or_else(|| Error::Asset("material shader is unloaded".into()))?,
            None => {
                &self
                    .backend
                    .as_ref()
                    .ok_or_else(|| Error::Asset("material renderer is not initialized".into()))?
                    .builtin
            }
        };
        if matches!(desc.alpha, AlphaMode::Cutout(_)) && (shader.mode < 0 || shader.cutoff < 0) {
            return Err(Error::Asset(
                "cutout shaders require rayengineAlphaMode and rayengineAlphaCutoff uniforms"
                    .into(),
            ));
        }
        for param in &desc.parameters {
            param.value.validate()?;
            if Some(param.uniform.shader) != desc.shader {
                return Err(Error::Asset(
                    "material parameter belongs to another shader".into(),
                ));
            }
            let slot = shader
                .uniforms
                .get(param.uniform.index)
                .ok_or_else(|| Error::Asset("invalid material parameter".into()))?;
            if slot.value.gl_type() != param.value.gl_type() {
                return Err(Error::Asset(
                    "material parameter has wrong GLSL type".into(),
                ));
            }
        }
        Ok(())
    }
    pub(super) fn lighting(&self) -> &Lighting {
        &self.lighting
    }
    pub(super) fn set_lighting(&mut self, lighting: Lighting) -> Result<(), Error> {
        lighting.validate()?;
        self.lighting = lighting;
        if let Some(backend) = &mut self.backend {
            backend.lighting_dirty = true;
        }
        Ok(())
    }
    pub(super) fn pass(&self) -> Option<SurfaceGuard> {
        self.backend.as_ref().map(|backend| backend.state.begin())
    }

    pub(super) fn prepare<'a>(
        &'a mut self,
        id: MaterialId,
        textures: &'a [Option<Texture2D>],
        tint: Color,
    ) -> Option<Prepared<'a>> {
        let desc = self.materials.get(id.0)?.as_ref()?;
        let texture = match desc.texture {
            Some(TextureId(id)) => Some(textures.get(id)?.as_ref()?),
            None => None,
        };
        let shader = match desc.shader {
            Some(ShaderId(id)) => self.shaders.get_mut(id)?.as_mut()?,
            None => {
                let backend = self.backend.as_mut()?;
                if desc.shading == Shading::Lit {
                    if backend.lighting_dirty {
                        backend
                            .lights
                            .upload(&mut backend.lit.native, &self.lighting);
                        backend.lighting_dirty = false;
                    }
                    &mut backend.lit
                } else {
                    &mut backend.builtin
                }
            }
        };
        for (index, uniform) in shader.uniforms.iter_mut().enumerate() {
            // Resolve the final value once. The last override wins if the
            // description supplies the same binding more than once.
            let effective = desc
                .parameters
                .iter()
                .rev()
                .find(|param| param.uniform.index == index)
                .map(|param| param.value)
                .unwrap_or(uniform.value);
            if uniform.uploaded != Some(effective) {
                set_value(&mut shader.native, uniform.location, effective);
                uniform.uploaded = Some(effective);
            }
        }
        let (mode, cutoff) = match desc.alpha {
            AlphaMode::Opaque => (0, 0.0),
            AlphaMode::Cutout(t) => (1, t),
            AlphaMode::Blend => (2, 0.0),
        };
        // Engine-owned programs retain uniform values between draws. Alpha
        // uniforms are reserved, so only this path can change them.
        if shader.last_alpha != Some((mode, cutoff)) {
            if shader.mode >= 0 {
                shader.native.set_shader_value(shader.mode, mode);
            }
            if shader.cutoff >= 0 {
                shader.native.set_shader_value(shader.cutoff, cutoff);
            }
            shader.last_alpha = Some((mode, cutoff));
        }
        let mul = |a: u8, b: u8| ((u16::from(a) * u16::from(b) + 127) / 255) as u8;
        Some(Prepared {
            shader: &shader.native,
            texture,
            tint: Color::new(
                mul(desc.tint.r, tint.r),
                mul(desc.tint.g, tint.g),
                mul(desc.tint.b, tint.b),
                mul(desc.tint.a, tint.a),
            ),
            alpha: desc.alpha,
        })
    }
}

pub(crate) struct Prepared<'a> {
    shader: &'a Shader,
    texture: Option<&'a Texture2D>,
    tint: Color,
    pub(crate) alpha: AlphaMode,
}
impl Prepared<'_> {
    pub(crate) fn draw<D: RaylibDraw + RaylibDraw3D>(
        &self,
        raw: &mut D,
        mesh: impl AsRef<raylib::ffi::Mesh>,
        transform: Matrix,
    ) {
        gpu::draw(raw, mesh, self.shader, self.texture, self.tint, transform);
    }
}

fn set_value(shader: &mut Shader, location: i32, value: UniformValue) {
    match value {
        UniformValue::Float(v) => shader.set_shader_value(location, v),
        UniformValue::Int(v) => shader.set_shader_value(location, v),
        UniformValue::Bool(v) => shader.set_shader_value(location, i32::from(v)),
        UniformValue::Vec2(v) => shader.set_shader_value(location, v2(v)),
        UniformValue::Vec3(v) => shader.set_shader_value(location, v3(v)),
        UniformValue::Vec4(v) => shader.set_shader_value(location, [v.x, v.y, v.z, v.w]),
        UniformValue::Mat4(v) => shader.set_shader_value_matrix(location, matrix(v)),
    }
}

#[cfg(test)]
mod tests;

/// Load a state-only guard without creating shaders or materials.
pub(crate) fn alpha_pass(thread: &RaylibThread) -> Result<SurfaceGuard, Error> {
    Ok(gpu::RenderState::load(thread)?.begin())
}

// Fixed-size cached bindings; all string lookups happen once during setup.
struct LightUniforms {
    ambient: i32,
    direction: i32,
    directional_color: i32,
    count: i32,
    positions: [i32; MAX_POINT_LIGHTS],
    colors: [i32; MAX_POINT_LIGHTS],
    ranges: [i32; MAX_POINT_LIGHTS],
}
impl LightUniforms {
    fn new(shader: &Shader) -> Result<Self, Error> {
        let result = Self {
            ambient: shader.get_shader_location("ambient"),
            direction: shader.get_shader_location("direction"),
            directional_color: shader.get_shader_location("directionalColor"),
            count: shader.get_shader_location("pointCount"),
            positions: std::array::from_fn(|i| {
                shader.get_shader_location(&format!("pointPosition[{i}]"))
            }),
            colors: std::array::from_fn(|i| {
                shader.get_shader_location(&format!("pointColor[{i}]"))
            }),
            ranges: std::array::from_fn(|i| {
                shader.get_shader_location(&format!("pointRange[{i}]"))
            }),
        };
        if [
            result.ambient,
            result.direction,
            result.directional_color,
            result.count,
        ]
        .into_iter()
        .chain(result.positions)
        .chain(result.colors)
        .chain(result.ranges)
        .any(|l| l < 0)
        {
            return Err(Error::Asset(
                "built-in lighting shader is missing required uniforms".into(),
            ));
        }
        Ok(result)
    }
    fn upload(&self, shader: &mut Shader, lights: &Lighting) {
        shader.set_shader_value(self.ambient, v3(lights.ambient));
        let (direction, color) = lights
            .directional
            .map(|l| (l.direction.normalize(), l.color))
            .unwrap_or((
                rayengine_core::glam::Vec3::Z,
                rayengine_core::glam::Vec3::ZERO,
            ));
        shader.set_shader_value(self.direction, v3(direction));
        shader.set_shader_value(self.directional_color, v3(color));
        shader.set_shader_value(self.count, lights.points.len() as i32);
        for (i, light) in lights.points.iter().enumerate() {
            shader.set_shader_value(self.positions[i], v3(light.position));
            shader.set_shader_value(self.colors[i], v3(light.color));
            shader.set_shader_value(self.ranges[i], light.range);
        }
    }
}
