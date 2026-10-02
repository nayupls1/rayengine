# Optional project manifests

`Cargo.toml` describes Rust builds. `rayengine.toml` describes project identity,
asset discovery, runtime defaults and open game/plugin settings. Existing games
continue to work without a manifest; loading one is opt-in in Rust. `rayengine new`
generates an example and a starter that loads it.

A minimal manifest is just:

```toml
schema_version = 1
```

The schema version is an integer independent of the SDK version (this format is
introduced for 0.0.2). It is required when the file exists. Supported additive
fields will be documented as the SDK evolves; unknown engine keys fail rather
than being silently ignored. Breaking changes require a new schema version.
Package distribution is reserved for future tooling.

## Schema and defaults

The following complete example is checked by the parser tests. All sections are
optional. These engine values show the defaults used by CLI inspection and
`Config::new("rayengine game")`:

```toml
schema_version = 1

[project]
name = "Example game"           # optional identity; does not set window.title
executable = "example-game"    # optional Cargo binary target for rayengine run

[assets]
roots = ["assets"]
exclude = ["**/*.bak", "private"] # default: []

[window]
title = "rayengine game"
size = [1280, 720]

[render]
reference_size = [960.0, 540.0]
scale_mode = "fit"             # fit | expand | integer_fit
vsync = true
target_fps = 120               # 0 means no cap
bar_color = [9, 14, 24, 255]    # RGBA bytes

[runtime]
fixed_hz = 120
max_catch_up = 8
audio = false

[fonts.body]
path = "assets/fonts/body.ttf" # required in every effective declaration
raster_size = 32               # default 32, 1..=512
filter = "linear"              # linear (default) | nearest
glyphs = [32, 65, 66, 67]       # optional nonempty Unicode scalar list

[package]
# Reserved opaque table. This release neither packages nor validates its keys.

[game]
difficulty = "normal"          # arbitrary TOML values validated by your game

[plugins.weather]
rain = true                    # arbitrary table validated by this plugin

[profiles.dev.render]
vsync = false

[profiles.pixel.render]
scale_mode = "integer_fit"

[profiles.pixel.fonts.body]
filter = "nearest"             # retains body.path, raster_size and glyphs

[profiles.dev.game]
difficulty = "easy"
```

Window and reference dimensions must be in `1..=8192`; reference values must
also be finite. `target_fps` is `0..=1000`, and `fixed_hz`/`max_catch_up` are
`1..=1000`. Titles cannot contain NUL. Project names must be nonblank. Profile,
font, plugin and executable names contain only ASCII letters, digits, `_`, `-`.
Paths must be nonempty UTF-8 without NUL. Colors must be four byte values.

