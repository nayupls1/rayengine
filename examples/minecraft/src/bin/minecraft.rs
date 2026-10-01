//! Native first-person voxel demo using the same generation recipe as the CPU tool.
use rayengine::prelude::*;
use rayengine_minecraft::{preview::TerrainPreview, textures::TextureSet};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut seed = 42;
    let mut native = Vec::new();
    let mut textures = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--seed" {
            seed = args
                .next()
                .ok_or("--seed needs a u64 integer")?
                .parse::<u64>()?;
        } else if arg == "--textures" {
            textures = Some(TextureSet::load(
                args.next()
                    .ok_or("--textures needs an extracted PNG directory or pack root")?,
            )?);
        } else {
            native.push(arg);
        }
    }
    let options = RunOptions::parse(native)?;
    let mut config = Config::new("Minecraft voxel demo");
    config.audio = false;
    config.exit_key = None; // Escape belongs to the inventory; F10/close/quit exits.
    App::new(config)
        .with_options(options)
        .run(TerrainPreview::with_textures(
            seed,
            textures.unwrap_or_else(TextureSet::fallback),
        )?)?;
    Ok(())
}
