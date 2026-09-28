# rayengine

A Linux-first Rust game SDK and CLI built on raylib. No editor: games are
ordinary Cargo projects, with shared conventions for 2D and 3D.

Version 0.0.1 is being built around fixed simulation timing, responsive
viewports, typed entities/components, simple collision and playable examples.
See [the architecture](docs/architecture.md).

```sh
cargo check --workspace
cargo doc --workspace --no-deps
```

Rust 1.88+, CMake, a C compiler, libclang, and Linux graphics/audio development
libraries are required for the raylib SDK. The `rayengine-core` crate can be
built and tested independently without graphics dependencies.

The [interactive agent testing protocol](https://github.com/nayupls1/rayengine/issues/1)
is tracked separately and deferred beyond 0.0.1.
