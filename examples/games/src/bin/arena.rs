//! Run the 2D platform fighter.
use rayengine::prelude::*;
use rayengine_demos::arena::Arena;

fn main() -> Result<(), Error> {
    App::new(Config::new("rayengine / Arena"))
        .with_options(RunOptions::from_env()?)
        .run(Arena::default())?;
    Ok(())
}
