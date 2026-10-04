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
implemented in 0.0.2. Its event registration, schema, transport, screenshot
synchronization and repeatability contract need separate design work.

## Full project lifecycle

```sh
rayengine templates
rayengine new ../topdown --template topdown --sdk-path /path/to/crates/rayengine
rayengine add particles ../topdown --features render
rayengine remove particles ../topdown
rayengine watch ../topdown --profile dev -- --hidden
rayengine --json watch ../topdown --cycles 2 --timeout-ms 30000
rayengine --json package ../topdown --profile pixel --output ../releases
rayengine clean ../topdown --output ../releases
rayengine doctor --fix-hints
```

`new --template` accepts `2d`, `3d`, `topdown`, and `platformer`. `--kind` remains
compatible for the original starters; specifying both options is an argument
error. The tilemap starters load `assets/level.toml`, draw primitive tiles without
textures and use `Body2D` collision; the platformer adds gravity/jumping. Their
`new` result keeps the existing `kind`, `name`, `path`, `sdk_path`, `files` schema.
All templates compile in `scripts/template_smoke.py`. Some first-party plugins
are not yet published; use a repository SDK path for those starters until their
matching plugin versions are published.

`add <plugin> [path]` and `remove <plugin> [path]` accept particles, voxel, beacons,
and tilemap (also `rayengine-` prefixed names). Registry dependencies pin the SDK's
concrete version; a local SDK infers `../../plugins/<name>`. `--plugin-path` selects
another local plugin with the same SDK source/version; `--features a,b` enables
optional Cargo features. Edits preserve TOML comments and existing namespaces.
Repeated add/remove return explicit errors. Remove deletes that plugin namespace
from both base and profiles, leaving other plugins alone. It also removes Cargo
feature references to the removed dependency (including aliases and forwarding),
while retaining authored feature names. Cargo validates the result; failure
restores the original manifest. Commands edit manifests;
they do not register hooks, invoke plugin code or fetch/build the new dependency.
Generated libraries remain explicit game-owned compositions.

`package` (`bundle` alias, JSON command always `package`) builds with Cargo
`--release`; `--profile` selects a **project** profile to bake into the bundled
manifest. `--bin` overrides its executable; otherwise Cargo's `default-run` or the
only binary is selected. `--features a,b` enables Cargo features. Choose a package
manifest rather than a virtual workspace root. Outputs default to the package's
`bundles/` directory. Existing folders/archives are refused; failures remove only
this invocation's reserved outputs. The archive contains `bin/game`, executable
`launch`, a relocatable `rayengine.toml`, discovered assets (with exclusions/root
precedence), declared fonts, project license/notice files, dependency notices,
`runtime-libraries.txt`, `README.txt`, and `.rayengine-bundle.json` provenance.
Run `./launch [args]` after extraction. Selected profiles are already merged;
launcher clears inherited profile overrides and sets the bundled manifest path.
Generated starters also discover the sibling bundle manifest when run directly.
Games must opt into the shared manifest loader and asset lookup; game-owned
absolute paths in extension tables are not rewritten. Missing optional asset
roots stay empty; missing declared fonts fail packaging. Font files are included
independently of discovery exclusions.

Linux bundles require the runtime libraries listed by `ldd` and a graphics display
for SDK games. Typical X11 dependencies: glibc, libgcc, libX11, libXrandr,
libXinerama, libXcursor, libXi, OpenGL/Mesa and ALSA. Wayland builds also require
Wayland and xkbcommon. Build on the oldest supported glibc baseline; system
libraries are not bundled. Packaging requires the native build prerequisites,
`tar` (gzip support) and `ldd`. `scripts/cli_package_smoke.py` builds a real starter,
deletes its sources, extracts outside the checkout and runs the launcher with
bounded frames and a PNG assertion. `--archive file.tar.gz` runs the same probe
without Cargo; CI uses a fresh Ubuntu container with only documented runtime
libraries, Python, Xvfb and archive tools.

