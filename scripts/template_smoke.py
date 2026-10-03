#!/usr/bin/env python3
"""Compile real starters, a generated plugin, and external plugin consumers."""

import json
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parent.parent
environment = os.environ.copy()
environment["CARGO_TARGET_DIR"] = str(Path(environment.get("CARGO_TARGET_DIR", root / "target")).resolve())
subprocess.run(["cargo", "build", "-p", "rayengine-cli", "--locked"], cwd=root, env=environment, check=True)
binary = Path(environment["CARGO_TARGET_DIR"]) / "debug" / "rayengine"
with tempfile.TemporaryDirectory(prefix="rayengine-template-") as temporary:
    for kind in ("2d", "3d", "topdown", "platformer"):
        project = Path(temporary) / f"game-{kind}"
        for arguments in (["new", str(project), "--template", kind, "--sdk-path", str(root / "crates/rayengine")], ["check", str(project)]):
            result = subprocess.run([str(binary), "--json", *arguments], env=environment, text=True, capture_output=True)
            response = json.loads(result.stdout)
            if result.returncode or not response["ok"]:
                raise SystemExit(json.dumps(response, indent=2))
        # CLI inspection agrees with the generated manifest, even from elsewhere.
        result = subprocess.run([str(binary), "--json", "info", str(project / "rayengine.toml"), "--profile", "dev"], cwd=temporary, env=environment, text=True, capture_output=True, check=True)
        description = json.loads(result.stdout)["data"]["project_manifest"]
        assert description["settings"]["assets"]["roots"] == [str(project / "assets")]
        assert description["settings"]["project"]["executable"] == project.name
        assert description["settings"]["render"]["vsync"] is False
        print(f"Generated {kind} starter compiles and its manifest resolves")

    # A generated library nested below a game stays independent until the game
    # explicitly adds it as a dependency. Exercise the real CLI output.
    game = Path(temporary) / "game-3d"
    plugin = game / "plugins/my-plugin"
    result = subprocess.run(
        [str(binary), "--json", "new-plugin", str(plugin), "--name", "my-plugin", "--sdk-path", str(root / "crates/rayengine")],
        env=environment, text=True, capture_output=True,
    )
    response = json.loads(result.stdout)
    if result.returncode or not response["ok"]:
        raise SystemExit(json.dumps(response, indent=2))
    subprocess.run(["cargo", "check", "--manifest-path", str(plugin / "Cargo.toml")], env=environment, check=True)
    manifest = game / "Cargo.toml"
    source = manifest.read_text()
    source = source.replace("[dependencies]\n", '[dependencies]\nmy-plugin = { path = "plugins/my-plugin" }\n')
    source = source.replace("[dependencies]\n", "[dependencies]\nrayengine-beacons = { path = " + json.dumps(str(root / "plugins/beacons")) + " }\n")
    manifest.write_text(source)
    (game / "src/main.rs").write_text('''use rayengine::prelude::*;
use my_plugin::{MyPlugin, PluginState};
use rayengine_beacons::{Beacon, BeaconWorld};

#[derive(Default)]
struct Demo { plugin: MyPlugin, state: PluginState, world: BeaconWorld, beacon: Option<Beacon> }
impl Game for Demo {
    fn init(&mut self, context: &mut InitContext<'_, '_>) -> Result<(), Error> {
        self.plugin.init(&mut self.state, context)?;
        let mut beacon = Beacon::new(Vec3::ZERO, Color::WHITE, 1.0)?;
        beacon.init(&mut self.world, context)?;
        self.beacon = Some(beacon);
        Ok(())
    }
    fn fixed_update(&mut self, context: &mut Update<'_, '_>) {
        self.plugin.fixed_update(&mut self.state, context);
        if let Some(beacon) = &mut self.beacon { beacon.fixed_update(&mut self.world, context); }
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::BLACK);
        self.plugin.draw(&self.state, frame);
        if let Some(beacon) = &mut self.beacon { beacon.draw(&self.world, frame); }
    }
}
fn main() -> Result<(), Error> { App::new(Config::new("External plugins")).run(Demo::default())?; Ok(()) }
''')
    subprocess.run(["cargo", "check", "--manifest-path", str(manifest)], env=environment, check=True)
    print("Generated and repository plugins compose in an external game")
