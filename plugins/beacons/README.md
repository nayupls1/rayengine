# rayengine-beacons

A small optional example plugin: independently configured spinning beacons use
shared game-owned ECS state, generated mesh assets, and the public SDK lifecycle.
The engine has no dependency on this crate. It is unpublished; use a Cargo path
dependency from this checkout.

```toml
[dependencies]
rayengine = { path = "../rayengine/crates/rayengine" }
rayengine-beacons = { path = "../rayengine/plugins/beacons" }
```

Run the composed game with `cargo run -p rayengine-beacons --example composition`.
Space pauses both instances; Escape exits. Standard runner controls work,
including `--frames`, `--size`, `--screenshot`, and `--diagnostics`.

[`BeaconWorld`] belongs to the game. Each [`Beacon`] owns one scene entity and
one generated mesh; two instances never scan or mutate each other's entities.
The game calls [`rayengine::Plugin`] hooks in order and handles init errors.
CPU simulation is separately callable with [`Beacon::step`]. The example opens
one world pass per plugin to keep it small; a larger plugin can expose drawing
methods that share a caller-owned canvas.

Keep an initialized beacon paired with its original world and run's assets.
Initialization rejects duplicates and uploads geometry before spawning state.
Unload is idempotent, removes only this instance's entity/mesh, and allows reinit.
Dropping a beacon alone does not remove its handles from their owners. On normal
exit or an init error propagated from the game, all game fields and then SDK
assets are dropped while the native context still exists.

The supported SDK requirement is the version in this crate's Cargo manifest.
There is no runtime ABI/version negotiation. Match the same SDK source/version
as the consuming game; distinct SDK copies create distinct Rust types.

## Complete composition example
