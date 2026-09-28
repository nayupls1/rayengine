//! Run the 3D exploration platformer.
use rayengine::prelude::*;
use rayengine_demos::meadow::Meadow;

fn main() -> Result<(), Error> {
    App::new(Config::new("rayengine / Meadow"))
        .with_options(RunOptions::from_env()?)
        .run(Meadow::default())?;
    Ok(())
}
