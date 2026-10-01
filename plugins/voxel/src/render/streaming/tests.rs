use super::*;
use crate::{
    BlockDef, BlockId, BlockPos, BlockRegistry, Chunk, ChunkStamp, Eviction, MeshInput,
    MeshingOptions, StreamConfig,
};
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
struct Probe {
    world: VoxelWorld,
    cpu: ChunkStreamer,
    gpu: StreamRenderer,
    materials: VoxelMaterials,
    stage: usize,
    ticks: usize,
    old: Vec<MeshId>,
    done: Arc<std::sync::atomic::AtomicBool>,
}
impl Probe {
    fn ready(&mut self, p: ChunkPos) {
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            let r = self
                .cpu
                .tick(&mut self.world, p, |_, _, _: ChunkStamp| Eviction::Saved)
                .unwrap();
            if r.ready > 0 {
                return;
            }
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn pump(&mut self, frame: &mut Frame<'_, '_>, bytes: usize) -> StreamRenderReport {
        let r = self.gpu.pump(
            &self.world,
            &mut self.cpu,
            &self.materials,
            frame,
            UploadBudget {
                max_requests: 1,
                max_bytes: bytes,
                max_time: Duration::from_secs(1),
            },
        );
        assert!(r.uploads.attempted <= 1);
        assert!(r.uploads.bytes <= bytes);
        assert!(r.peak_resources.meshes <= self.gpu.config.max_meshes);
        assert!(r.peak_resources.buffer_bytes <= self.gpu.config.max_buffer_bytes);
        assert!(r.resources.meshes <= self.gpu.config.max_meshes);
        assert!(r.resources.buffer_bytes <= self.gpu.config.max_buffer_bytes);
        assert!(r.resources.staged_requests <= self.gpu.config.max_staging_requests);
        assert!(r.resources.staged_bytes <= self.gpu.config.max_staging_bytes);
        assert_eq!(
            frame.assets.resource_counts().meshes as usize,
            r.resources.meshes
        );
        assert_eq!(
            frame.assets.resource_counts().generated_mesh_bytes as usize,
            r.resources.buffer_bytes
        );
        r
    }
}
impl Game for Probe {
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        let material = ctx.material(MaterialDesc::default())?;
        self.materials.bind(
            SurfaceKey {
                tile: TileId(0),
                layer: MeshLayer::Opaque,
            },
            material,
        )?;
        self.materials.bind(
            SurfaceKey {
                tile: TileId(1),
                layer: MeshLayer::Opaque,
            },
            material,
        )?;
        Ok(())
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        self.ticks += 1;
        assert!(self.ticks <= 25);
        let p = ChunkPos::default();
        match self.stage {
            0 => {
                self.ready(p);
                let r = self.pump(frame, 1); // No oversized exception.
                assert_eq!(r.uploads.attempted, 0);
                assert!(r.uploads.blocked_upload_bytes.is_some());
                assert_eq!(r.resources.meshes, 0);
                self.stage = 1;
            }
            1 => {
                let r = self.pump(frame, usize::MAX);
                assert!(r.error.is_none());
                if r.committed == 1 {
                    let c = self.gpu.chunk(p).unwrap();
                    assert_eq!(c.batches.len(), 2);
                    self.old = c.batches.iter().map(|b| b.mesh).collect();
                    self.world
                        .set_block(BlockPos::new(0, 0, 0), BlockId::AIR)
                        .unwrap();
                    self.ready(p);
                    self.stage = 2;
                } else {
                    assert!(self.gpu.chunk(p).is_none());
                }
            }
            2 => {
                let r = self.pump(frame, usize::MAX);
                assert_eq!(r.committed, 0);
                assert_eq!(r.resources.meshes, 3); // Two old + one partial.
                assert!(self.old.iter().all(|id| frame.assets.mesh(*id).is_some()));
                self.world
                    .set_block(BlockPos::new(1, 0, 0), BlockId::AIR)
                    .unwrap();
                self.cpu
                    .tick(&mut self.world, p, |_, _, _| Eviction::Keep)
                    .unwrap();
                let r = self.pump(frame, 0);
                assert_eq!(r.discarded, 1);
                assert_eq!(r.resources.meshes, 2); // Partial stale upload was unloaded.
                self.ready(p);
                self.stage = 3;
            }
            3 => {
                let r = self.pump(frame, usize::MAX);
                if r.committed == 1 {
                    assert_eq!(r.resources.meshes, 2);
                    assert!(self.old.iter().all(|id| frame.assets.mesh(*id).is_none()));
                    self.old = self
                        .gpu
                        .chunk(p)
                        .unwrap()
                        .batches
                        .iter()
                        .map(|b| b.mesh)
                        .collect();
                    // Reject whole replacement if old+new cannot fit the hard bound.
                    self.gpu.config.max_meshes = 2;
                    self.world
                        .set_block(BlockPos::new(2, 0, 0), BlockId::AIR)
                        .unwrap();
                    self.ready(p);
                    self.stage = 4;
                }
            }
            4 => {
                let r = self.pump(frame, usize::MAX);
                assert!(r.error.is_some());
                assert_eq!(r.uploads.attempted, 0);
                assert_eq!(r.resources.meshes, 2);
                assert!(self.old.iter().all(|id| frame.assets.mesh(*id).is_some()));
                assert_eq!(self.cpu.failure(p), Some(&crate::StreamFailure::Upload));
                self.gpu.config.max_meshes = 8;
                self.cpu.retry(p);
                self.ready(p);
                let r = self.pump(frame, usize::MAX);
                assert_eq!(r.resources.meshes, 3);
                // Invalidate a borrowed material after the first successful batch.
                let id = self
                    .materials
                    .surface(SurfaceKey {
                        tile: TileId(1),
                        layer: MeshLayer::Opaque,
                    })
                    .unwrap();
                frame.assets.unload_material(id);
                self.stage = 5;
            }
            5 => {
                let r = self.pump(frame, usize::MAX);
                assert!(r.error.is_some());
                assert_eq!(r.resources.meshes, 2);
                assert!(self.old.iter().all(|id| frame.assets.mesh(*id).is_some()));
                // Clear geometry with a valid empty mesh despite missing material.
                for index in 0..crate::CHUNK_VOLUME {
                    self.world
                        .set_block(
                            p.block(crate::LocalPos::from_index(index).unwrap())
                                .unwrap(),
                            BlockId::AIR,
                        )
                        .unwrap();
                }
                self.ready(p);
                self.stage = 6;
            }
            6 => {
                let r = self.pump(frame, usize::MAX);
                assert_eq!(r.committed, 1);
                assert_eq!(r.resources.meshes, 0);
                let next = ChunkPos::new(2, 0, 0);
                self.cpu
                    .tick(&mut self.world, next, |_, _, _| Eviction::Saved)
                    .unwrap();
                let r = self.pump(frame, 0);
                assert_eq!(r.unloaded, 1);
                assert_eq!(r.resources.chunks, 0);
                self.gpu.unload(&mut self.cpu, frame.assets);
                self.gpu.unload(&mut self.cpu, frame.assets);
                self.cpu.shutdown();
                self.done.store(true, std::sync::atomic::Ordering::Release);
                self.stage = 7;
            }
            _ => {}
        }
    }
}
#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_voxel_streaming_budgets_stale_rollback_replacement_and_unload() {
    let mut registry = BlockRegistry::new();
    let a = registry.register(BlockDef::new("test:a")).unwrap();
    let mut b = BlockDef::new("test:b");
    b.textures = [TileId(1); 6];
    let b = registry.register(b).unwrap();
    let mut world = VoxelWorld::new(Arc::new(registry), 1);
    let mut chunk = Chunk::filled(world.shared_registry(), a).unwrap();
    chunk
        .set(crate::LocalPos::new(15, 15, 15).unwrap(), b)
        .unwrap();
    world.insert_chunk(ChunkPos::default(), chunk).unwrap();
    assert_eq!(
        MeshInput::capture(&world, ChunkPos::default())
            .unwrap()
            .build(MeshingOptions::default())
            .unwrap()
            .stats()
            .batches,
        2
    );
    let cpu = ChunkStreamer::new(
        StreamConfig {
            radius: 0,
            max_resident: 1,
            workers: 1,
            max_jobs: 1,
            max_meshes: 1,
            ..Default::default()
        },
        |_, registry, _| Chunk::filled(registry, BlockId::AIR),
    )
    .unwrap();
    let gpu = StreamRenderer::new(StreamRenderConfig {
        max_chunks: 1,
        max_meshes: 8,
        ..Default::default()
    })
    .unwrap();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut config = Config::new("Voxel streaming probe");
    config.audio = false;
    config.vsync = false;
    config.window_size = (64, 64);
    App::new(config)
        .with_options(RunOptions {
            hidden: true,
            frames: Some(20),
            uncapped: true,
            ..Default::default()
        })
        .run(Probe {
            world,
            cpu,
            gpu,
            materials: VoxelMaterials::new(),
            stage: 0,
            ticks: 0,
            old: Vec::new(),
            done: done.clone(),
        })
        .unwrap();
    assert!(done.load(std::sync::atomic::Ordering::Acquire));
}

