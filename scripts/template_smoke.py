#!/usr/bin/env python3
"""Generate and compile both real starter projects, outside the workspace."""

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
    for kind in ("2d", "3d"):
        project = Path(temporary) / f"game-{kind}"
        for arguments in (["new", str(project), "--kind", kind, "--sdk-path", str(root / "crates/rayengine")], ["check", str(project)]):
            result = subprocess.run([str(binary), "--json", *arguments], env=environment, text=True, capture_output=True)
            response = json.loads(result.stdout)
            if result.returncode or not response["ok"]:
                raise SystemExit(json.dumps(response, indent=2))
        print(f"Generated {kind} starter compiles")
