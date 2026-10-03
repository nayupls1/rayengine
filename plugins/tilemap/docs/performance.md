# Tile submission comparison

Run `cargo bench --locked -p rayengine-tilemap --bench tilemap` or include it in
`scripts/benchmark.sh save NAME`. Criterion compares a fully occupied 512×512
map (262,144 cells, one layer, 16×16 chunks, 32-unit tiles) using the same
`black_box((tile_id, bounds))` submission callback. Construction/filling is
outside timing. The naive loop visits/submits every occupied cell; the culled
path selects chunks with the camera AABB and tests cells with the engine's
rotated-camera tests. This
measures CPU traversal and submission preparation, not GPU/frame time.

The camera is centered at (8192,8192), has 540 world-unit height and a 16:9 Fit
viewport. Only 4 chunks and 576 cells are submitted, compared with 262,144 naive
submissions. The benchmark asserts fewer than 20 visible chunks and 1024 cells;
unit tests compare the visitor with brute-force frustum filtering for rotation,
portrait/wide windows and all three viewport policies.

A local run on an Intel Core i7-12700K, Linux x86_64, Rust 1.99.0, workspace
release profile (thin LTO), Criterion 0.8.2, 20 samples, 300 ms warmup and 1 s
measurement produced these mean estimates:

| Traversal | Mean | 95% confidence interval |
| --- | ---: | ---: |
| Naive full map | 484.57 µs | 482.53–486.96 µs |
| Culled chunks | 11.68 µs | 11.64–11.74 µs |

The full workspace and native suites had finished; targeted native/MSRV checks
ran alongside this measurement. The roughly 41× local reduction is
illustrative, not a portable performance guarantee. Criterion artifacts in
`target/criterion` are regenerated locally; compare baselines on the same
machine/settings. The raw mean estimates are retained in `measurements.json`.

Culling visits only chunks in a conservative camera AABB, then applies rotated
rectangle tests to cells. Chunk selection includes a world-coordinate rounding
margin; it avoids oriented chunk rejection because f32 SAT can disagree between
a chunk and its contained cells near the map's precision limit. Visible nonempty chunks each scan at most 256 cells. Empty
chunks are skipped immediately. Layer ordering is stable and source regions
stay in one cached atlas; the SDK/raylib sprite path batches ordinary draws.
Runtime edits update cell occupancy immediately and allocate no new chunks.
Character movement queries the swept region instead of the whole map; raycasts
currently scan chunk headers and reject off-ray chunk boxes before testing
cells. External spatial index rebuilds are explicit snapshots and include all
solid cells; callers can use the grid's regional geometry without a rebuild.
