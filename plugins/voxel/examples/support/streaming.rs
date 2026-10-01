//! Shared, versioned fixtures for the headless example and streaming benchmark.
use rayengine_voxel::prelude::*;
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
pub struct Scenario {
    pub world: VoxelWorld,
    pub cpu: ChunkStreamer,
}
#[derive(Default, Debug)]
pub struct Peaks {
    pub jobs: usize,
    pub resident: usize,
    pub mesh_slots: usize,
    pub ready_bytes: usize,
    pub loaded: usize,
    pub evicted: usize,
    pub mesh_bytes: usize,
}
impl Peaks {
    pub fn observe(&mut self, r: StreamReport) {
        self.jobs = self.jobs.max(r.jobs);
        self.resident = self.resident.max(r.resident);
        self.mesh_slots = self.mesh_slots.max(r.mesh_slots);
        self.ready_bytes = self.ready_bytes.max(r.ready_bytes);
        self.loaded += r.loaded;
        self.evicted += r.evicted;
    }
}
pub fn fixture(radius: u32) -> Scenario {
    let mut registry = BlockRegistry::new();
    let stone = registry.register(BlockDef::new("workload:stone")).unwrap();
    let count = (2 * radius as usize + 1).pow(2);
    let config = StreamConfig {
        radius,
        max_resident: count,
        workers: 1,
        max_jobs: 2,
        max_meshes: 2,
        ..Default::default()
    };
    let cpu = ChunkStreamer::new(config, move |_, registry, cancel| {
        let mut cells = vec![BlockId::AIR; CHUNK_VOLUME];
        for z in 0..16 {
            for x in 0..16 {
                if cancel.is_cancelled() {
                    return Err(VoxelError::Allocation);
                }
                let height = 4 + x / 4 + z / 4;
                for y in 0..height {
                    cells[x + z * 16 + y * 256] = stone;
                }
            }
        }
        let mut chunk = Chunk::from_blocks(registry, cells)?;
        chunk.mark_saved(chunk.revision());
        Ok(chunk)
    })
    .unwrap();
    Scenario {
        world: VoxelWorld::new(Arc::new(registry), count),
        cpu,
    }
}
/// Settles every wanted chunk with a mock CPU consumer; no native upload occurs.
pub fn settle(s: &mut Scenario, focus: ChunkPos, peaks: &mut Peaks) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let report = s
            .cpu
            .tick(&mut s.world, focus, |_, _, _| Eviction::Keep)
            .unwrap();
        peaks.observe(report);
        while let Some(mesh) = s.cpu.take_mesh(&s.world) {
            peaks.mesh_bytes = peaks.mesh_bytes.max(mesh.stats().buffer_bytes);
            s.cpu.finish_mesh(mesh.dependencies(), true);
        }
        // One more tick checks that all accepted receipts are current and no work is left.
        let report = s
            .cpu
            .tick(&mut s.world, focus, |_, _, _| Eviction::Keep)
            .unwrap();
        peaks.observe(report);
        if report.jobs == 0 && report.ready == 0 && s.world.len() == report.desired {
            return;
        }
        assert!(Instant::now() < deadline, "stream workload timed out");
        thread::yield_now();
    }
}
/// Sixteen sequential focus hops. Generation, meshing and completion latency included.
pub fn travel_16(s: &mut Scenario) -> Peaks {
    let mut peaks = Peaks::default();
    for x in 0..16 {
        settle(s, ChunkPos::new(x, 0, 0), &mut peaks);
    }
    assert_eq!(peaks.loaded, 16);
    assert_eq!(peaks.evicted, 15);
    assert!(peaks.jobs <= 2 && peaks.mesh_slots <= 2 && peaks.resident <= 1);
    peaks
}
