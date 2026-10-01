//! Headless bounded streaming workload with machine-readable peak/timing output.
mod support {
    pub mod streaming;
}
fn main() {
    let mut scenario = support::streaming::fixture(0);
    let start = std::time::Instant::now();
    let p = support::streaming::travel_16(&mut scenario);
    let ns = start.elapsed().as_nanos();
    println!(
        "{{\"workload\":\"voxel-stream/travel-16.v1\",\"elapsed_ns\":{ns},\"loaded\":{},\"evicted\":{},\"peak_jobs\":{},\"peak_resident\":{},\"peak_mesh_slots\":{},\"peak_ready_bytes\":{},\"max_mesh_bytes\":{}}}",
        p.loaded, p.evicted, p.jobs, p.resident, p.mesh_slots, p.ready_bytes, p.mesh_bytes
    );
    scenario.cpu.shutdown();
}
