# Getting started

Install Rust **1.89 or later**, CMake, a C compiler, libclang and the native
graphics/audio development headers. Raylib's C sources and GLFW are built by
the Rust dependency; a separate raylib installation is unnecessary.

On Arch Linux:

```sh
sudo pacman -S --needed base-devel cmake clang pkgconf libx11 libxrandr libxinerama libxcursor libxi mesa alsa-lib
```

On Debian/Ubuntu:

```sh
sudo apt-get install build-essential cmake clang libclang-dev pkg-config libasound2-dev libudev-dev libx11-dev libxrandr-dev libxinerama-dev libxcursor-dev libxi-dev libgl1-mesa-dev
```

Build and inspect prerequisites from the repository:

```sh
cargo run -p rayengine-cli -- doctor
cargo run -p rayengine-demos --bin arena
cargo run -p rayengine-demos --bin meadow
```

The default desktop backend is X11, also usable through XWayland. To compile
native Wayland, install its development libraries and enable the feature:

```sh
# Arch: wayland wayland-protocols libxkbcommon
# Debian/Ubuntu: libwayland-dev wayland-protocols libxkbcommon-dev libegl1-mesa-dev
cargo run -p rayengine-demos --features rayengine/wayland --bin meadow
```

The SDK's `wayland` feature enables the bundled GLFW Wayland backend directly,
with X11 still available as a fallback. It avoids raylib-rs's `wayland` alias,
which links a system GLFW library without enabling the bundled Wayland backend.
The engine's minimum Rust version includes hecs 0.11's const-generic inference
requirement, newer than the minimum advertised in its package metadata.

Install the published CLI and create a game:

```sh
cargo install rayengine-cli --version 0.0.1 --locked
rayengine doctor
rayengine new ../my-game --kind 2d
# For 3D, use --kind 3d.
cargo run --manifest-path ../my-game/Cargo.toml
```

Scaffolding creates a new directory and refuses to overwrite anything already
there. Generated projects depend on `rayengine = "0.0.1"` from crates.io by
default. An ordinary Cargo project can also add that dependency directly.

For development before publication, build/install the CLI from this checkout
and explicitly use the local SDK:

```sh
cargo install --path crates/rayengine-cli --locked
rayengine new ../another-game --kind 3d --sdk-path "$PWD/crates/rayengine"
```

A project created with `--sdk-path` requires that checkout to remain available;
change its dependency to a registry version before sharing it.

The CLI itself does not link raylib. Creating and inspecting projects works
without a display. Compiling games requires the native prerequisites; running
them requires a graphics context.

Build the SDK reference and these guides as HTML:

```sh
cargo doc --workspace --no-deps
# Entry point: target/doc/rayengine/index.html
```

Use `--open` to view locally if desired. Markdown guides are included in rustdoc,
so examples compile with the SDK instead of drifting into a separate website.
There is no MDX or JavaScript build requirement.
