//! Compare generated/imported unlit and lit surfaces. Run `--hidden --frames 30` for a probe.
use rayengine::{prelude::*, raylib::prelude::Image};
use std::{fmt::Write, path::PathBuf};

const LEFT: Action = Action(0);
const RIGHT: Action = Action(1);
const AMBIENT_UP: Action = Action(2);
const AMBIENT_DOWN: Action = Action(3);
const RANGE_UP: Action = Action(4);
const RANGE_DOWN: Action = Action(5);
const TOGGLE_SUN: Action = Action(6);
const TOGGLE_POINT: Action = Action(7);

struct Demo {
    directory: PathBuf,
    mesh: Option<MeshId>,
    model: Option<ModelId>,
    unlit: Option<MaterialId>,
    lit: Option<MaterialId>,
    angle: f32,
    ambient: f32,
    range: f32,
    sun: bool,
    point: bool,
}
impl Default for Demo {
    fn default() -> Self {
        Self {
            directory: std::env::temp_dir()
                .join(format!("rayengine-lighting-demo-{}", std::process::id())),
            mesh: None,
            model: None,
            unlit: None,
            lit: None,
            angle: 0.6,
            ambient: 0.15,
            range: 6.0,
            sun: true,
            point: true,
        }
    }
}
impl Drop for Demo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
fn octahedron() -> MeshData {
    let mut mesh = MeshData {
        normals: Some(Vec::new()),
        texcoords: Some(Vec::new()),
        ..MeshData::default()
    };
    let ring = [Vec3::X, Vec3::Z, -Vec3::X, -Vec3::Z];
    for top in [true, false] {
        for i in 0..4 {
            let tip = if top { Vec3::Y } else { -Vec3::Y };
            let (a, b) = if top {
                (ring[(i + 1) % 4], ring[i])
            } else {
                (ring[i], ring[(i + 1) % 4])
            };
            let normal = (a - tip).cross(b - tip).normalize();
            mesh.positions.extend([tip, a, b]);
            mesh.normals.as_mut().unwrap().extend([normal; 3]);
            mesh.texcoords.as_mut().unwrap().extend([
                Vec2::new(0.5, 0.0),
                Vec2::new(0.0, 1.0),
                Vec2::ONE,
            ]);
        }
    }
    mesh
}
impl Game for Demo {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(LEFT, KeyboardKey::KEY_LEFT)
            .bind(RIGHT, KeyboardKey::KEY_RIGHT)
            .bind(AMBIENT_UP, KeyboardKey::KEY_A)
            .bind(AMBIENT_DOWN, KeyboardKey::KEY_Z)
            .bind(RANGE_UP, KeyboardKey::KEY_R)
            .bind(RANGE_DOWN, KeyboardKey::KEY_F)
            .bind(TOGGLE_SUN, KeyboardKey::KEY_D)
            .bind(TOGGLE_POINT, KeyboardKey::KEY_P)
    }
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        std::fs::create_dir(&self.directory)?;
        let data = octahedron();
        self.mesh = Some(ctx.mesh(&data)?);
        // Import the same fixture through OBJ, so columns differ only by shading/path.
        let mut obj = String::new();
        for p in &data.positions {
            writeln!(obj, "v {} {} {}", p.x, p.y, p.z).unwrap();
        }
        for n in data.normals.as_ref().unwrap() {
            writeln!(obj, "vn {} {} {}", n.x, n.y, n.z).unwrap();
        }
        for uv in data.texcoords.as_ref().unwrap() {
            writeln!(obj, "vt {} {}", uv.x, uv.y).unwrap();
        }
        for i in (1..=data.positions.len()).step_by(3) {
            writeln!(
                obj,
                "f {i}/{i}/{i} {}/{}/{} {}/{}/{}",
                i + 1,
                i + 1,
                i + 1,
                i + 2,
                i + 2,
                i + 2
            )
            .unwrap();
        }
        std::fs::write(self.directory.join("surface.obj"), obj)?;
        self.model = Some(ctx.model(self.directory.join("surface.obj"))?);
        let mut image = Image::gen_image_color(32, 32, Color::new(230, 180, 100, 255));
        image.draw_rectangle(0, 0, 16, 16, Color::new(140, 190, 230, 255));
        image.draw_rectangle(16, 16, 16, 16, Color::new(140, 190, 230, 255));
        let png = image
            .export_image_to_memory(".png")
            .map_err(|e| Error::Asset(e.to_string()))?;
        std::fs::write(self.directory.join("albedo.png"), &*png)?;
        let texture = ctx.texture(self.directory.join("albedo.png"))?;
        self.unlit = Some(ctx.material(MaterialDesc {
            texture: Some(texture),
            ..MaterialDesc::default()
        })?);
        self.lit = Some(ctx.material(MaterialDesc {
            texture: Some(texture),
            shading: Shading::Lit,
            ..MaterialDesc::default()
        })?);
        Ok(())
    }
    fn fixed_update(&mut self, update: &mut Update<'_, '_>) {
        let axis = |up, down| f32::from(update.input.down(up)) - f32::from(update.input.down(down));
        self.angle += axis(RIGHT, LEFT) * update.tick.dt;
        self.ambient =
            (self.ambient + axis(AMBIENT_UP, AMBIENT_DOWN) * update.tick.dt * 0.4).clamp(0.0, 1.0);
        self.range =
            (self.range + axis(RANGE_UP, RANGE_DOWN) * update.tick.dt * 2.0).clamp(0.1, 20.0);
        if update.input.pressed(TOGGLE_SUN) {
            self.sun = !self.sun;
        }
        if update.input.pressed(TOGGLE_POINT) {
            self.point = !self.point;
        }
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame
            .assets
            .set_lighting(Lighting {
                ambient: Vec3::splat(self.ambient),
                directional: self.sun.then_some(DirectionalLight {
                    direction: Vec3::new(self.angle.sin(), -0.6, -self.angle.cos()),
                    color: Vec3::splat(0.65),
                }),
                points: if self.point {
                    vec![PointLight {
                        position: Vec3::new(0.0, 1.5, 2.0),
                        color: Vec3::new(0.2, 0.4, 1.0),
                        range: self.range,
                    }]
                } else {
                    vec![]
                },
            })
            .expect("controls stay in valid ranges");
        frame.clear(Color::new(24, 28, 38, 255));
        frame.world_3d(
            Camera3D {
                position: Vec3::new(0.0, 2.5, 10.0),
                target: Vec3::ZERO,
                ..Camera3D::default()
            },
            |c| {
                for (i, imported, lit) in [
                    (0, false, false),
                    (1, false, true),
                    (2, true, false),
                    (3, true, true),
                ] {
                    let transform = Transform3D {
                        position: Vec3::new(i as f32 * 2.4 - 3.6, 0.0, 0.0),
                        rotation: Quat::from_rotation_y(0.35),
                        scale: Vec3::new(0.85, 1.2, 0.7),
                    };
                    let material = if lit {
                        self.lit.unwrap()
                    } else {
                        self.unlit.unwrap()
                    };
                    assert!(if imported {
                        c.try_model_material(self.model.unwrap(), material, transform, Color::WHITE)
                            .unwrap()
                    } else {
                        c.try_mesh_material(self.mesh.unwrap(), material, transform, Color::WHITE)
                            .unwrap()
                    });
                }
            },
        );
        frame.ui(|ui| {
            ui.text(
                "Generated: unlit / lit       Imported OBJ: unlit / lit",
                Vec2::new(24.0, 24.0),
                24.0,
                Color::WHITE,
            );
            ui.text(
                "Left/Right: sun direction  A/Z: ambient  R/F: point range  D/P: toggle lights",
                Vec2::new(24.0, 60.0),
                18.0,
                Color::LIGHTGRAY,
            );
            ui.text(
                &format!(
                    "Ambient {:.2}   Point range {:.1}   Sun {}   Point {}",
                    self.ambient, self.range, self.sun, self.point
                ),
                Vec2::new(24.0, 88.0),
                18.0,
                Color::LIGHTGRAY,
            );
        });
    }
}
fn main() -> Result<(), Error> {
    App::new(Config::new("rayengine / Basic lighting"))
        .with_options(RunOptions::from_env()?)
        .run(Demo::default())?;
    Ok(())
}
