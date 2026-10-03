# Render quality comparison

Recorded 2026-10-02T21:57:39.439095+00:00; release build on 12th Gen Intel(R) Core(TM) i7-12700K, Linux-7.2.5-3-omarchy-x86_64-with-glibc2.44.
Backend: x11 (XWayland display), renderer: NVIDIA Corporation / NVIDIA GeForce RTX 4070/PCIe/SSE2 / 3.3.0 NVIDIA 610.57.04.
Toolchain: cargo 1.98.1 (797e8a9bc 2026-08-05); rustc 1.98.1 (48a229cea 2026-09-01).

Command: `RUSTUP_TOOLCHAIN=1.98.1 python3 scripts/quality_comparison.py --frames 2000 --repeats 5`.
Each profile submitted the same `quality/mixed-static.v1` workload at 1280×720,
with a 960×540 reference view, VSync off and no FPS cap. Each sample includes
initial render target allocation and 2000 frames; profile order rotates per repeat.
Default and 64-pixel custom font atlas, diagonal lines, cube and sphere are identical.

| Mode | World pixels | Estimated targets (MiB) | Frame median (ms) | Per-run range (ms) | Render median (ms) | Present median (ms) | Relative frame wall time |
| --- | --- | ---: | ---: | --- | ---: | ---: | ---: |
| native | 1280×720 | 7.031 | 0.1174 | 0.1083–0.1289 | 0.0629 | 0.0531 | 1.00× |
| fxaa | 1280×720 | 21.094 | 0.1425 | 0.1269–0.1634 | 0.0846 | 0.0586 | 1.21× |
| 2x | 2560×1440 | 42.188 | 0.1532 | 0.1413–0.1988 | 0.0867 | 0.0665 | 1.30× |
| 2x-fxaa | 2560×1440 | 42.188 | 0.1552 | 0.1457–0.2074 | 0.0854 | 0.0702 | 1.32× |

These are uncapped CPU frame wall times, including driver stalls and window
presentation, **not GPU execution timings or expected FPS for a game**. Hidden
windows and this small static workload can leave much of the GPU work asynchronous.
The 2× path shades four times the world pixels even when these CPU timings do not
grow by four. Driver, workload, display, GPU load and clocks affect the result.
Frame, render and presentation medians are taken independently; their medians
need not add exactly. Target estimates exclude font/game assets and driver overhead.

The estimate for native is one RGBA+depth target (7.031 MiB); FXAA adds two native
targets (21.094 MiB). At 2×, four native equivalents of world storage plus UI and
output total 42.188 MiB. At 3840×2160, 2× requests approximately 379.69 MiB;
at 4096×4096, 2× requires 768 MiB and fails the 512 MiB budget explicitly.

The [recorded summary](render-quality-evidence/summary.json) includes repeated
samples, effective settings, actual GL vendor/renderer/version, toolchain, OS, CPU,
environment and the exact clean implementation revision under measurement.
The measured commit integrates project profiles, input, lighting and state APIs;
subsequent documentation/master updates do not change this static workload.
Re-run the script to measure the final checkout.

Screenshots from the identical release workload:

- [Native](render-quality-evidence/native.png)
- [FXAA](render-quality-evidence/fxaa.png)
- [2× supersampling](render-quality-evidence/2x.png)
- [2× + FXAA](render-quality-evidence/2x-fxaa.png)

The native offscreen probe counts grayscale edge coverage pixels (channel values
9–246) in a fixed region containing a diagonal triangle, cube and sphere:

| Physical DPI | Native | FXAA | 2× | 2× + FXAA |
| --- | ---: | ---: | ---: | ---: |
| 1× | 0 | 606 | 254 | 326 |
| 1.25× | 0 | 766 | 414 | 493 |
| 2× | 0 | 1187 | 490 | 620 |

This verifies actual filtered offscreen pixels, independent of window MSAA.
More coverage pixels demonstrate smoothing; the count is not an overall image
quality ranking. Default and custom font patches remain byte-identical across
the four quality modes at each DPI. Additional probes check transparent world
clears, translucent UI, reference-sized point-filtered pixel art, resize, bars,
screenshot pixels, excessive resize rejection and running again after failure.
DPI factors are exercised directly with native GPU targets; a live desktop DPI
transition is not automated by these probes.
