# rayengine-core

Display-independent game primitives for [rayengine](https://crates.io/crates/rayengine).
Includes entities/components, transforms, fixed timing, action input, cameras,
viewports, UI interaction, character collision, arcade physics (2D/3D shapes,
layers, triggers and moving platforms), static directional hit geometry,
spatial queries, grid pathfinding, layered routes with links and traffic,
footprint placement, tweens, easing, screen shake, background jobs and saves.
No raylib, native graphics libraries or display are required.

Optional game-owned simulation timelines add pause and speed controls while
preserving fixed timesteps, bounded catch-up and separate presentation timing.

```toml
[dependencies]
rayengine-core = "0.0.3"
```

See the [API reference](https://docs.rs/rayengine-core) and
[repository](https://github.com/nayupls1/rayengine). Rust 1.89 or later is required.
Licensed under MIT.

`collision::Sector2` provides validated static 2D sectors, exact circle
intersection including touching boundaries, and conservative broadphase bounds.
Try the headless example with
`cargo run -p rayengine-core --example directional_hit`. Continuous collision
during a rotating swing, attack windows, damage and visibility remain game rules.

Display-independent [audio buses and gain fades](https://docs.rs/rayengine-core/latest/rayengine_core/audio/)
provide validated master/music/sfx and custom bus controls, transient ducking,
and serializable user settings without initializing an audio device.
