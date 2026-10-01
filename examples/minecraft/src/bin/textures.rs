//! Headless asset-subset inspection; emits one JSON object and writes no images.
use clap::Parser;
use rayengine_minecraft::textures::{TextureSet, Tile};
use std::path::PathBuf;
#[derive(Parser)]
#[command(about = "Validate demo textures from an extracted PNG directory or pack root")]
struct Args {
    /// Explicit local source; omit to inspect built-in original fallback textures.
    #[arg(long)]
    source: Option<PathBuf>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let textures = match args.source {
        Some(path) => TextureSet::load(path)?,
        None => TextureSet::fallback(),
    };
    let atlas = textures.pack();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version":1,"source":atlas.source,"atlas":{"width":atlas.width,"height":atlas.height,"rgba_bytes":atlas.rgba.len()},
            "tiles":Tile::ALL.map(|tile|serde_json::json!({"tile":tile.id().0,"name":format!("{tile:?}"),"source_size":textures.tile(tile).size(),"rect":atlas.rects[tile as usize].to_array()}))
        }))?
    );
    Ok(())
}
