# Minecraft survival demo 0.0.1

Use a Linux desktop compatible with the architecture, libc and shared libraries
recorded in `manifest.json` and `runtime-libraries.txt`. OpenGL 3.3 and an
X11/XWayland display are required for the default bundle. A bundle labelled
`wayland` also enables native Wayland. The binary includes original fallback
textures; no Minecraft installation or Cargo workspace is needed to play.

From the extracted bundle, create and reopen a world:

```sh
./minecraft --seed 42 --save saves/world.save
./minecraft --save saves/world.save
./minecraft --help
./minecraft --version
```

Without `--save`, progress lasts for this session. An existing slot restores its
seed; a conflicting `--seed` or corrupt/incompatible save produces an error and
preserves the slot. Back up important saves before upgrading. Linux saves use
flushed replacement; `--atomic-save` opts into replacement without power-loss
flush guarantees. F5 saves or retries a failure, autosave runs every ten
simulation seconds, and Quit waits for the latest checkpoint. Native close also
attempts a final checkpoint; forced termination can lose unsaved progress.

| Action | Control |
| --- | --- |
| Move / look | WASD / mouse |
| Jump / sprint | Space / Shift |
| Mine / place one held block | Hold LMB / click RMB |
| Hotbar | 1–9 |
| Inventory/crafting | E or Escape |
| Swap inventory slots | Click two slots |
| Navigate/select inventory | Tab/Up/Down; Enter/Space |
| Save/retry / quit | F5 / F10 or Quit button |

Start with an empty inventory. Gather two logs, craft planks and sticks, then a
wooden pickaxe. Mine stone to craft stone tools. Move within two blocks of drops
to collect them. Falling more than three blocks causes damage; the death screen
lets you respawn with inventory retained. Five crack stages show mining progress.

The recipe produces hills, caves, trees and ores at Y=0..127. Streaming keeps at
most 160 chunks resident. Checkpoints hold up to 1,024 edited chunks, 36 inventory
slots and 128 pickups. At the edit-history limit, existing edited regions remain
editable; new histories are blocked before consuming items or altering cells.
Failed saves pin dirty chunks and can pause travel until F5 successfully retries.
Saves record generator/settings/registry identity and strict schema1; migration
is deferred. Texture selection is independent of the saved world.

This minimal demo includes stationary pickups, simple tools/crafting and fall
health. Mobs, liquids, redstone, multiplayer, extensive crafting, hunger,
propagated torch lighting and interactive JSON game automation are deferred.
Windows/macOS are optional source targets; mobile/browser are outside the scope.

Open `reference/rayengine_minecraft/index.html` for the full checked guide and
`reference/rayengine/index.html` for the SDK. The voxel plugin reference is at
`reference/rayengine_voxel/index.html`; plugins are optional Cargo libraries and
the core owns no voxel/game schema. Source and validation instructions live at
https://github.com/nayupls1/rayengine. The manifest pins the source revision.
`THIRD_PARTY_NOTICES/` contains dependency license files and package notices.
`SHA256SUMS` covers bundled files; the adjacent `.tar.gz.sha256` covers the archive.
Only original fallback assets are distributed. Optional imported Minecraft PNGs
stay local and can be selected with `--textures PATH`.