`watch` polls content changes every 50 ms and debounces for 200 ms by default
(`--debounce-ms N`). It watches the package, local Cargo dependencies, declared
asset roots/fonts and project manifest. Workspace Cargo manifests/lockfiles,
workspace Cargo configuration and toolchain declarations are also watched,
including virtual workspaces. It skips symlinks, `.git`, `target`,
`artifacts`, and `bundles` to avoid feedback loops. Each cycle stops the old game,
builds, and starts the selected binary with the same manifest/profile environment
as `run`. Failed builds keep watching so the next edit can recover. A game exit
waits for another edit. Ctrl-C stops owned children; on Unix this includes their
process groups. `--cycles N` counts the initial build and stops immediately after
the Nth cycle; `--features a,b` enables Cargo features. `--timeout-ms N` limits the watch loop/build waits. Initial Cargo
metadata resolution occurs before this timer. Interactive mode reports each cycle
on stderr. JSON mode captures children and returns **one completion object** on
termination, retaining the latest 32 cycles and total count. It emits no live
JSON events. Use a bound or Ctrl-C to obtain the result; a failed final build/game
returns `watch_failed`, while an interrupted build is a successful user stop.

`clean` runs Cargo clean (the workspace's shared target directory may be removed)
and deletes only marked CLI bundles belonging to this package, plus their
archives. Unmarked directories/files remain. A custom package `--output` must be
passed again to clean that location. `doctor --fix-hints` includes/prints commands
for Arch, Debian/Ubuntu, Fedora and openSUSE; unknown distributions get a generic
hint. **No installation command is executed.** Missing prerequisites still exit 1
with hints under `error.details.fix_hints`.

## Schema-1 lifecycle result fields

All results retain the common envelope above. Fields below are stable; absolute
paths serialize as strings, absent options as null, lists as arrays.

| Command | Success `data` fields | Operational `error.code` and details |
| --- | --- | --- |
| `templates` | `templates: [{name, description, plugins: [string]}]` | Only argument errors (exit 2) |
| `new --template` | Existing `new` fields; `kind` is the template name; `files` includes generated level | Existing creation/SDK errors; plugin errors below for tilemap starters |
| `add`, `remove` | `manifest`, `project_manifest` (file path, or null for remove without a project manifest), `plugin`, `dependency`, `plugin_path` (path or null), `features: [string]` | `unknown_plugin`, `plugin_already_added`, `plugin_not_added`, `invalid_plugin`, `plugin_sdk_mismatch`, `missing_sdk`, `invalid_sdk`, `invalid_cargo_manifest`; details null |
| `package`, `bundle` | `manifest`, `folder`, `archive`, `binary`, `profile`, `release: true`, `assets: [relative path]`, `runtime_libraries: string`, `notices: [{name, version, license, repository, files}]`, `diagnostics: [Cargo object]`, `stderr: string` | `unsupported_platform`, `invalid_package`, `invalid_binary`, `bundle_exists`, `missing_executable`, `invalid_assets`, `runtime_libraries_failed`, `archive_failed`; details null. `cargo_failed` retains Cargo diagnostics/stdout/stderr/exit_code |
| `watch` | `manifest`, `binary`, `profile`, `release`, `features`, `cycle_count: integer`, `cycles: [cycle]`, `stopped: signal/cycle_limit/timeout` | `watch_failed`: details is the completion data for a failed final cycle; setup failures have null details. `invalid_binary` and Cargo setup failures use their usual schemas |
| `clean` | `manifest`, `removed_bundles: [absolute path]`, `stdout`, `stderr` | `cargo_failed` with Cargo details |
| `doctor --fix-hints` | Existing `engine_version`, `platform`, `checks`; `fix_hints: [string]` (empty without flag) | `missing_prerequisites`: details has all doctor data, including hints |

A watch cycle contains `cycle: integer`, `changed: [absolute path]`, `ok: bool`.
A successful build adds `binary`, `diagnostics`, `build_output: {stdout, stderr}`,
`game_output: {stdout, stderr}` (or null before completion) and `game_exit_code`
(null for running/stopped games). Failure adds `error: {code, message, details}`;
compiler failure details contain stdout/stderr with Cargo diagnostics also in the
cycle's `diagnostics`. Game failures use `game_failed` with null details. Stopped
builds use `watch_interrupted`. Earlier failures remain in the bounded history
but do not fail a successfully recovered final cycle.

Shared errors for commands that read projects: `missing_manifest`,
`invalid_project_manifest`, `invalid_metadata`, `invalid_package`, `cargo_failed`,
`process_failed`, `io_failed`. Non-Cargo errors have null details unless specified
above. Argument failures have `command: null`; commands and aliases parsed
successfully always use their canonical command name. Profiles are validated
before builds; new/templates/add/remove/clean/doctor have no runtime profile flag.
CLI process tests cover every new command's success and failure envelope,
manifest preservation, packaging relocation, debounce/restart and hint capture.
