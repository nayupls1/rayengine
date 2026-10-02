use super::*;
use rayengine_core::{
    camera,
    collision::{Aabb2, Aabb3},
    glam::Vec3,
    transform::Transform3D,
};

#[test]
fn diagnostic_arguments_validate_before_opening_a_window() {
    let parse = |args: &[&str]| RunOptions::parse(args.iter().map(|s| s.to_string()));
    let options = parse(&["--diagnostics", "report.json", "--workload", "mesh/100.v1"]).unwrap();
    let config = options.diagnostics.unwrap();
    assert_eq!(config.workload, "mesh/100.v1");
    assert_eq!(config.output, Some(PathBuf::from("report.json")));
    assert!(parse(&["--workload", "has spaces"]).is_err());
    assert!(parse(&["--diagnostics"]).is_err());
    assert!(parse(&["--workload"]).is_err());
    struct Empty;
    impl Game for Empty {
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
        fn draw(&mut self, _: &mut Frame<'_, '_>) {}
    }
    assert!(
        App::new(Config::new("bad diagnostics"))
            .with_options(RunOptions {
                diagnostics: Some(DiagnosticsConfig::new("")),
                ..Default::default()
            })
            .run(Empty)
            .is_err()
    );
    assert_eq!(Assets::new(None).resource_counts(), Default::default());
}

