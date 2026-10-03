//! Version-one fixed slot/recipe/pickup/UI workloads; fixtures and clones untimed.
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use rayengine_core::{glam::Vec2, ui::UiInput};
use rayengine_minecraft::{
    breaking,
    gameplay::{Interaction, Player},
    hud::{INVENTORY_REGIONS, Layout, Menu, MenuInput},
    survival::{Inventory, Item, MAX_PICKUPS, Recipe, Survival, SurvivalInput},
    terrain::{Terrain, TerrainSettings},
};
use rayengine_voxel::{glam::DVec3, prelude::*};
use std::{hint::black_box, time::Duration};
fn workloads(c: &mut Criterion) {
    let mut full = Inventory::default();
    full.insert(Item::Dirt, 36 * 64);
    assert_eq!(full.count(Item::Dirt), 2304);
    let mut craft = Inventory::default();
    craft.insert(Item::Planks, 64);
    craft.insert(Item::Stick, 64);
    let mut rejected = full;
    rejected.remove(Item::Dirt, 128);
    rejected.insert(Item::Planks, 64);
    rejected.insert(Item::Stick, 64);
    assert!(!rejected.can_craft(Recipe::WoodenPickaxe));
    assert!(craft.can_craft(Recipe::WoodenPickaxe));
    let mut group = c.benchmark_group("minecraft_survival_v1");
    group.bench_function("insert_full_36", |b| {
        b.iter_batched(
            || black_box(full),
            |mut inv| {
                let left = inv.insert(black_box(Item::Stone), 64);
                black_box((inv, left))
            },
            BatchSize::SmallInput,
        )
    });
    group.bench_function("craft_wooden_pickaxe", |b| {
        b.iter_batched(
            || black_box(craft),
            |mut inv| {
                let result = inv.craft(black_box(Recipe::WoodenPickaxe));
                black_box((inv, result))
            },
            BatchSize::SmallInput,
        )
    });
    group.bench_function("reject_craft_full_36", |b| {
        b.iter_batched(
            || black_box(rejected),
            |mut inv| {
                let result = inv.craft(black_box(Recipe::WoodenPickaxe));
                black_box((inv, result))
            },
            BatchSize::SmallInput,
        )
    });
    let terrain = Terrain::new(42, TerrainSettings::default()).unwrap();
    let blocks = terrain.blocks();
    let mut world = VoxelWorld::new(terrain.registry(), 2);
    for z in [-1, 0] {
        world
            .insert_chunk(
                ChunkPos::new(0, 0, z),
                Chunk::filled(terrain.registry(), BlockId::AIR).unwrap(),
            )
            .unwrap();
    }
    let mut player = Player::new(DVec3::new(0.5, 1.0, 0.5)).unwrap();
    player.controller.set_look(0.0, 0.0).unwrap();
    let target = BlockPos::new(0, 2, -1);
    let mut pickups = Survival::default();
    let mut interaction = Interaction::default();
    for _ in 0..MAX_PICKUPS {
        world.set_block(target, blocks.wood).unwrap();
        let r = pickups
            .interact(
                &mut interaction,
                &mut world,
                &player,
                SurvivalInput {
                    mining: true,
                    dt: 1.1,
                    ..Default::default()
                },
                blocks,
            )
            .unwrap();
        assert!(r.edit.is_some());
    }
    assert_eq!(pickups.pickups().len(), 128);
    let mut check = pickups.clone();
    assert_eq!(check.collect(player.position()), 128);
    assert_eq!(check.inventory.count(Item::Log), 128);
    group.bench_function("collect_128_pickups", |b| {
        b.iter_batched(
            || pickups.clone(),
            |mut s| {
                let count = s.collect(black_box(player.position()));
                black_box((s, count))
            },
            BatchSize::SmallInput,
        )
    });
    let size = Vec2::new(960.0, 540.0);
    let mut menu = Menu::default();
    let mut session = Survival::default();
    session.inventory = craft;
    let input = MenuInput {
        ui: UiInput {
            pointer: Some(Layout::new(size).recipes[0].center()),
            window_focused: true,
            ..Default::default()
        },
        ..Default::default()
    };
    menu.update(
        size,
        MenuInput {
            toggle: true,
            ..input
        },
        &mut session,
    );
    menu.update(size, input, &mut session);
    assert_eq!(menu.state().responses().len(), INVENTORY_REGIONS);
    group.bench_function("inventory_hover_43", |b| {
        b.iter(|| black_box(menu.update(black_box(size), black_box(input), &mut session)))
    });
    let mesh = breaking::mesh(4);
    assert_eq!(mesh.validate().unwrap().vertex_count, 1248);
    group.bench_function("build_cracks_stage_5", |b| {
        b.iter_batched(
            || (),
            |()| black_box(breaking::mesh(black_box(4))),
            BatchSize::LargeInput,
        )
    });
    group.finish();
}
criterion_group! { name=benches; config=Criterion::default().warm_up_time(Duration::from_millis(500)).measurement_time(Duration::from_secs(2)); targets=workloads }
criterion_main!(benches);
