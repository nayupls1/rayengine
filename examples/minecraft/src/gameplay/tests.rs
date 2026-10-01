use super::*;
use std::sync::Arc;
fn fixture() -> (VoxelWorld, BlockId, Player) {
    let mut registry = BlockRegistry::new();
    let stone = registry.register(BlockDef::new("stone")).unwrap();
    let registry = Arc::new(registry);
    let mut world = VoxelWorld::new(registry.clone(), 32);
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                world
                    .insert_chunk(
                        ChunkPos::new(x, y, z),
                        Chunk::filled(registry.clone(), BlockId::AIR).unwrap(),
                    )
                    .unwrap();
            }
        }
    }
    let mut player = Player::new(DVec3::new(-0.5, 1.0, 0.5)).unwrap();
    player.controller.set_look(0.0, 0.0).unwrap();
    (world, stone, player)
}
#[test]
fn selection_reach_negative_coordinates_and_unloaded_stop() {
    let (mut world, stone, player) = fixture();
    let eye = as_global(player.origin) + player.controller.camera(1.0).position.as_dvec3();
    let p = BlockPos::new(-1, 2, -5);
    world.set_block(p, stone).unwrap();
    let RaycastOutcome::Hit(hit) = select(&world, eye, -DVec3::Z).unwrap().outcome else {
        panic!("no hit");
    };
    assert_eq!(hit.position, p);
    assert_eq!(hit.adjacent, Some(BlockPos::new(-1, 2, -4)));
    assert_eq!(hit.distance, 4.5);
    world.set_block(p, BlockId::AIR).unwrap();
    world.set_block(BlockPos::new(-1, 2, -6), stone).unwrap();
    assert_eq!(
        select(&world, eye, -DVec3::Z).unwrap().outcome,
        RaycastOutcome::Miss
    );
    world.remove_chunk(ChunkPos::new(-1, 0, -1));
    assert!(matches!(
        player.selection(&world).unwrap().outcome,
        RaycastOutcome::Unloaded { .. }
    ));
}
#[test]
fn placement_checks_body_loaded_air_registry_and_touching_feet() {
    let (mut world, stone, player) = fixture();
    for y in 1..=2 {
        assert!(!can_place(&world, BlockPos::new(-1, y, 0), stone, &player));
    }
    let support = BlockPos::new(-1, 0, 0);
    assert!(can_place(&world, support, stone, &player));
    world.set_block(support, stone).unwrap();
    assert!(!can_place(&world, support, stone, &player));
    assert!(!can_place(&world, BlockPos::new(100, 0, 0), stone, &player));
    assert!(!can_place(
        &world,
        BlockPos::new(-1, 3, 0),
        BlockId::from_raw(100),
        &player
    ));
    assert!(!can_place(
        &world,
        BlockPos::new(-1, 3, 0),
        BlockId::AIR,
        &player
    ));
}
#[test]
fn mining_is_timed_resets_on_release_change_and_reinstallation() {
    let (mut world, stone, mut player) = fixture();
    let p = BlockPos::new(-1, 2, -2);
    world.set_block(p, stone).unwrap();
    let mut interaction = Interaction::default();
    let mut tick = |world: &mut VoxelWorld, player: &Player, held: bool, dt: f32| {
        interaction
            .step(world, player, held, false, dt, stone)
            .unwrap()
    };
    assert_eq!(tick(&mut world, &player, true, 0.6).progress, 0.6);
    assert_eq!(world.block(p), Some(stone));
    tick(&mut world, &player, false, 0.0);
    assert_eq!(tick(&mut world, &player, true, 0.6).progress, 0.6);
    let pos = p.split().0;
    let chunk = world.remove_chunk(pos).unwrap();
    world.insert_chunk(pos, chunk).unwrap();
    assert_eq!(tick(&mut world, &player, true, 0.6).progress, 0.6);
    player
        .controller
        .set_look(std::f32::consts::PI, 0.0)
        .unwrap();
    assert!(tick(&mut world, &player, true, 0.6).selected.is_none());
    player.controller.set_look(0.0, 0.0).unwrap();
    tick(&mut world, &player, true, 0.6);
    let report = tick(&mut world, &player, true, 0.5);
    assert_eq!(report.edit.unwrap().previous, stone);
    assert_eq!(world.block(p), Some(BlockId::AIR));
}
#[test]
fn unbreakable_blocks_and_cross_chunk_placement() {
    let (mut world, stone, mut player) = fixture();
    // Feet/player at z=0.5, target across z=0 into the negative chunk.
    let target = BlockPos::new(-1, 2, -1);
    world.set_block(target, stone).unwrap();
    let mut interaction = Interaction::default();
    // Adjacent cell contains the player's head; actual right-click must reject.
    assert!(
        interaction
            .step(&mut world, &player, false, true, 0.01, stone)
            .unwrap()
            .edit
            .is_none()
    );
    player
        .controller
        .teleport(player.controller.body.position + Vec3::Z * 2.0)
        .unwrap();
    let report = interaction
        .step(&mut world, &player, false, true, 0.01, stone)
        .unwrap();
    assert_eq!(report.edit.unwrap().position, BlockPos::new(-1, 2, 0));
    assert_eq!(world.block(BlockPos::new(-1, 2, 0)), Some(stone));
    let mut registry = BlockRegistry::new();
    let mut def = BlockDef::new("bedrock");
    def.hardness = None;
    let bedrock = registry.register(def).unwrap();
    let registry = Arc::new(registry);
    let mut world = VoxelWorld::new(registry.clone(), 1);
    world
        .insert_chunk(
            ChunkPos::new(-1, 0, 0),
            Chunk::filled(registry, bedrock).unwrap(),
        )
        .unwrap();
    let player = Player::new(DVec3::new(-0.5, 1.0, 0.5)).unwrap();
    let report = interaction
        .step(&mut world, &player, true, false, 100.0, bedrock)
        .unwrap();
    assert_eq!(report.progress, 0.0);
    assert!(report.edit.is_none());
}
#[test]
fn missing_terrain_pauses_gravity_and_rebase_preserves_camera_velocity() {
    let (mut world, stone, mut player) = fixture();
    for x in -5..=5 {
        for z in -5..=5 {
            world.set_block(BlockPos::new(x, 0, z), stone).unwrap();
        }
    }
    let mut crossed = false;
    for _ in 0..50 {
        player
            .step(
                &world,
                FirstPersonInput {
                    movement: rayengine_voxel::glam::Vec2::X,
                    ..Default::default()
                },
                1.0 / 60.0,
            )
            .unwrap();
        if player.origin.x == 0 {
            crossed = true;
        }
        assert!((player.position().y - 1.9).abs() < 0.001);
    }
    assert!(crossed);
    assert!(player.controller.body.grounded);
    assert!(player.controller.body.velocity.x > 4.0);
    let before = player.position();
    let velocity = player.controller.body.velocity;
    world.remove_chunk(player.focus());
    assert!(matches!(
        player.step(&world, FirstPersonInput::default(), 1.0 / 60.0),
        Err(ColliderError::Unloaded(_))
    ));
    assert_eq!(player.position(), before);
    assert_eq!(player.controller.body.velocity, velocity);
}

