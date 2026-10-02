//! Native first-person voxel demo using the same generation recipe as the CPU tool.
use rayengine::prelude::*;
use rayengine_core::save::{Durability, SaveOptions};
use rayengine_minecraft::{
    persistence::{LIMITS, Store},
    preview::TerrainPreview,
    textures::TextureSet,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut seed = None;
    let mut save_path = None;
    let mut durability = Durability::Durable;
    let mut native = Vec::new();
    let mut textures = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            print!("{}", include_str!("../../usage.txt"));
            return Ok(());
        } else if arg == "--version" || arg == "-V" {
            println!("rayengine-minecraft {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        } else if arg == "--seed" {
            seed = Some(
                args.next()
                    .ok_or("--seed needs a u64 integer")?
                    .parse::<u64>()?,
            );
        } else if arg == "--save" {
            save_path = Some(args.next().ok_or("--save needs a slot path")?);
        } else if arg == "--atomic-save" {
            durability = Durability::Atomic;
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
    if durability == Durability::Atomic && save_path.is_none() {
        return Err("--atomic-save requires --save PATH".into());
    }
    let textures = textures.unwrap_or_else(TextureSet::fallback);
    let game = if let Some(path) = save_path {
        TerrainPreview::with_save(
            seed,
            textures,
            Store::open(
                path,
                SaveOptions {
                    durability,
                    limits: LIMITS,
                },
            )?,
        )?
    } else {
        TerrainPreview::with_textures(seed.unwrap_or(42), textures)?
    };
    let outcome = game.save_outcome();
    App::new(config).with_options(options).run(game)?;
    outcome.check()?;
    Ok(())
}
