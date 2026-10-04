# Publishing rayengine to crates.io

The manually dispatched **Publish crates** workflow publishes `rayengine-core`,
`rayengine`, `rayengine-voxel` and `rayengine-cli` together. Examples and beacons
remain repository-only. The CLI installs a binary named `rayengine`.

## One-time setup

- [ ] Sign in to crates.io with GitHub and verify the account email.
- [ ] Confirm availability/ownership of all four crate names.
- [ ] Create a crates.io API token with **publish-new** and **publish-update**
  scopes. Restrict its crate names to the four release packages and choose an
  expiry. Ownership, yanking and Trusted Publisher management scopes are not
  needed by this workflow.
- [ ] Add the token as repository Actions secret **CARGO_REGISTRY_TOKEN** at
  **Settings → Secrets and variables → Actions → New repository secret**.
- [ ] Merge the release preparation into `master`. GitHub displays manual
  workflows once their definition exists on the default branch.

No repository `.env` file or GitHub personal access token is needed. The job maps
the Actions secret to `CARGO_REGISTRY_TOKEN` only in credential/publishing steps.
Preparation and CI run without this secret. See the
[Cargo publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html).

## Every release

- [ ] Update `[workspace.package].version` and the internal versions in
  `[workspace.dependencies]` together. All four published crates share a version.
- [ ] Update README/quickstart installation examples and the changelog. CLI
  scaffolds use the CLI's own package version automatically.
- [ ] Review the public API, platform support, license and package contents.
- [ ] Commit and merge the release version into `master`.
- [ ] Open **Actions → Publish crates → Run workflow**, choose `master`, enter the
  committed version (for example `0.0.3`), and keep **dry_run** checked.
- [ ] Check all jobs passed. Download the `crate-candidates` artifact to inspect
  the four `.crate` archives and `SHA256SUMS` if desired.
- [ ] Run the same workflow for the same commit/version with **dry_run** unchecked
  to upload. Actual publication is restricted to `master`.
- [ ] Verify all four crates.io pages, READMEs and docs.rs build results.
- [ ] Tag the exact published commit `v0.0.3` (substitute the release version) and
  create release notes. This workflow does not create tags or GitHub Releases.

The input validates a version already committed in the manifests; it does not
bump versions. Dry runs can also run on a feature branch after the workflow has
been merged into the default branch. The actual run repeats checks on its own
selected commit, so a moving branch cannot reuse checks from another commit.

## What the workflows verify

`ci.yml` runs on pushes, pull requests, manual dispatch and calls from the
publishing workflow. It uses Rust 1.98.1 and Ubuntu 24.04 explicitly, plus a Rust
1.89.0 job for the declared minimum version. Linux jobs install CMake, Clang,
libclang and graphics/audio/Wayland headers through the shared setup action.
There is no GPU requirement: native tests use Xvfb and software Mesa.

The full suite includes formatting, Clippy, CPU/gameplay/CLI tests, rustdoc,
local template/plugin checks, native rendering probes, Wayland compilation and
benchmark compilation. The packaging job adds:

- Coordinated version/metadata checks and tests of release rejection conditions.
- A joint `cargo package --locked` (verification without any upload) for the four intended packages.
- Inspection of license text, READMEs, templates, shaders and archive identities;
  checksums accompany the artifacts.
- CLI installation from its extracted archive, then generation of registry-based
  2D/3D/plugin starters from outside the checkout.
- Fresh consumers on Rust 1.89.0 of the archived SDK and voxel crates, including Wayland and
  voxel rendering. These use temporary Cargo patches to the extracted archives;
  the repository source is not used for the game dependencies.
- SDK documentation with `raylib/nobuild`, matching its docs.rs configuration.
  Voxel's docs.rs configuration covers its default CPU API. Render-feature docs
  are checked separately in the workspace documentation suite.

After upload, the publishing job repeats installed CLI and consumer checks using
actual registry packages, with no local Cargo patches.

Cargo 1.90+ supports publishing/validating multiple dependent workspace crates
together. It verifies all selected packages before uploading and publishes in
dependency order. This handles the first release even when none of the internal
crates exists in the registry yet. See the
[Rust 1.90 announcement](https://blog.rust-lang.org/2025/09/18/Rust-1.90.0/).

## Local preparation

Install the prerequisites from the quickstart, then use Rust 1.98.1:

```sh
scripts/check.sh
python3 scripts/template_smoke.py
python3 scripts/test_release_check.py
python3 scripts/release_check.py --version 0.0.3 --package
python3 scripts/release_smoke.py
xvfb-run -a env LIBGL_ALWAYS_SOFTWARE=1 scripts/native_smoke.sh
```

During uncommitted development only, add `--allow-dirty` to `release_check.py`
when generating candidates. CI uses clean committed sources. Python 3.11+ is
required by the metadata script; archive smoke extraction uses Python 3.12+ (or
3.11.8+ with tarfile extraction filters).

## Failure and future releases

Publishing several crates is not atomic. If an upload or post-publication smoke
check fails, inspect the workflow log and registry before rerunning: some or all
versions may already exist. Do not bump or yank successful versions blindly.
From the exact release commit, publish only missing packages after investigating
the cause. Cargo waits for uploaded versions to become available in the index;
if it times out, confirm whether the upload succeeded before trying again.

Published versions cannot be overwritten. A broken published version may need a
new version, with a yank if appropriate. Cargo's `"0.0.1"` dependency requirement
does not accept `0.0.2`, so each coordinated release must update internal version
requirements. See [Cargo publishing](https://doc.rust-lang.org/cargo/reference/publishing.html)
and [version requirements](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#version-requirement-syntax).

For future automation, consider per-crate Trusted Publishers for this repository
and `publish.yml`, using OIDC temporary credentials instead of a stored token.
That is a separate authentication configuration; the current workflow uses the
explicitly requested API token. See the
[official authentication action](https://github.com/rust-lang/crates-io-auth-action).
docs.rs automatically builds documentation after publication; confirm its actual
build results at release time. See [docs.rs build documentation](https://docs.rs/about/builds).