#[test]
fn far_origin_movement_preserves_global_selection_and_interpolation() {
    let (base, stone, _) = fixture();
    let mut world = VoxelWorld::new(base.shared_registry(), 4);
    let edge = i32::MAX - 31; // Integer chunk origin, with one further chunk available.
    for x in [edge, edge + 16] {
        world
            .insert_chunk(
                BlockPos::new(x, 0, 0).split().0,
                Chunk::filled(world.shared_registry(), BlockId::AIR).unwrap(),
            )
            .unwrap();
    }
    for x in edge..=edge + 31 {
        for z in 0..=2 {
            world.set_block(BlockPos::new(x, 0, z), stone).unwrap();
        }
    }
    let mut player = Player::new(DVec3::new(f64::from(edge) + 15.5, 1.0, 1.5)).unwrap();
    player.controller.set_look(0.0, 0.0).unwrap();
    let dt = 1.0 / 60.0;
    for _ in 0..20 {
        let previous = player.position();
        player
            .step(
                &world,
                FirstPersonInput {
                    movement: rayengine_voxel::glam::Vec2::X,
                    ..Default::default()
                },
                dt,
            )
            .unwrap();
        assert!(
            (as_global(player.origin) + player.controller.previous.as_dvec3() - previous).length()
                < 0.00001
        );
        assert!(player.controller.body.grounded);
    }
    assert_eq!(player.origin.x, edge + 16);
    assert!((player.position().y - 1.9).abs() < 0.0001);
    let floor_x = player.position().x.floor() as i32;
    let target = BlockPos::new(floor_x, 2, 0);
    world.set_block(target, stone).unwrap();
    assert!(
        matches!(player.selection(&world).unwrap().outcome,RaycastOutcome::Hit(hit) if hit.position==target)
    );
    for feet in [
        DVec3::splat(f64::NAN),
        DVec3::new(f64::from(i32::MAX) + 0.9, 1.0, 0.5),
        DVec3::new(0.5, f64::from(i32::MAX), 0.5),
        DVec3::new(f64::from(i32::MIN) + 0.1, 1.0, 0.5),
    ] {
        assert!(Player::new(feet).is_err());
    }
}
