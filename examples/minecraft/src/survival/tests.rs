use super::*;
use crate::terrain::{Terrain, TerrainSettings};
fn fixture() -> (VoxelWorld, DemoBlocks, Player) {
    let terrain = Terrain::new(42, TerrainSettings::default()).unwrap();
    let b = terrain.blocks();
    let mut world = VoxelWorld::new(terrain.registry(), 27);
    for x in -1..=1 {
        for y in 0..=2 {
            for z in -1..=1 {
                world
                    .insert_chunk(
                        ChunkPos::new(x, y, z),
                        Chunk::filled(terrain.registry(), BlockId::AIR).unwrap(),
                    )
                    .unwrap();
            }
        }
    }
    let mut player = Player::new(DVec3::new(0.5, 1.0, 0.5)).unwrap();
    player.controller.set_look(0.0, 0.0).unwrap();
    (world, b, player)
}
#[test]
fn stacks_fill_existing_before_empty_and_tools_never_stack() {
    let mut inv = Inventory::default();
    assert_eq!(inv.insert(Item::Dirt, 70), 0);
    assert_eq!(inv.slots()[0].unwrap().count(), 64);
    assert_eq!(inv.slots()[1].unwrap().count(), 6);
    inv.swap(1, 20);
    inv.insert(Item::Dirt, 5);
    assert!(inv.slots()[1].is_none());
    assert_eq!(inv.slots()[20].unwrap().count(), 11);
    inv.insert(Item::WoodenPickaxe, 2);
    assert_eq!(inv.slots()[1].unwrap().count(), 1);
    assert_eq!(inv.slots()[2].unwrap().count(), 1);
    let before = inv;
    assert!(!inv.remove(Item::Dirt, 100));
    assert_eq!(inv, before);
    assert!(inv.remove(Item::Dirt, 75));
    assert_eq!(inv.count(Item::Dirt), 0);
    assert!(!inv.swap(0, 36));
    assert!(!inv.consume(36));
    assert_eq!(inv.insert(Item::Dirt, u16::MAX), u16::MAX - 34 * 64);
    assert_eq!(inv.insert(Item::Stone, 1), 1);
}
#[test]
fn crafting_bootstraps_tools_and_rejections_are_atomic() {
    let mut inv = Inventory::default();
    inv.insert(Item::Log, 2);
    for r in [
        Recipe::Planks,
        Recipe::Planks,
        Recipe::Sticks,
        Recipe::WoodenPickaxe,
    ] {
        inv.craft(r).unwrap();
    }
    assert_eq!(inv.count(Item::WoodenPickaxe), 1);
    assert_eq!(inv.count(Item::Stick), 2);
    assert_eq!(inv.count(Item::Planks), 3);
    inv.insert(Item::Stone, 3);
    inv.craft(Recipe::StonePickaxe).unwrap();
    assert_eq!(inv.count(Item::StonePickaxe), 1);
    let before = inv;
    assert_eq!(inv.craft(Recipe::StoneAxe), Err(CraftError::Ingredients));
    assert_eq!(inv, before);
    let mut full = Inventory::default();
    full.insert(Item::Dirt, 34 * 64);
    full.insert(Item::Planks, 64);
    full.insert(Item::Stick, 64);
    let before = full;
    assert_eq!(full.craft(Recipe::WoodenPickaxe), Err(CraftError::Capacity));
    assert_eq!(full, before);
    // Consumed ingredients can free a result slot in an otherwise full inventory.
    full.remove(Item::Planks, 61);
    assert!(full.can_craft(Recipe::WoodenPickaxe));
    full.craft(Recipe::WoodenPickaxe).unwrap();
    assert_eq!(full.count(Item::WoodenPickaxe), 1);
    for r in Recipe::ALL {
        let mut inv = Inventory::default();
        for &(item, count) in r.ingredients() {
            inv.insert(item, count);
        }
        inv.craft(r).unwrap();
        let (item, count) = r.output();
        assert_eq!(inv.count(item), count);
    }
}
#[test]
fn mining_pickups_and_placement_conserve_counts_on_failure() {
    let (mut world, b, player) = fixture();
    let target = BlockPos::new(0, 2, -1);
    world.set_block(target, b.wood).unwrap();
    let mut survival = Survival::default();
    let mut interaction = Interaction::default();
    let tick = SurvivalInput {
        mining: true,
        dt: 1.1,
        ..Default::default()
    };
    let r = survival
        .interact(&mut interaction, &mut world, &player, tick, b)
        .unwrap();
    assert_eq!(r.edit.unwrap().previous, b.wood);
    assert_eq!(survival.pickups().len(), 1);
    assert_eq!(survival.collect(player.position()), 1);
    assert_eq!(survival.inventory.count(Item::Log), 1);
    // Near placement hits the player and leaves the item untouched.
    world.set_block(target, b.stone).unwrap();
    let place = SurvivalInput {
        place: true,
        dt: 0.02,
        ..Default::default()
    };
    assert!(
        survival
            .interact(&mut interaction, &mut world, &player, place, b)
            .unwrap()
            .edit
            .is_none()
    );
    assert_eq!(survival.inventory.count(Item::Log), 1);
    // A further target has a valid adjacent air cell: one committed edit consumes one.
    world.set_block(target, BlockId::AIR).unwrap();
    world.set_block(BlockPos::new(0, 2, -3), b.stone).unwrap();
    let r = survival
        .interact(&mut interaction, &mut world, &player, place, b)
        .unwrap();
    assert_eq!(r.edit.unwrap().position, BlockPos::new(0, 2, -2));
    assert_eq!(survival.inventory.count(Item::Log), 0);
    assert!(
        survival
            .interact(&mut interaction, &mut world, &player, place, b)
            .unwrap()
            .edit
            .is_none()
    );
    // Ingredients are not blocks even when selected.
    survival.inventory.insert(Item::Planks, 4);
    assert!(
        survival
            .interact(&mut interaction, &mut world, &player, place, b)
            .unwrap()
            .edit
            .is_none()
    );
    assert_eq!(survival.inventory.count(Item::Planks), 4);
}
#[test]
fn tool_tiers_speed_and_switching_reset_progress() {
    let (mut world, b, player) = fixture();
    assert_eq!(drop_item(b.stone, None, b), None);
    assert_eq!(
        drop_item(b.coal, Some(Item::WoodenPickaxe), b),
        Some(Item::Coal)
    );
    assert_eq!(drop_item(b.iron, Some(Item::WoodenPickaxe), b), None);
    assert_eq!(
        drop_item(b.iron, Some(Item::StonePickaxe), b),
        Some(Item::IronOre)
    );
    assert_eq!(drop_item(b.bedrock, Some(Item::StonePickaxe), b), None);
    assert_eq!(Item::WoodenAxe.mining_speed(b.stone, b), 1.0);
    let target = BlockPos::new(0, 2, -2);
    world.set_block(target, b.stone).unwrap();
    let mut survival = Survival::default();
    survival.inventory.insert(Item::WoodenPickaxe, 1);
    survival.inventory.insert(Item::StonePickaxe, 1);
    let mut i = Interaction::default();
    let tick = SurvivalInput {
        mining: true,
        dt: 0.2,
        ..Default::default()
    };
    let r = survival
        .interact(&mut i, &mut world, &player, tick, b)
        .unwrap();
    assert!((r.progress - 0.3).abs() < 0.0001);
    survival.select(1);
    let r = survival
        .interact(&mut i, &mut world, &player, tick, b)
        .unwrap();
    assert!((r.progress - 0.6).abs() < 0.0001); // restarted, not .9
    assert!(
        survival
            .interact(&mut i, &mut world, &player, tick, b)
            .unwrap()
            .edit
            .is_some()
    );
    assert_eq!(survival.pickups()[0].stack.item(), Item::Stone);
    assert_eq!(survival.inventory.count(Item::StonePickaxe), 1);
}
#[test]
fn full_pickups_prevent_world_mutation_and_full_inventory_retains_drops() {
    let (mut world, b, player) = fixture();
    let target = BlockPos::new(0, 2, -2);
    world.set_block(target, b.wood).unwrap();
    let mut s = Survival::default();
    s.inventory.insert(Item::Dirt, 36 * 64);
    s.pickups.push(Pickup {
        position: player.position(),
        stack: Stack {
            item: Item::Log,
            count: 5,
        },
    });
    assert_eq!(s.collect(player.position()), 0);
    assert_eq!(s.pickups()[0].stack.count(), 5);
    s.inventory.remove(Item::Dirt, 64);
    assert_eq!(s.collect(player.position()), 5);
    assert!(s.pickups().is_empty());
    s.pickups.resize(
        MAX_PICKUPS,
        Pickup {
            position: DVec3::splat(100.0),
            stack: Stack {
                item: Item::Log,
                count: 1,
            },
        },
    );
    let r = s
        .interact(
            &mut Interaction::default(),
            &mut world,
            &player,
            SurvivalInput {
                mining: true,
                dt: 2.0,
                ..Default::default()
            },
            b,
        )
        .unwrap();
    assert!(r.edit.is_none());
    assert_eq!(world.block(target), Some(b.wood));
    assert!(!s.select(HOTBAR_SLOTS));
    s.health.damage(MAX_HEALTH);
    s.pickups.clear();
    assert!(
        s.interact(
            &mut Interaction::default(),
            &mut world,
            &player,
            SurvivalInput {
                mining: true,
                dt: 2.0,
                ..Default::default()
            },
            b
        )
        .unwrap()
        .edit
        .is_none()
    );
}
#[test]
fn fall_damage_uses_peak_and_resets_on_landing_and_respawn() {
    let mut h = Health::default();
    h.movement(10.0, true);
    h.movement(11.3, false); // ordinary jump is safe
    assert_eq!(h.movement(10.0, true), 0);
    assert_eq!(h.value(), 20);
    h.movement(16.0, false);
    h.movement(13.0, false);
    assert_eq!(h.movement(10.0, true), 3);
    assert_eq!(h.movement(10.0, true), 0);
    h.movement(40.0, false);
    h.movement(10.0, true);
    assert_eq!(h.value(), 0);
    h.damage(255);
    assert_eq!(h.value(), 0);
    h.respawn();
    assert_eq!(h.movement(1.0, true), 0);
    assert_eq!(h.value(), 20);
    h.movement(-17.0, false);
    assert_eq!(h.value(), 0);
}
#[test]
fn respawn_uses_loaded_edited_terrain_and_stays_collision_safe() {
    let (mut world, b, _) = fixture();
    let spawn = BlockPos::new(0, 0, 0);
    assert!(respawn_feet(&world, spawn).is_none());
    world.set_block(spawn, b.stone).unwrap();
    assert_eq!(respawn_feet(&world, spawn), Some(DVec3::new(0.5, 1.0, 0.5)));
    world.set_block(BlockPos::new(0, 1, 0), b.wood).unwrap();
    let feet = respawn_feet(&world, spawn).unwrap();
    assert_eq!(feet.y, 2.0); // never inside edited spawn block
    let player = Player::new(feet).unwrap();
    assert!(!crate::gameplay::can_place(
        &world,
        BlockPos::new(0, 3, 0),
        b.dirt,
        &player
    ));
    world.remove_chunk(spawn.split().0);
    assert!(respawn_feet(&world, spawn).is_none());
}

#[test]
fn partial_pickup_collection_retains_the_unaccepted_remainder() {
    let mut survival = Survival::default();
    survival.inventory.insert(Item::Dirt, 35 * 64);
    survival.inventory.insert(Item::Log, 63);
    survival.pickups.push(Pickup {
        position: DVec3::ZERO,
        stack: Stack {
            item: Item::Log,
            count: 5,
        },
    });
    assert_eq!(survival.collect(DVec3::ZERO), 1);
    assert_eq!(survival.inventory.count(Item::Log), 64);
    assert_eq!(survival.pickups()[0].stack.count(), 4);
    assert_eq!(survival.collect(DVec3::ZERO), 0);
    survival.inventory.remove(Item::Dirt, 64);
    assert_eq!(survival.collect(DVec3::ZERO), 4);
    assert!(survival.pickups().is_empty());
}
