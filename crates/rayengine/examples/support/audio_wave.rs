//! Original procedural mono WAV assets, generated without external downloads.
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

/// Temporary asset directory, removed after the runner releases its streams.
pub struct GeneratedAudio {
    pub directory: PathBuf,
}
impl GeneratedAudio {
    /// Generates two four-second loops and a short effect in a unique directory.
    pub fn new() -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "rayengine-audio-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory)?;
        let assets = Self { directory };
        write_tone(&assets.directory.join("calm.wav"), 220.0, 4.0)?;
        write_tone(&assets.directory.join("bright.wav"), 330.0, 4.0)?;
        write_tone(&assets.directory.join("click.wav"), 660.0, 0.25)?;
        Ok(assets)
    }
}
impl Drop for GeneratedAudio {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
fn write_tone(path: &Path, hz: f32, seconds: f32) -> io::Result<()> {
    let rate = 22_050_u32;
    let frames = (rate as f32 * seconds) as u32;
    let size = frames * 2;
    let mut file = std::fs::File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + size).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?; // PCM
    file.write_all(&1_u16.to_le_bytes())?; // mono
    file.write_all(&rate.to_le_bytes())?;
    file.write_all(&(rate * 2).to_le_bytes())?;
    file.write_all(&2_u16.to_le_bytes())?;
    file.write_all(&16_u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&size.to_le_bytes())?;
    for frame in 0..frames {
        let time = frame as f32 / rate as f32;
        let envelope = (time * 25.0).min(1.0) * ((seconds - time) * 25.0).min(1.0);
        let tone = (time * hz * std::f32::consts::TAU).sin()
            + 0.3 * (time * hz * 1.5 * std::f32::consts::TAU).sin();
        let sample = (tone * envelope * 4_000.0) as i16;
        file.write_all(&sample.to_le_bytes())?;
    }
    Ok(())
}
