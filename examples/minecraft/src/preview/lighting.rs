//! Demo-owned world lighting. Terrain uses a voxel shader that keeps the plugin's
//! per-tile UV repetition and adds sun/moon Lambert light, sky ambient, a held
//! torch and distance fog. Pickups use the SDK's built-in lit material with the
//! same sun and torch, so every lit surface agrees.
use crate::sky::SkyLight;
use rayengine::{assets::Assets, prelude::*};
use rayengine_voxel::{
    glam::Vec4,
    prelude::{MeshLayer, SurfaceKey, TileTexture, VoxelMaterials},
};

const VERTEX: &str = r#"#version 330
in vec3 vertexPosition;
in vec2 vertexTexCoord;
in vec3 vertexNormal;
in vec4 vertexColor;
uniform mat4 mvp;
uniform mat4 matModel;
out vec2 fragTexCoord;
out vec4 fragColor;
out vec3 worldPosition;
out vec3 worldNormal;
void main() {
    fragTexCoord = vertexTexCoord;
    fragColor = vertexColor;
    worldPosition = vec3(matModel * vec4(vertexPosition, 1.0));
    // Chunk transforms are translations, so model-space normals are world normals.
    worldNormal = vertexNormal;
    gl_Position = mvp * vec4(vertexPosition, 1.0);
}
"#;
const FRAGMENT: &str = r#"#version 330
in vec2 fragTexCoord;
in vec4 fragColor;
in vec3 worldPosition;
in vec3 worldNormal;
uniform sampler2D texture0;
uniform vec4 colDiffuse;
uniform vec4 voxelTileRect;
uniform int rayengineAlphaMode;
uniform float rayengineAlphaCutoff;
uniform vec3 sunDirection;
uniform vec3 sunColor;
uniform vec3 skyAmbient;
uniform vec3 fogColor;
uniform vec2 fogRange;
uniform vec3 viewPosition;
uniform vec3 torchPosition;
uniform vec3 torchColor;
uniform float torchRange;
out vec4 finalColor;
void main() {
    vec2 uv = voxelTileRect.xy + fract(fragTexCoord) * voxelTileRect.zw;
    vec4 albedo = textureGrad(texture0, uv,
        dFdx(fragTexCoord) * voxelTileRect.zw,
        dFdy(fragTexCoord) * voxelTileRect.zw) * colDiffuse;
    if (rayengineAlphaMode == 1 && albedo.a < rayengineAlphaCutoff) discard;
    vec3 normal = normalize(worldNormal);
    // The mesher bakes classic per-face shade into the vertex color; it scales
    // sky light so caves and overhangs still read as volumes at noon.
    float face = fragColor.r;
    vec3 light = skyAmbient * face + sunColor * max(dot(normal, -sunDirection), 0.0);
    vec3 toTorch = torchPosition - worldPosition;
    float distance = length(toTorch);
    float falloff = max(1.0 - distance / torchRange, 0.0);
    float facing = 0.35 + 0.65 * max(dot(normal, toTorch / max(distance, 0.0001)), 0.0);
    light += torchColor * falloff * falloff * facing;
    vec3 color = albedo.rgb * min(light, vec3(1.25));
    float fog = smoothstep(fogRange.x, fogRange.y, length(worldPosition - viewPosition));
    finalColor = vec4(mix(color, fogColor, fog), 1.0);
}
"#;
/// Warm torch irradiance at its center.
const TORCH: Vec3 = Vec3::new(1.5, 1.02, 0.55);