#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_diagnostics_counts_resources_replacements_and_json_schema() {
    let directory =
        std::env::temp_dir().join(format!("rayengine-diagnostics-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let png = Image::gen_image_color(8, 8, Color::WHITE)
        .export_image_to_memory(".png")
        .unwrap();
    std::fs::write(directory.join("white.png"), &*png).unwrap();
    std::fs::write(
        directory.join("triangle.obj"),
        "v -1 0 0\nv 1 0 0\nv 0 2 0\nf 1 2 3\n",
    )
    .unwrap();
    struct Probe {
        directory: PathBuf,
        mesh: Option<MeshId>,
        texture: Option<TextureId>,
        model: Option<ModelId>,
        material: Option<MaterialId>,
    }
    impl Game for Probe {
        fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
            self.mesh = Some(ctx.mesh(&MeshData::new(vec![Vec3::ZERO, Vec3::X, Vec3::Y]))?);
            self.texture = Some(ctx.texture(self.directory.join("white.png"))?);
            self.model = Some(ctx.model(self.directory.join("triangle.obj"))?);
            self.material = Some(ctx.material(MaterialDesc::default())?);
            let counts = ctx.assets.resource_counts();
            assert_eq!(counts.generated_mesh_bytes, 60); // positions + fallback UVs
            assert_eq!(counts.texture_bytes, 256);
            assert_eq!(
                (
                    counts.meshes,
                    counts.models,
                    counts.materials,
                    counts.shaders
                ),
                (1, 1, 1, 2)
            );
            Ok(())
        }
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
        fn draw(&mut self, frame: &mut Frame<'_, '_>) {
            let mesh = self.mesh.unwrap();
            if frame.index == 1 {
                frame.assets.unload_texture(self.texture.unwrap());
                frame
                    .replace_mesh(
                        mesh,
                        &MeshData::new(vec![
                            Vec3::ZERO,
                            Vec3::X,
                            Vec3::Y,
                            Vec3::ZERO,
                            Vec3::X,
                            Vec3::Y,
                        ]),
                    )
                    .unwrap();
                assert_eq!(frame.assets.resource_counts().generated_mesh_bytes, 120);
                assert!(frame.replace_mesh(mesh, &MeshData::default()).is_err());
                assert_eq!(frame.assets.resource_counts().generated_mesh_bytes, 120);
            } else if frame.index == 2 {
                assert!(frame.assets.unload_mesh(mesh));
            }
            let index = frame.index;
            frame.clear(Color::BLACK);
            frame.world_2d(camera::Camera2D::default(), |canvas| {
                canvas.rectangle(Aabb2::from_center(Vec2::ZERO, Vec2::ONE), Color::WHITE);
                canvas.circle(Vec2::ZERO, 1.0, Color::WHITE);
                canvas.line(Vec2::ZERO, Vec2::ONE, 1.0, Color::WHITE);
                assert_eq!(
                    canvas.texture(
                        self.texture.unwrap(),
                        Aabb2::from_center(Vec2::ZERO, Vec2::ONE),
                        Color::WHITE
                    ),
                    index == 0
                );
            });
            frame.world_3d(camera::Camera3D::default(), |canvas| {
                canvas.cube(Aabb3::from_center(Vec3::ZERO, Vec3::ONE), Color::WHITE);
                canvas.wire_cube(Aabb3::from_center(Vec3::ZERO, Vec3::ONE), Color::WHITE);
                canvas.sphere(Vec3::ZERO, 1.0, Color::WHITE);
                canvas.line(Vec3::ZERO, Vec3::ONE, Color::WHITE);
                assert_eq!(
                    canvas.mesh(mesh, Transform3D::default(), Color::WHITE),
                    index < 2
                );
                assert_eq!(
                    canvas.mesh_material(
                        mesh,
                        self.material.unwrap(),
                        Transform3D::default(),
                        Color::WHITE
                    ),
                    index < 2
                );
                assert!(canvas.model(self.model.unwrap(), Vec3::ZERO, 1.0, Color::WHITE));
                assert!(canvas.model_material(
                    self.model.unwrap(),
                    self.material.unwrap(),
                    Transform3D::default(),
                    Color::WHITE
                ));
            });
            frame.ui(|ui| {
                ui.rectangle(Aabb2::from_center(Vec2::ONE, Vec2::ONE), Color::WHITE);
                ui.circle(Vec2::ONE, 1.0, Color::WHITE);
                ui.text("test", Vec2::ZERO, 10.0, Color::WHITE);
                assert_eq!(
                    ui.icon(
                        self.texture.unwrap(),
                        Aabb2::from_center(Vec2::ONE, Vec2::ONE),
                        Color::WHITE
                    ),
                    index == 0
                );
            });
            frame.with_raylib(|_| {});
        }
    }
    let output = directory.join("report.json");
    let mut diagnostic = DiagnosticsConfig::new("probe/mixed-3.v1");
    diagnostic.output = Some(output.clone());
    let report = App::new(Config::new("diagnostics probe"))
        .with_options(RunOptions {
            frames: Some(3),
            size: Some((64, 64)),
            hidden: true,
            uncapped: true,
            diagnostics: Some(diagnostic),
            ..Default::default()
        })
        .run(Probe {
            directory: directory.clone(),
            mesh: None,
            texture: None,
            model: None,
            material: None,
        })
        .unwrap();
    let metrics = report.diagnostics.unwrap();
    assert_eq!(metrics.frames, report.frames);
    assert_eq!(metrics.updates, report.ticks);
    assert_eq!(metrics.update.samples, report.ticks);
    assert_eq!(
        (
            metrics.frame.samples,
            metrics.render.samples,
            metrics.present.samples
        ),
        (3, 3, 3)
    );
    assert_eq!(
        metrics.draws,
        DrawCounters {
            clears: 6,
            world_2d_passes: 3,
            world_3d_passes: 3,
            ui_passes: 3,
            raw_passes: 3,
            primitives_2d: 9,
            primitives_3d: 12,
            meshes: 10,
            models: 6,
            textures: 2,
            ui_primitives: 6,
            text: 3,
        }
    );
    assert_eq!(metrics.resources.meshes, 0);
    assert_eq!(metrics.resources.generated_mesh_bytes, 0);
    assert_eq!(metrics.peak_resources.meshes, 1);
    assert_eq!(metrics.peak_resources.generated_mesh_bytes, 120);
    assert_eq!(metrics.peak_resources.texture_bytes, 256);
    assert!(metrics.resources.model_geometry_bytes > 0);
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["workload"], "probe/mixed-3.v1");
    assert_eq!(json["draws"]["meshes"], 10);
    assert_eq!(json["settings"]["target_fps"], 0);
    assert_eq!(json["settings"]["vsync"], false);
    assert!(
        metrics.frame.total_ns
            >= metrics.render.total_ns + metrics.present.total_ns + metrics.update.total_ns
    );
    std::fs::remove_dir_all(directory).unwrap();
}
