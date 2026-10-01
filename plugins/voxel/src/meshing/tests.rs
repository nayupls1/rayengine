use super::*;
use crate::{BlockDef, BlockPos, Chunk, CollisionKind, LocalPos};
use rayengine_core::{
    camera::Camera3D,
    glam::Vec2,
    viewport::{ScaleMode, Viewport},
};
use std::collections::BTreeSet;

fn fixture(render: RenderKind) -> (VoxelWorld, BlockId) {
    let mut registry = BlockRegistry::new();
    let mut def = BlockDef::new("test:block");
    def.render = render;
    def.collision = CollisionKind::None; // Collision never controls visible faces.
    let id = registry.register(def).unwrap();
    let registry = Arc::new(registry);
    let mut world = VoxelWorld::new(registry.clone(), 8);
    world
        .insert_chunk(
            ChunkPos::default(),
            Chunk::filled(registry, BlockId::AIR).unwrap(),
        )
        .unwrap();
    (world, id)
}
fn build(world: &VoxelWorld, mode: MeshingMode) -> ChunkMesh {
    MeshInput::capture(world, ChunkPos::default())
        .unwrap()
        .build(MeshingOptions {
            mode,
            ..Default::default()
        })
        .unwrap()
}
#[test]
fn empty_and_uniform_chunks_have_exact_payloads_and_culled_face_counts() {
    let (mut world, id) = fixture(RenderKind::Opaque);
    let empty = build(&world, MeshingMode::Greedy);
    assert_eq!(empty.stats(), MeshStats::default());
    world
        .insert_chunk(
            ChunkPos::default(),
            Chunk::filled(world.shared_registry(), id).unwrap(),
        )
        .unwrap();
    let input = MeshInput::capture(&world, ChunkPos::default()).unwrap();
    assert_eq!(input.block_bytes(), 18 * 18 * 18 * 2);
    for (mode, quads) in [(MeshingMode::Culled, 1536), (MeshingMode::Greedy, 6)] {
        let mesh = input
            .build(MeshingOptions {
                mode,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            mesh.stats(),
            MeshStats {
                visible_faces: 1536,
                quads,
                vertices: quads * 4,
                triangles: quads * 2,
                batches: 1,
                buffer_bytes: quads * 156
            }
        );
        assert!(mesh.batches()[0].data().validate().is_ok());
    }
    let hidden = input
        .build(MeshingOptions {
            missing: MissingFaces::Hide,
            ..Default::default()
        })
        .unwrap();
    assert!(hidden.batches().is_empty());
}
#[test]
fn normals_winding_face_tiles_uv_orientation_and_shading() {
    let mut registry = BlockRegistry::new();
    let mut def = BlockDef::new("test:tiles");
    def.textures = std::array::from_fn(|i| TileId(i as u16));
    let id = registry.register(def).unwrap();
    let registry = Arc::new(registry);
    let mut world = VoxelWorld::new(registry.clone(), 1);
    let mut chunk = Chunk::filled(registry, BlockId::AIR).unwrap();
    chunk.set(LocalPos::new(3, 4, 5).unwrap(), id).unwrap();
    world.insert_chunk(ChunkPos::default(), chunk).unwrap();
    let mesh = build(&world, MeshingMode::Greedy);
    assert_eq!(mesh.stats().batches, 6);
    for (batch, face) in mesh.batches().iter().zip(Face::ALL) {
        assert_eq!(batch.surface().tile, TileId(face.index() as u16));
        let data = batch.data();
        data.validate().unwrap();
        let normal = Vec3::from_array(face.normal().map(|c| c as f32));
        assert_eq!(data.normals.as_ref().unwrap(), &vec![normal; 4]);
        for &[a, b, c] in data.indices.as_ref().unwrap().as_chunks::<3>().0 {
            let a = data.positions[usize::from(a)];
            let b = data.positions[usize::from(b)];
            let c = data.positions[usize::from(c)];
            assert!((b - a).cross(c - a).dot(normal) > 0.0);
        }
        let uv = data.texcoords.as_ref().unwrap();
        assert_eq!(uv.iter().copied().reduce(Vec2::min).unwrap(), Vec2::ZERO);
        assert_eq!(uv.iter().copied().reduce(Vec2::max).unwrap(), Vec2::ONE);
        if matches!(face, Face::NegX | Face::PosX | Face::NegZ | Face::PosZ) {
            for (p, uv) in data.positions.iter().zip(uv) {
                assert_eq!(uv.y, 5.0 - p.y); // Side textures are upright.
            }
        }
        let c = FaceShading::default().0[face.index()];
        assert_eq!(data.colors.as_ref().unwrap(), &vec![[c, c, c, 255]; 4]);
    }
}
#[test]
fn opaque_neighbors_hide_faces_but_cutout_holes_retain_geometry() {
    for (kind, expected) in [(RenderKind::Opaque, 10), (RenderKind::Cutout, 12)] {
        let (mut world, id) = fixture(kind);
        world.set_block(BlockPos::new(4, 4, 4), id).unwrap();
        world.set_block(BlockPos::new(5, 4, 4), id).unwrap();
        let mesh = build(&world, MeshingMode::Culled);
        assert_eq!(mesh.stats().visible_faces, expected);
        assert_eq!(
            mesh.batches()[0].surface().layer,
            if kind == RenderKind::Opaque {
                MeshLayer::Opaque
            } else {
                MeshLayer::Cutout
            }
        );
    }
    let (mut world, id) = fixture(RenderKind::Transparent);
    world.set_block(BlockPos::default(), id).unwrap();
    assert!(
        matches!(MeshInput::capture(&world, ChunkPos::default()).unwrap().build(Default::default()),
        Err(MeshingError::UnsupportedTransparent(b)) if b == id)
    );
}
#[test]
fn all_six_neighbor_slabs_and_signed_border_edits_invalidate_mesh_dependencies() {
    for face in Face::ALL {
        let (mut world, id) = fixture(RenderKind::Opaque);
        let owner = ChunkPos::new(-1, -1, -1);
        world.remove_chunk(ChunkPos::default());
        world
            .insert_chunk(owner, Chunk::filled(world.shared_registry(), id).unwrap())
            .unwrap();
        let before = MeshInput::capture(&world, owner)
            .unwrap()
            .build(Default::default())
            .unwrap();
        let neighbor = owner.neighbor(face).unwrap();
        world
            .insert_chunk(
                neighbor,
                Chunk::filled(world.shared_registry(), id).unwrap(),
            )
            .unwrap();
        assert!(!before.dependencies().is_current(&world));
        let input = MeshInput::capture(&world, owner).unwrap();
        let after = input.build(Default::default()).unwrap();
        assert_eq!(after.stats().visible_faces, 1280);
        assert_eq!(after.stats().quads, 5);
        let n = face.normal();
        let local = LocalPos::new(
            if n[0] > 0 { 0 } else { 15 },
            if n[1] > 0 { 0 } else { 15 },
            if n[2] > 0 { 0 } else { 15 },
        )
        .unwrap();
        let edit = world
            .set_block(neighbor.block(local).unwrap(), BlockId::AIR)
            .unwrap()
            .unwrap();
        assert!(edit.affected_chunks.as_slice().contains(&owner));
        assert!(!after.dependencies().is_current(&world));
        assert_eq!(
            MeshInput::capture(&world, owner)
                .unwrap()
                .build(Default::default())
                .unwrap()
                .stats()
                .visible_faces,
            1281
        );
        // Owned snapshot does not read new world data during its build.
        assert_eq!(
            input
                .build(Default::default())
                .unwrap()
                .stats()
                .visible_faces,
            1280
        );
        world.remove_chunk(neighbor);
        assert!(!after.dependencies().is_current(&world));
    }
}
#[test]
fn receipts_reject_other_worlds_reinstalled_owners_and_missing_neighbor_aba() {
    let (mut world, id) = fixture(RenderKind::Opaque);
    let input = MeshInput::capture(&world, ChunkPos::default()).unwrap();
    let mut other = VoxelWorld::new(world.shared_registry(), 1);
    other
        .insert_chunk(
            ChunkPos::default(),
            Chunk::filled(other.shared_registry(), BlockId::AIR).unwrap(),
        )
        .unwrap();
    assert_eq!(
        world.stamp(ChunkPos::default()),
        other.stamp(ChunkPos::default())
    );
    assert!(!input.dependencies().is_current(&other));
    world
        .insert_chunk(
            ChunkPos::new(1, 0, 0),
            Chunk::filled(world.shared_registry(), id).unwrap(),
        )
        .unwrap();
    let neighbor_input = MeshInput::capture(&world, ChunkPos::default()).unwrap();
    let neighbor = world.remove_chunk(ChunkPos::new(1, 0, 0)).unwrap();
    world
        .insert_chunk(ChunkPos::new(1, 0, 0), neighbor)
        .unwrap();
    assert!(!neighbor_input.dependencies().is_current(&world));
    let old = world.remove_chunk(ChunkPos::default()).unwrap();
    world.insert_chunk(ChunkPos::default(), old).unwrap();
    assert!(!input.dependencies().is_current(&world));
    assert!(MeshInput::capture(&world, ChunkPos::new(99, 0, 0)).is_err());
    assert!(MeshInput::capture(&world, ChunkPos::new(i32::MAX, 0, 0)).is_err());
}

// Expand geometric rectangles to unit faces; compare to a direct world-cell oracle.
fn coverage(mesh: &ChunkMesh) -> BTreeSet<(i32, i32, i32, usize, u16)> {
    let mut out = BTreeSet::new();
    for batch in mesh.batches() {
        for (i, quad) in batch.data().positions.as_chunks::<4>().0.iter().enumerate() {
            let min = quad.iter().copied().reduce(Vec3::min).unwrap().to_array();
            let max = quad.iter().copied().reduce(Vec3::max).unwrap().to_array();
            let normal = batch.data().normals.as_ref().unwrap()[i * 4];
            let face = Face::ALL
                .into_iter()
                .find(|f| Vec3::from_array(f.normal().map(|x| x as f32)) == normal)
                .unwrap();
            let (axis, u, v, positive) = axes(face);
            for b in min[v] as i32..max[v] as i32 {
                for a in min[u] as i32..max[u] as i32 {
                    let mut p = [0; 3];
                    p[axis] = min[axis] as i32 - i32::from(positive);
                    p[u] = a;
                    p[v] = b;
                    assert!(
                        out.insert((p[0], p[1], p[2], face.index(), batch.surface().tile.0)),
                        "duplicate unit face"
                    );
                }
            }
            let uv = &batch.data().texcoords.as_ref().unwrap()[i * 4..i * 4 + 4];
            let span = uv.iter().copied().reduce(Vec2::max).unwrap()
                - uv.iter().copied().reduce(Vec2::min).unwrap();
            assert_eq!(span, Vec2::new(max[u] - min[u], max[v] - min[v]));
        }
    }
    out
}
#[test]
fn greedy_and_culled_cover_the_same_seeded_faces_without_stretching_or_cross_tile_merges() {
    let mut registry = BlockRegistry::new();
    let ids: Vec<_> = (0..3)
        .map(|i| {
            let mut d = BlockDef::new(format!("test:{i}"));
            d.textures = [TileId(i); 6];
            registry.register(d).unwrap()
        })
        .collect();
    let registry = Arc::new(registry);
    let mut world = VoxelWorld::new(registry.clone(), 1);
    let mut chunk = Chunk::filled(registry, BlockId::AIR).unwrap();
    let mut seed = 123456789u64;
    for i in 0..4096 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        if !(seed >> 32).is_multiple_of(5) {
            chunk
                .set(
                    LocalPos::from_index(i).unwrap(),
                    ids[(seed >> 48) as usize % 3],
                )
                .unwrap();
        }
    }
    world.insert_chunk(ChunkPos::default(), chunk).unwrap();
    let mut oracle = BTreeSet::new();
    for i in 0..4096 {
        let pos = ChunkPos::default()
            .block(LocalPos::from_index(i).unwrap())
            .unwrap();
        let id = world.block(pos).unwrap();
        let def = world.registry().get(id).unwrap();
        if def.render == RenderKind::Invisible {
            continue;
        }
        for face in Face::ALL {
            if world
                .block(pos.neighbor(face).unwrap())
                .is_none_or(|id| world.registry().get(id).unwrap().render != RenderKind::Opaque)
            {
                oracle.insert((pos.x, pos.y, pos.z, face.index(), def.texture(face).0));
            }
        }
    }
    for mode in [MeshingMode::Culled, MeshingMode::Greedy] {
        let mesh = build(&world, mode);
        assert_eq!(coverage(&mesh), oracle);
    }
}
#[test]
fn worst_case_cutout_splits_at_16_bit_limits_and_respects_all_output_budgets() {
    let (mut world, id) = fixture(RenderKind::Cutout);
    world
        .insert_chunk(
            ChunkPos::default(),
            Chunk::filled(world.shared_registry(), id).unwrap(),
        )
        .unwrap();
    let input = MeshInput::capture(&world, ChunkPos::default()).unwrap();
    let mesh = input
        .build(MeshingOptions {
            mode: MeshingMode::Culled,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(mesh.stats().quads, MAX_CHUNK_FACES);
    assert_eq!(mesh.stats().batches, 2);
    for batch in mesh.batches() {
        batch.data().validate().unwrap();
        assert!(batch.data().positions.len() <= 65532);
    }
    for limits in [
        MeshLimits {
            max_quads: 1,
            ..Default::default()
        },
        MeshLimits {
            max_batches: 1,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            input.build(MeshingOptions {
                mode: MeshingMode::Culled,
                limits,
                ..Default::default()
            }),
            Err(MeshingError::LimitExceeded)
        ));
    }
    for vertices in [0, 3, 5, 65536, usize::MAX] {
        assert!(matches!(
            input.build(MeshingOptions {
                limits: MeshLimits {
                    max_vertices_per_batch: vertices,
                    ..Default::default()
                },
                ..Default::default()
            }),
            Err(MeshingError::InvalidLimits)
        ));
    }
    let small = input
        .build(MeshingOptions {
            limits: MeshLimits {
                max_vertices_per_batch: 4,
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    assert_eq!(small.stats().batches, small.stats().quads);
    assert!(small.batches().iter().all(|b| b.data().validate().is_ok()));
}
#[test]
fn worker_transfer_and_camera_relative_visibility_preserve_signed_grid_extremes() {
    let (mut world, id) = fixture(RenderKind::Opaque);
    let pos = ChunkPos::new(134217727, 0, -134217728);
    world
        .insert_chunk(pos, Chunk::filled(world.shared_registry(), id).unwrap())
        .unwrap();
    let input = MeshInput::capture(&world, pos).unwrap();
    let mesh = std::thread::spawn(move || input.build(Default::default()).unwrap())
        .join()
        .unwrap();
    let origin = pos.origin().unwrap();
    assert_eq!(chunk_translation(pos, origin), Vec3::ZERO);
    assert_eq!(chunk_bounds(pos, origin).size(), Vec3::splat(16.0));
    let viewport =
        Viewport::new(Vec2::splat(640.0), Vec2::splat(640.0), ScaleMode::Expand).unwrap();
    let camera = Camera3D {
        position: Vec3::new(8.0, 8.0, 32.0),
        target: Vec3::splat(8.0),
        ..Default::default()
    };
    let view = Frustum3D::from_camera(&camera, &viewport, 0.1, 100.0).unwrap();
    assert!(mesh.visible(&view, origin));
    let away = Camera3D {
        target: camera.position + Vec3::Z,
        ..camera
    };
    assert!(!mesh.visible(
        &Frustum3D::from_camera(&away, &viewport, 0.1, 100.0).unwrap(),
        origin
    ));
    assert!(mesh.batches().iter().all(|b| {
        b.data()
            .positions
            .iter()
            .all(|p| p.min_element() >= 0.0 && p.max_element() <= 16.0)
    }));
}