/// Per-frame inputs, in camera-relative render space.
pub(super) struct FrameLight {
    pub sky: SkyLight,
    pub view: Vec3,
    pub torch: Option<(Vec3, f32)>,
    /// Fog start/end distance from the camera.
    pub fog: Vec2,
}
struct Uniforms {
    sun_direction: UniformId,
    sun_color: UniformId,
    ambient: UniformId,
    fog_color: UniformId,
    fog_range: UniformId,
    view: UniformId,
    torch_position: UniformId,
    torch_color: UniformId,
    torch_range: UniformId,
}
/// Owned terrain shader/materials and lit pickup materials. Textures are borrowed.
#[derive(Default)]
pub(super) struct WorldLighting {
    shader: Option<ShaderId>,
    uniforms: Option<Uniforms>,
    materials: Vec<MaterialId>,
    /// Lit pickup material per item, in Item::ALL order.
    pub items: Vec<MaterialId>,
    /// Shared unit cube with normals and per-face UVs.
    pub cube: Option<MeshId>,
}
impl WorldLighting {
    /// Builds terrain materials bound into a fresh voxel lookup. On failure,
    /// everything created here is released before returning.
    pub fn create(
        &mut self,
        ctx: &mut InitContext<'_, '_>,
        tiles: &[TileTexture],
        icons: &[TextureId],
    ) -> Result<VoxelMaterials, Error> {
        let result = self.build(ctx, tiles, icons);
        if result.is_err() {
            self.unload(ctx.assets);
        }
        result
    }
    fn build(
        &mut self,
        ctx: &mut InitContext<'_, '_>,
        tiles: &[TileTexture],
        icons: &[TextureId],
    ) -> Result<VoxelMaterials, Error> {
        let shader = ctx.shader_from_source(Some(VERTEX), FRAGMENT)?;
        self.shader = Some(shader);
        let mut uniform = |name: &str, value| ctx.uniform(shader, name, value);
        let v3 = |v: Vec3| UniformValue::Vec3(v);
        self.uniforms = Some(Uniforms {
            sun_direction: uniform("sunDirection", v3(Vec3::NEG_Y))?,
            sun_color: uniform("sunColor", v3(Vec3::ONE))?,
            ambient: uniform("skyAmbient", v3(Vec3::splat(0.5)))?,
            fog_color: uniform("fogColor", v3(Vec3::ONE))?,
            fog_range: uniform("fogRange", UniformValue::Vec2(Vec2::new(1e4, 2e4)))?,
            view: uniform("viewPosition", v3(Vec3::ZERO))?,
            torch_position: uniform("torchPosition", v3(Vec3::ZERO))?,
            torch_color: uniform("torchColor", v3(Vec3::ZERO))?,
            torch_range: uniform("torchRange", UniformValue::Float(1.0))?,
        });
        let rect = ctx.uniform(
            shader,
            "voxelTileRect",
            UniformValue::Vec4(Vec4::new(0.0, 0.0, 1.0, 1.0)),
        )?;
        let mut voxel = VoxelMaterials::new();
        for tile in tiles {
            for (layer, alpha) in [
                (MeshLayer::Opaque, AlphaMode::Opaque),
                (MeshLayer::Cutout, AlphaMode::Cutout(0.5)),
            ] {
                let material = ctx.material(MaterialDesc {
                    texture: Some(tile.texture),
                    shader: Some(shader),
                    alpha,
                    parameters: vec![MaterialParam {
                        uniform: rect,
                        value: UniformValue::Vec4(tile.rect),
                    }],
                    ..Default::default()
                })?;
                self.materials.push(material);
                voxel.bind(
                    SurfaceKey {
                        tile: tile.tile,
                        layer,
                    },
                    material,
                )?;
            }
        }
        for &icon in icons {
            let material = ctx.material(MaterialDesc {
                texture: Some(icon),
                shading: Shading::Lit,
                alpha: AlphaMode::Cutout(0.5),
                ..Default::default()
            })?;
            self.items.push(material);
        }
        self.cube = Some(ctx.mesh(&cube())?);
        Ok(voxel)
    }
    /// Upload this frame's sky, fog and torch values before the 3D pass.
    pub fn apply(&self, assets: &mut Assets<'_>, light: &FrameLight) -> Result<(), Error> {
        let Some(u) = &self.uniforms else {
            return Ok(());
        };
        let (torch_position, torch_color, torch_range) = match light.torch {
            Some((position, range)) => (position, TORCH, range),
            None => (Vec3::ZERO, Vec3::ZERO, 1.0),
        };
        for (binding, value) in [
            (u.sun_direction, UniformValue::Vec3(light.sky.direction)),
            (u.sun_color, UniformValue::Vec3(light.sky.color)),
            (u.ambient, UniformValue::Vec3(light.sky.ambient)),
            (u.fog_color, UniformValue::Vec3(light.sky.sky)),
            (u.fog_range, UniformValue::Vec2(light.fog)),
            (u.view, UniformValue::Vec3(light.view)),
            (u.torch_position, UniformValue::Vec3(torch_position)),
            (u.torch_color, UniformValue::Vec3(torch_color)),
            (u.torch_range, UniformValue::Float(torch_range)),
        ] {
            assets.set_uniform(binding, value)?;
        }
        assets.set_lighting(Lighting {
            ambient: light.sky.ambient,
            directional: Some(DirectionalLight {
                direction: light.sky.direction,
                color: light.sky.color,
            }),
            points: light
                .torch
                .map(|(position, range)| PointLight {
                    position,
                    color: TORCH,
                    range,
                })
                .into_iter()
                .collect(),
        })
    }
    /// Release owned materials, mesh and shader; borrowed textures stay loaded.
    pub fn unload(&mut self, assets: &mut Assets<'_>) {
        for id in self.materials.drain(..).chain(self.items.drain(..)) {
            assets.unload_material(id);
        }
        if let Some(mesh) = self.cube.take() {
            assets.unload_mesh(mesh);
        }
        if let Some(shader) = self.shader.take() {
            assets.unload_shader(shader);
        }
        self.uniforms = None;
    }
}
/// Unit cube centered on the origin, counter-clockwise outward faces.
fn cube() -> MeshData {
    let mut mesh = MeshData {
        normals: Some(Vec::with_capacity(24)),
        texcoords: Some(Vec::with_capacity(24)),
        indices: Some(Vec::with_capacity(36)),
        ..MeshData::default()
    };
    // (normal, u, v) with u × v = normal.
    for (n, u, v) in [
        (Vec3::X, Vec3::NEG_Z, Vec3::Y),
        (Vec3::NEG_X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::X, Vec3::NEG_Z),
        (Vec3::NEG_Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
    ] {
        let base = mesh.positions.len() as u16;
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            mesh.positions.push((n + u * su + v * sv) * 0.5);
            mesh.normals.as_mut().unwrap().push(n);
            mesh.texcoords
                .as_mut()
                .unwrap()
                .push(Vec2::new((su + 1.0) * 0.5, (1.0 - sv) * 0.5));
        }
        mesh.indices
            .as_mut()
            .unwrap()
            .extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_faces_wind_outward_with_unit_normals() {
        let mesh = cube();
        assert_eq!(mesh.validate().unwrap().vertex_count, 24);
        let normals = mesh.normals.as_ref().unwrap();
        for tri in mesh.indices.as_ref().unwrap().chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| mesh.positions[tri[i] as usize]);
            let n = normals[tri[0] as usize];
            assert!((b - a).cross(c - a).normalize().dot(n) > 0.99);
        }
    }
}
