# Playable SDK demos

Run from the repository root:

```sh
cargo run --release -p rayengine-demos --bin arena
cargo run --release -p rayengine-demos --bin meadow
cargo run --release -p rayengine-demos --bin dungeon
```

Arena and Meadow controls are in the [root README](../../README.md).
All three games use original geometric/pixel art and require no asset downloads.

## Embervault

A short, complete top-down dungeon: carry a lantern through six chambers,
defeat the watchers, solve the counterweight puzzle, cross the burning pits,
and defeat the Warden to recover the stolen ember. Each room has a visible
objective, a sealed eastern gate, and a one-use healing shrine. Clearing its
watchers opens the gate; the counterweight room also needs its block on the
switch. Step through the final gate after defeating the Warden to win.

Enemies announce attacks with red rings. Ordinary watchers stagger when hit;
the Warden commits to its attack, leaving a recovery window afterward. Dash
provides brief protection but does not cross walls or pits. Hold strike to
chain swings. Hits cannot travel through walls. The brass block switch latches
once solved; reset a misplaced block with R / LB.

| Action | Keyboard / mouse | Gamepad 0 (Xbox labels) |
| --- | --- | --- |
| Move | WASD (or arrows via Settings) | Left stick, analog speed |
| Aim | Move mouse / face movement | Right stick / face movement |
| Strike | Hold left mouse / J (or K via Settings) | Hold X |
| Dash | Space | A |
| Use healing shrine | E nearby | Y nearby |
| Reset unsolved block | R | LB |
| Field journal / inventory | Tab | View / Back |
| Pause / back | Escape | Menu / Start or B |
| Navigate menus | Up/Down, Tab, or mouse | D-pad Up/Down |
| Select | Enter or click | A |
| Adjust focused volume slider | Left/Right or drag | D-pad Left/Right |
| Quit | Title menu Quit / window close | Title menu Quit |

Settings apply immediately: music/SFX bus volume, optional CRT scanlines,
WASD/arrow movement presets, and J/K strike binding. Controller and menu
bindings remain available when rebinding. UI draws after effects, so text
stays sharp. Losing focus automatically pauses gameplay. Menus stop the world,
including attack windups, particles, damage grace periods and dash recharge.

Every chamber entrance checkpoints a fresh attempt with six hearts. Continue
and Retry restore that chamber with its enemies and puzzle reset; mid-room
positions and damage are intentionally not saved. Victory clears the active
checkpoint and records a completed descent. Starting a new descent replaces
the checkpoint. Settings persist on leaving their menu and on graceful exit.

Saves use the engine's versioned, checksummed atomic container at
`$XDG_DATA_HOME/embervault/profile.save`, or
`~/.local/share/embervault/profile.save`. Set `RAYENGINE_DUNGEON_SAVE` to select
an isolated slot. Invalid/future saves stop startup with an actionable error
and remain untouched. Write failures are shown in-game and detailed in stderr.

## Systems demonstrated

- Six disk-loaded tilemap levels: solid walls and pits, door and hazard trigger
  zones, spawn metadata, torches, shrines, and a pressure switch.
- `PhysicsWorld2D`: enemy separation, pushable block, swept trigger contacts,
  walls and moving player. A* uses a navigation grid rebuilt from current tiles
  and block positions; opening a gate changes both collision and navigation.
- CPU sprite clips for player/watchers/Warden idle, walk, attack and hurt.
  Bounded particle emitters provide torch sparks, hit bursts and dash dust.
- A game-owned 2D lighting shader combines a moving lantern with four flickering
  torches. This keeps the pixel presentation and demonstrates custom shader
  uniforms/post-processing without extending the engine's 3D light API. It is
  radial lighting, without shadow occlusion. Low health adds a red vignette;
  optional CRT scanlines use the engine's built-in effect.
- Tweens slide gates, flash hurt actors, shake the camera, and animate menus.
- `StateStack`: title → play → pause → journal/settings, dialogue, death/retry,
  and victory/replay. `UiState` supports buttons, focus, pointer dragging and
  controller menus; the original Ember Mono outline font is manifest-loaded.
- Six original streamed music loops crossfade between chambers; hit, dash and
  shrine sounds use the SFX bus. Audio is required to launch (a silent device
  works for CI).
- `rayengine.toml` declares all levels, assets, the custom font and runtime
  defaults. The launcher honors `RAYENGINE_MANIFEST` / `RAYENGINE_PROFILE`.

Art, font, and music source/generation instructions are in
[assets/dungeon/README.md](assets/dungeon/README.md). The Rust implementation is
split into simulation, controls, persistence, presentation and lifecycle modules
under [src/dungeon](src/dungeon).

## Validation and distribution

```sh
# No display/audio device is opened by these gameplay tests.
cargo test --locked -p rayengine-demos dungeon --lib
# Real window, shaders, fonts, sprites and streamed audio; exports PNGs.
cargo test --locked -p rayengine-demos native_dungeon -- --ignored --test-threads=1
# Build a portable Linux folder and archive using the new CLI.
cargo run --locked -p rayengine-cli -- package examples/games --bin dungeon --output artifacts/embervault-bundles
# Package, extract and launch from outside the checkout with an isolated save.
python3 scripts/dungeon_package_smoke.py
```

The CPU tests cover damage/grace/dash/death, wall collision and occluded sword
hits, switch/door rules, pushing/separation, navigation, pit/shrine triggers,
control presets, save corruption/version/range handling, and a complete descent
using ordinary combat/movement inputs, including the block puzzle and Warden.
They need the SDK's native build dependencies but no running display.

The native probe renders the first, puzzle, and final rooms at landscape and
portrait sizes with CRT enabled. Screenshots go to `artifacts/smoke/dungeon-*.png`.
CI runs it and the relocated bundle check from `scripts/native_audio_smoke.sh`,
inside the existing private clocked PulseAudio sink and Xvfb display. The bundle
contains the binary, levels, original audio/art/font, manifest, launcher and
dependency notices; run its `./launch` from any working directory. Linux system
libraries and a working graphics/audio device remain runtime requirements.