`render.reference_size`, `scale_mode`, `target_fps`, `vsync`, `bar_color` name
existing runtime controls. `fonts.<name>.path`, `raster_size`, `filter`, `glyphs`
are the shared contract for the custom-font work (#47). Declarations do not yet
create font resources. The rendering-quality work (#48) can extend `render`
with supported quality controls; `render_scale` and `anti_aliasing` are currently
unknown engine settings and fail. A manifest never promises unsupported GPU
behavior.

## Profiles and precedence

Select zero or one named profile. Profiles independently override the shared
base, never other profiles. Tables merge recursively by key, including font
names and game/plugin tables. Arrays and scalars replace; arrays never append.
There is no deletion/null syntax. `roots = []` explicitly disables discovery.
Profile sections may override assets, window, render, runtime, fonts, package,
game and plugins. Project identity cannot be overridden in a profile.

Precedence is:

1. Rust `Config` supplied by the caller (or SDK defaults for CLI inspection).
2. Fields explicitly declared in the shared manifest base.
3. Fields explicitly declared in the selected manifest profile.
4. `RunOptions` overrides, such as `--size` and `--uncapped`.

Omitted fields preserve custom Rust configuration. CLI inspection reports
fully defaulted manifest settings; it cannot infer defaults from arbitrary game
code. For matching effective values, use `Config::new("rayengine game")`. The
manifest does not configure run controls such as screenshots or frame limits.
`project.executable` selects the CLI run target; CLI `--bin` overrides it.
`--release` selects a Cargo build profile and is independent of `--profile`.

## Loading and paths

```rust,no_run
use rayengine::{Config, manifest::ProjectManifest};

// Anchor to the source project rather than the process's working directory.
let file = concat!(env!("CARGO_MANIFEST_DIR"), "/rayengine.toml");
let manifest = ProjectManifest::load(file)?;
let project = manifest.resolve(Some("dev"))?;
let config = Config::new("rayengine game").with_project(&project)?;
let files = project.discover_assets()?;
// During Game::init: context.texture(project.asset("player.png")?)?
// The context still owns native resource creation on the render/audio thread.
# Ok::<(), Box<dyn std::error::Error>>(())
```

`ProjectManifest::load_optional` and `Config::with_optional_project` accept a
project directory, Cargo manifest, or explicit `rayengine.toml`. Directory/Cargo
inputs look only for a sibling `rayengine.toml`, without searching parents. A
missing optional sibling is allowed; an explicit missing file is an error. A
profile requires a manifest. The CLI accepts the same inputs for info, check,
build and run, and uses the sibling Cargo manifest for the build.

The manifest file is canonicalized once; relative asset roots and font paths
are joined to its directory. Absolute paths stay absolute. Optional asset files
need not exist at inspection time. Joined paths preserve `..` components, so
symlink semantics are consistent with the OS; no cwd-dependent rewriting occurs.
Font paths are manifest-relative, independent of asset roots and exclusions.

`discover_assets` returns regular files sorted within each root in declaration
order. `asset("textures/player.png")` searches the same roots and returns the
first selected match. Duplicate logical names use the first root. Missing roots
are allowed, other discovery I/O errors are reported. Links within roots are
skipped by discovery and lookup (the root itself may be a link). Lookup names
must be relative and contain no `.`/`..` components. Neither API loads textures,
models, audio or font atlases. Existing explicit native asset-loading APIs still
accept explicit paths.

Exclusions use case-sensitive globset syntax relative to each asset root, with
`/` separators. `*`/`?` do not cross directory separators, `**` does. A matching
directory excludes its subtree. For example `private` excludes the private
folder, and `**/*.bak` excludes backup files at any depth. Empty, absolute,
parent-traversing, backslash-containing or malformed globs fail validation.

## CLI and extensions

```sh
rayengine --json info /path/to/game --profile dev
rayengine --json check /path/to/game/rayengine.toml --profile pixel
rayengine run /path/to/game --profile dev --bin example-game -- --size 800x600
```

JSON info includes `project_manifest` (null for manifest-free projects) with
canonical path, selected profile and resolved/defaulted settings, and `profiles`
with the available names. Check/build/run validate the entire manifest before
starting Cargo. All profiles are validated even when unselected. Errors include
the file and setting/profile context; syntax errors include parser locations.
CLI errors use `invalid_project_manifest` in the existing JSON result envelope.
Unknown versions, unknown engine keys, unknown profiles, invalid enums/ranges
and invalid extension table shapes fail early. Asset existence is checked when
looking up/discovering/loading, not when parsing declarations.

CLI run passes `RAYENGINE_MANIFEST` and `RAYENGINE_PROFILE` to the game. Generated
starters use these, or fall back to their compiled `CARGO_MANIFEST_DIR`; direct
Cargo runs load the base unless `RAYENGINE_PROFILE` selects another profile.
Existing games opt in by explicitly loading the manifest and applying it to
Config; CLI run cannot modify a game's Rust configuration by itself. Game
arguments after `--` retain their existing meaning and are not rewritten.

`settings.game` and `settings.plugins["weather"]` retain arbitrary TOML values.
Use `table.clone().try_into::<YourConfig>()` to deserialize a plugin's own typed
configuration; that plugin validates it. Use serde `deny_unknown_fields` if the
plugin wants strict keys. There is no engine registry, automatic plugin
initialization, or automatic filesystem interpretation inside these namespaces.
`package` is similarly opaque and reserved for later packaging tooling.
