# Agent workflow and CLI contract

Build or install `rayengine-cli`, then create, inspect and check game projects:

```sh
rayengine --json doctor
rayengine --json new ../agent-game --kind 2d --sdk-path /absolute/path/to/rayengine/crates/rayengine
rayengine --json new-plugin ../agent-game/plugins/my-plugin --name my-plugin
rayengine --json info ../agent-game
rayengine --json check ../agent-game
rayengine --json build ../agent-game --release
rayengine run ../agent-game -- --frames 60 --screenshot artifacts/frame.png
```

`--json` emits one JSON object on stdout. Results use schema version 1:

```json
{"schema_version":1,"ok":true,"command":"new","data":{"kind":"2d","name":"agent-game","path":"/absolute/path/agent-game"}}
```

Operational failures use a structured error and process exit code 1:

```json
{"schema_version":1,"ok":false,"command":"check","error":{"code":"cargo_failed","message":"cargo check failed","details":{"exit_code":101,"diagnostics":[],"stdout":"","stderr":"..."}}}
```

`new-plugin` creates a standalone Cargo library, returning `command:
"new-plugin"`, `data.kind: "plugin"`, `data.sdk_path` (null for the registry default, or the resolved local SDK path), and a file list
containing `src/lib.rs`. Like `new`, it refuses existing destinations and accepts
`--name` and `--sdk-path`. Add it to the game with an explicit Cargo path
dependency; the command does not edit the game or register hooks. Generated game
workspaces exclude the `plugins/` directory so nested standalone libraries can
be dependencies. For other workspaces, exclude the plugin's path or remove its
`[workspace]` table and add it as a member. See the [plugin guide](crate::guides::plugins).

Argument errors return exit code 2 and `error.code = "invalid_arguments"` in
JSON mode. Successful commands return exit code 0. Help and version requests
remain normal textual CLI output. Do not determine success from an empty stderr;
check the exit code and `ok` field.

Cargo's JSON diagnostic objects are preserved in `data.diagnostics` on success
and `error.details.diagnostics` on failure. Child stderr is captured inside the
result instead of corrupting JSON stdout. `info` returns Cargo package metadata;
`doctor` reports installed tools and native header availability. Doctor is a
prerequisite presence check, not proof that a driver or audio device works.

In JSON mode, `run` captures game stdout/stderr until it exits, then returns one
result. Use a bounded `--frames` argument when asking an agent to run it. Normal
text mode streams the process interactively. This is command completion output,
not a live transport for injecting gameplay commands.

Game scaffolds include `RunOptions::from_env`. Bounded rendering controls are:

- `--frames N`: exit after N **render frames**, not N fixed simulation ticks.
- `--screenshot file.png`: save the final displayed framebuffer as PNG.
- `--size WIDTHxHEIGHT`: request initial logical window dimensions.
- `--hidden`: hide the native window; a display and graphics context are still required.
- `--uncapped`: disable VSync and render throttling.

A tiling window manager can override the requested window size. Camera fitting
uses the actual size reported by the backend. Native smoke probes use hidden
windows to make scripted resize checks predictable.

For gameplay assertions, expose your own simulation structs/functions and test
them with `rayengine-core::Input` and a fixed dt. Arena and Meadow demonstrate
that the same step is used in unit tests, benchmarks and interactive play. These
functions are internal Rust test access, not an external game protocol.

The proposed pause → input → step → image/state → input workflow is tracked in
[issue #1](https://github.com/nayupls1/rayengine/issues/1) and is deliberately not
implemented in 0.0.1. Its event registration, schema, transport, screenshot
synchronization and repeatability contract need separate design work.
