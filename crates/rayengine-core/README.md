# rayengine-core

Display-independent game primitives for [rayengine](https://crates.io/crates/rayengine).
Includes entities/components, transforms, fixed timing, action input, cameras,
viewports, UI interaction, character collision, arcade physics (2D/3D shapes,
layers, triggers and moving platforms), spatial queries, grid pathfinding,
tweens, easing, screen shake, background jobs and saves. No raylib, native
graphics libraries or display are required.

```toml
[dependencies]
rayengine-core = "0.0.2"
```

See the [API reference](https://docs.rs/rayengine-core) and
[repository](https://github.com/nayupls1/rayengine). Rust 1.89 or later is required.
Licensed under MIT.

Display-independent [audio buses and gain fades](https://docs.rs/rayengine-core/latest/rayengine_core/audio/)
provide validated master/music/sfx and custom bus controls, transient ducking,
and serializable user settings without initializing an audio device.
