# rayengine-cli

Project scaffolding and Cargo tools for rayengine games. The installed binary
is named `rayengine`.

```sh
cargo install rayengine-cli --version 0.0.1 --locked
rayengine doctor
rayengine new my-game --kind 2d
rayengine check my-game
rayengine run my-game
```

Use `--kind 3d` for a first-person starter, or `new-plugin` to create a plugin
library. Starters depend on the matching published SDK version by default.
For engine development, supply `--sdk-path /path/to/rayengine/crates/rayengine`.
The CLI also supports `info`, `build`, and `--json` structured responses.

Rust 1.89 or later is required. The CLI itself has no native graphics dependency;
compiling or running games requires the SDK's Linux build/runtime prerequisites.
See the [quickstart](https://docs.rs/rayengine/latest/rayengine/guides/quickstart/index.html)
and [CLI contract](https://docs.rs/rayengine/latest/rayengine/guides/agent_workflow/index.html).
Licensed under MIT.
