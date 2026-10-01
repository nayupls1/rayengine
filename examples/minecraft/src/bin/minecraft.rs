//! Native seeded terrain preview using the same generation recipe as the CPU tool.
use rayengine::prelude::*;
use rayengine_minecraft::preview::TerrainPreview;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut seed = 42;
    let mut native = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--seed" {
            seed = args
                .next()
                .ok_or("--seed needs a u64 integer")?
                .parse::<u64>()?;
        } else {
            native.push(arg);
        }
    }
    let options = RunOptions::parse(native)?;
    let mut config = Config::new("Minecraft terrain preview");
    config.audio = false;
    App::new(config)
        .with_options(options)
        .run(TerrainPreview::new(seed)?)?;
    Ok(())
}
