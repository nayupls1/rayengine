# Example artwork

`sprites.png` is an original, hand-authored 128×16 RGBA pixel-art sheet created
for rayengine. It contains eight 16×16 frames, left to right:

- 0–1: idle/breathing.
- 2–3: walking.
- 4–7: sword swing.

The character faces right; the game flips source sampling when facing left.
Transparent pixels preserve the scene background. The artwork is distributed
under the SDK's MIT license (see `../../LICENSE`), including
permission to redistribute and modify it in games. No third-party artwork,
downloads, generated cache files or proprietary assets are required.

`character.glb` is an original low-poly lamplighter generated deterministically
by `scripts/generate_character_model.py` (run it from the repository root to
rebuild the file and the test fixtures). It has a seven-bone armature, rigid
vertex skinning and three clips sampled for glTF's 60 Hz import rate: `idle`
(2 s loop), `walk` (1 s loop) and `wave` (1.6 s one-shot). It is distributed
under the same MIT license.