#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_voxel_streaming_rejects_alpha_changes_during_staging() {
    struct AlphaProbe {
        world: VoxelWorld,
        cpu: ChunkStreamer,
        gpu: StreamRenderer,
        materials: VoxelMaterials,
        layer: MeshLayer,
        replacement_alpha: AlphaMode,
    }
    impl AlphaProbe {
        fn ready(&mut self) {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if self
                    .cpu
                    .tick(&mut self.world, ChunkPos::default(), |_, _, _| {
                        Eviction::Keep
                    })
                    .unwrap()
                    .ready
                    > 0
                {
                    break;
                }
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(1));
            }
        }
        fn pump(&mut self, frame: &mut Frame<'_, '_>, requests: usize) -> StreamRenderReport {
            self.gpu.pump(
                &self.world,
                &mut self.cpu,
                &self.materials,
                frame,
                UploadBudget {
                    max_requests: requests,
                    max_bytes: usize::MAX,
                    max_time: Duration::from_secs(1),
                },
            )
        }
    }
    impl Game for AlphaProbe {
        fn init(&mut self, context: &mut InitContext<'_, '_>) -> Result<(), Error> {
            for (tile, layer, alpha) in [
                (TileId(0), MeshLayer::Opaque, AlphaMode::Opaque),
                (TileId(1), MeshLayer::Cutout, AlphaMode::Cutout(0.5)),
            ] {
                let material = context.material(MaterialDesc {
                    alpha,
                    ..Default::default()
                })?;
                self.materials.bind(SurfaceKey { tile, layer }, material)?;
            }
            Ok(())
        }
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
        fn draw(&mut self, frame: &mut Frame<'_, '_>) {
            self.ready();
            assert_eq!(self.pump(frame, 8).committed, 1);
            let position = ChunkPos::default();
            let old: Vec<_> = self
                .gpu
                .chunk(position)
                .unwrap()
                .batches
                .iter()
                .map(|b| b.mesh)
                .collect();
            assert_eq!(old.len(), 2);
            self.world
                .set_block(BlockPos::default(), BlockId::AIR)
                .unwrap();
            self.ready();
            let staged = self.pump(frame, 1);
            assert_eq!(staged.committed, 0);
            assert_eq!(staged.resources.meshes, 3);
            let key = SurfaceKey {
                tile: TileId(u16::from(self.layer == MeshLayer::Cutout)),
                layer: self.layer,
            };
            let id = self.materials.surface(key).unwrap();
            let mut desc = frame.assets.material(id).unwrap().clone();
            desc.alpha = self.replacement_alpha;
            frame.assets.replace_material(id, desc).unwrap();
            let rejected = self.pump(frame, 1);
            assert!(
                rejected.error.is_some(),
                "changed alpha policy must reject the transaction"
            );
            assert_eq!(rejected.committed, 0);
            assert_eq!(rejected.resources.meshes, 2);
            assert_eq!(
                self.gpu
                    .chunk(position)
                    .unwrap()
                    .batches
                    .iter()
                    .map(|b| b.mesh)
                    .collect::<Vec<_>>(),
                old
            );
            assert!(old.iter().all(|&id| frame.assets.mesh(id).is_some()));
            assert_eq!(
                self.cpu.failure(position),
                Some(&crate::StreamFailure::Upload)
            );
            self.gpu.unload(&mut self.cpu, frame.assets);
            self.cpu.shutdown();
            frame.clear(Color::BLACK);
        }
    }
    for (layer, replacement_alpha) in [
        (MeshLayer::Opaque, AlphaMode::Blend),
        (MeshLayer::Opaque, AlphaMode::Cutout(0.5)),
        (MeshLayer::Cutout, AlphaMode::Opaque),
    ] {
        let mut registry = BlockRegistry::new();
        let opaque = registry.register(BlockDef::new("test:opaque")).unwrap();
        let mut cutout = BlockDef::new("test:cutout");
        cutout.render = crate::RenderKind::Cutout;
        cutout.textures = [TileId(1); 6];
        let cutout = registry.register(cutout).unwrap();
        let mut world = VoxelWorld::new(Arc::new(registry), 1);
        let mut chunk = Chunk::filled(world.shared_registry(), opaque).unwrap();
        chunk
            .set(crate::LocalPos::new(15, 15, 15).unwrap(), cutout)
            .unwrap();
        world.insert_chunk(ChunkPos::default(), chunk).unwrap();
        let cpu = ChunkStreamer::new(
            StreamConfig {
                radius: 0,
                max_resident: 1,
                workers: 1,
                max_jobs: 1,
                max_meshes: 1,
                ..Default::default()
            },
            |_, r, _| Chunk::filled(r, BlockId::AIR),
        )
        .unwrap();
        let mut config = Config::new("Streaming material mutation probe");
        config.audio = false;
        config.vsync = false;
        config.window_size = (64, 64);
        App::new(config)
            .with_options(RunOptions {
                hidden: true,
                frames: Some(1),
                uncapped: true,
                ..Default::default()
            })
            .run(AlphaProbe {
                world,
                cpu,
                gpu: StreamRenderer::new(StreamRenderConfig::default()).unwrap(),
                materials: VoxelMaterials::new(),
                layer,
                replacement_alpha,
            })
            .unwrap();
    }
}
