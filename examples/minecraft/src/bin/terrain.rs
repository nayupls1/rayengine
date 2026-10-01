//! Headless terrain fixture and safe-spawn report; no native context required.
use clap::Parser;
use rayengine_minecraft::terrain::{Terrain, TerrainSettings, chunk_fingerprint};
use rayengine_voxel::prelude::*;
use std::collections::BTreeMap;
#[derive(Parser)]
#[command(about = "Generate a deterministic terrain chunk and report its safe spawn")]
struct Args {
    #[arg(long, default_value_t = 42)]
    seed: u64,
    #[arg(long, default_value = "0,2,0", allow_hyphen_values = true, value_parser = parse_chunk)]
    chunk: ChunkPos,
}
fn parse_chunk(value: &str) -> Result<ChunkPos, String> {
    let coordinates: Vec<i32> = value
        .split(',')
        .map(|s| {
            s.parse()
                .map_err(|_| "chunk must be X,Y,Z integers".to_string())
        })
        .collect::<Result<_, _>>()?;
    let [x, y, z] = coordinates.as_slice() else {
        return Err("chunk must have exactly three coordinates".into());
    };
    let p = ChunkPos::new(*x, *y, *z);
    p.origin().map_err(|e| e.to_string())?;
    Ok(p)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let terrain = Terrain::new(args.seed, TerrainSettings::default())?;
    let start = std::time::Instant::now();
    let chunk = terrain.chunk(args.chunk)?;
    let elapsed_ns = start.elapsed().as_nanos();
    let mut blocks = BTreeMap::new();
    for &id in chunk.blocks() {
        *blocks
            .entry(chunk.registry().get(id).unwrap().name.clone())
            .or_insert(0usize) += 1;
    }
    let origin = args.chunk.origin()?;
    let spawn = terrain.find_spawn(origin.x, origin.z, 16, 1089)?;
    let info = terrain.info();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1, "generator": info.name, "generator_version": info.version,
            "seed": info.seed, "settings": terrain.settings(), "chunk": [args.chunk.x, args.chunk.y, args.chunk.z],
            "fingerprint": format!("{:016x}", chunk_fingerprint(&chunk)), "elapsed_ns": elapsed_ns,
            "blocks": blocks, "spawn_support": [spawn.support.x, spawn.support.y, spawn.support.z], "spawn_feet": spawn.feet().to_array(),
        }))?
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunk_argument_validates_signed_coordinates_and_extent() {
        assert_eq!(parse_chunk("-1,2,-3").unwrap(), ChunkPos::new(-1, 2, -3));
        for value in ["", "1,2", "1,2,3,4", "x,2,3", "2147483647,0,0"] {
            assert!(parse_chunk(value).is_err());
        }
    }
}
