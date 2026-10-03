//! Opt-in cached crack submissions and 43-region inventory drawing. Setup untimed.
use criterion::Criterion;
use rayengine::prelude::*;
use rayengine_minecraft::{
    breaking,
    hud::{INVENTORY_REGIONS, Layout, Menu, MenuInput, recipe_id, slot_id},
    survival::{Item, Recipe, Survival},
};
use rayengine_voxel::glam::Mat4;
use std::{hint::black_box, time::Duration};
struct Bench {
    criterion: Option<Criterion>,
    cracks: Vec<MeshId>,
    menu: Menu,
    survival: Survival,
}
impl Game for Bench {
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        for stage in 0..breaking::STAGES {
            self.cracks.push(ctx.mesh(&breaking::mesh(stage))?);
        }
        assert_eq!(ctx.assets.resource_counts().meshes, 5);
        assert_eq!(ctx.assets.resource_counts().generated_mesh_bytes, 79_488);
        eprintln!(
            "minecraft_survival_render_v1: stages=5 crack_bytes=79488 slots=36 regions=43 logical_view=960x960 target=64x64"
        );
        Ok(())
    }
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        let mut c = self.criterion.take().unwrap();
        let mut g = c.benchmark_group("minecraft_survival_render_v1");
        frame.clear(Color::BLACK);
        frame.world_3d(
            Camera3D {
                position: Vec3::new(2.0, 2.0, 3.0),
                target: Vec3::splat(0.5),
                ..Default::default()
            },
            |canvas| {
                g.bench_function("draw_cracks_stage_5_16", |b| {
                    b.iter(|| {
                        for _ in 0..16 {
                            assert!(canvas.mesh_matrix(
                                black_box(self.cracks[4]),
                                Mat4::IDENTITY,
                                Color::BLACK
                            ));
                        }
                    })
                });
            },
        );
        let layout = Layout::new(Vec2::splat(960.0));
        frame.ui(|ui| {
            g.bench_function("draw_inventory_43", |b| {
                b.iter(|| {
                    ui.rectangle(layout.panel, Color::new(19, 25, 33, 255));
                    for (i, &bounds) in layout.slots.iter().enumerate() {
                        let stack = self.survival.inventory.slots()[i].unwrap();
                        let response = self.menu.state().response(slot_id(i)).unwrap();
                        ui.button(
                            bounds,
                            black_box(stack.item().label()),
                            response,
                            UiButtonStyle {
                                font_size: 11.0,
                                ..Default::default()
                            },
                        );
                        ui.text(
                            black_box("64"),
                            bounds.min + Vec2::splat(4.0),
                            10.0,
                            Color::WHITE,
                        );
                    }
                    for (i, &bounds) in layout.recipes.iter().enumerate() {
                        ui.button(
                            bounds,
                            black_box(Recipe::ALL[i].label()),
                            self.menu.state().response(recipe_id(i)).unwrap(),
                            UiButtonStyle {
                                font_size: 13.0,
                                ..Default::default()
                            },
                        );
                    }
                })
            });
        });
        g.finish();
        for mesh in self.cracks.drain(..) {
            frame.assets.unload_mesh(mesh);
        }
        assert_eq!(frame.assets.resource_counts().meshes, 0);
        c.final_summary();
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("RAYENGINE_RENDER_BENCH").as_deref() != Ok("1") {
        eprintln!("Use scripts/render_benchmark.sh save NAME minecraft_survival_render_v1");
        return Ok(());
    }
    let mut survival = Survival::default();
    survival.inventory.insert(Item::Dirt, 36 * 64);
    let mut menu = Menu::default();
    menu.update(
        Vec2::splat(960.0),
        MenuInput {
            toggle: true,
            ui: UiInput {
                window_focused: true,
                ..Default::default()
            },
            ..Default::default()
        },
        &mut survival,
    );
    assert_eq!(menu.state().responses().len(), INVENTORY_REGIONS);
    let mut config = Config::new("Survival render benchmark");
    config.audio = false;
    config.vsync = false;
    config.window_size = (64, 64);
    config.reference_size = Vec2::splat(960.0);
    App::new(config)
        .with_options(RunOptions {
            hidden: true,
            uncapped: true,
            frames: Some(1),
            ..Default::default()
        })
        .run(Bench {
            criterion: Some(
                Criterion::default()
                    .sample_size(30)
                    .warm_up_time(Duration::from_millis(500))
                    .measurement_time(Duration::from_secs(2))
                    .configure_from_args(),
            ),
            cracks: Vec::with_capacity(5),
            menu,
            survival,
        })?;
    Ok(())
}
