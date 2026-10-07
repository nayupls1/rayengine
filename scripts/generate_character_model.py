#!/usr/bin/env python3
"""Generate the original skinned glTF assets used by rayengine's 3D character
animation example and native probes. Run from the repository root:

    python3 scripts/generate_character_model.py

The output is deterministic and uses only the Python standard library. The
geometry, skeleton and animation curves are authored here and distributed
under the repository's MIT license.
"""

import json
import math
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CHARACTER = ROOT / "crates/rayengine/examples/assets/character.glb"
PENDULUM = ROOT / "crates/rayengine/tests/fixtures/pendulum.glb"
# Valid glTF that raylib's animation loader would dereference a null parent for.
ROOT_JOINT = ROOT / "crates/rayengine/tests/fixtures/root_joint.glb"

ARRAY_BUFFER = 34962
ELEMENT_ARRAY_BUFFER = 34963
FLOAT = 5126
UNSIGNED_BYTE = 5121
UNSIGNED_SHORT = 5123


def quat_axis(axis, degrees):
    half = math.radians(degrees) / 2.0
    s = math.sin(half)
    x, y, z = axis
    # Round so the binary output does not depend on platform libm tails.
    return [round(x * s, 7), round(y * s, 7), round(z * s, 7), round(math.cos(half), 7)]


def box(center, size, bone, color):
    """Axis-aligned box with flat normals, rigidly bound to one joint."""
    cx, cy, cz = center
    hx, hy, hz = (s / 2.0 for s in size)
    faces = [
        ((1, 0, 0), [(1, -1, -1), (1, 1, -1), (1, 1, 1), (1, -1, 1)]),
        ((-1, 0, 0), [(-1, -1, 1), (-1, 1, 1), (-1, 1, -1), (-1, -1, -1)]),
        ((0, 1, 0), [(-1, 1, -1), (-1, 1, 1), (1, 1, 1), (1, 1, -1)]),
        ((0, -1, 0), [(-1, -1, 1), (-1, -1, -1), (1, -1, -1), (1, -1, 1)]),
        ((0, 0, 1), [(-1, -1, 1), (1, -1, 1), (1, 1, 1), (-1, 1, 1)]),
        ((0, 0, -1), [(1, -1, -1), (-1, -1, -1), (-1, 1, -1), (1, 1, -1)]),
    ]
    vertices = []
    for normal, corners in faces:
        for sx, sy, sz in corners:
            vertices.append(((cx + sx * hx, cy + sy * hy, cz + sz * hz), normal, bone, color))
    return vertices


class Builder:
    def __init__(self):
        self.data = bytearray()
        self.views = []
        self.accessors = []

    def _view(self, payload, target=None):
        while len(self.data) % 4:
            self.data.append(0)
        view = {"buffer": 0, "byteOffset": len(self.data), "byteLength": len(payload)}
        if target is not None:
            view["target"] = target
        self.data.extend(payload)
        self.views.append(view)
        return len(self.views) - 1

    def accessor(self, fmt, values, component, kind, target=None, normalized=False, bounds=False):
        payload = b"".join(struct.pack("<" + fmt, *v) if isinstance(v, (tuple, list)) else struct.pack("<" + fmt, v) for v in values)
        accessor = {
            "bufferView": self._view(payload, target),
            "componentType": component,
            "count": len(values),
            "type": kind,
        }
        if normalized:
            accessor["normalized"] = True
        if bounds:
            columns = list(zip(*values)) if isinstance(values[0], (tuple, list)) else [values]
            accessor["min"] = [min(c) for c in columns]
            accessor["max"] = [max(c) for c in columns]
        self.accessors.append(accessor)
        return len(self.accessors) - 1


def build(path, joints, parts, animations, mesh_name, armature=True):
    """joints: [(name, parent_index or None, local_translation)]; parts: [box vertices];
    animations: {name: [(joint, path, [(time, value)])]}"""
    vertices = [v for part in parts for v in part]
    indices = []
    for quad in range(len(vertices) // 4):
        base = quad * 4
        indices += [base, base + 1, base + 2, base, base + 2, base + 3]
    assert len(vertices) < 65536

    b = Builder()
    position = b.accessor("3f", [tuple(round(c, 6) for c in v[0]) for v in vertices], FLOAT, "VEC3", ARRAY_BUFFER, bounds=True)
    normal = b.accessor("3f", [v[1] for v in vertices], FLOAT, "VEC3", ARRAY_BUFFER)
    color = b.accessor("4B", [v[3] for v in vertices], UNSIGNED_BYTE, "VEC4", ARRAY_BUFFER, normalized=True)
    joint = b.accessor("4B", [(v[2], 0, 0, 0) for v in vertices], UNSIGNED_BYTE, "VEC4", ARRAY_BUFFER)
    weight = b.accessor("4f", [(1.0, 0.0, 0.0, 0.0) for _ in vertices], FLOAT, "VEC4", ARRAY_BUFFER)
    index = b.accessor("H", indices, UNSIGNED_SHORT, "SCALAR", ELEMENT_ARRAY_BUFFER)

    # Bind pose: joints have identity rotation/scale, so world translation is
    # the sum along the parent chain and inverse binds are pure translations.
    world = []
    for name, parent, local in joints:
        origin = (0.0, 0.0, 0.0) if parent is None else world[parent]
        world.append(tuple(round(o + l, 6) for o, l in zip(origin, local)))
    inverse = b.accessor(
        "16f",
        [(1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, -x, -y, -z, 1) for x, y, z in world],
        FLOAT,
        "MAT4",
    )

    nodes = []
    for i, (name, parent, local) in enumerate(joints):
        node = {"name": name, "translation": list(local)}
        children = [j for j, (_, p, _) in enumerate(joints) if p == i]
        if children:
            node["children"] = children
        nodes.append(node)
    roots = [i for i, (_, parent, _) in enumerate(joints) if parent is None]
    # Raylib's glTF animation loader reads the root joint's parent, so joints
    # live under an armature node as in typical DCC exports.
    scene = list(roots)
    if armature:
        scene = [len(nodes)]
        nodes.append({"name": "armature", "children": roots})
    mesh_node = len(nodes)
    nodes.append({"name": mesh_name, "mesh": 0, "skin": 0})

    gltf_animations = []
    for name, channels in animations.items():
        samplers, gltf_channels = [], []
        for joint_index, target, keys in channels:
            times = [round(t, 6) for t, _ in keys]
            values = [tuple(v) for _, v in keys]
            input_accessor = b.accessor("f", times, FLOAT, "SCALAR", bounds=True)
            kind, fmt = ("VEC4", "4f") if target == "rotation" else ("VEC3", "3f")
            output_accessor = b.accessor(fmt, values, FLOAT, kind)
            samplers.append({"input": input_accessor, "output": output_accessor, "interpolation": "LINEAR"})
            gltf_channels.append({"sampler": len(samplers) - 1, "target": {"node": joint_index, "path": target}})
        gltf_animations.append({"name": name, "samplers": samplers, "channels": gltf_channels})

    document = {
        "asset": {"version": "2.0", "generator": "rayengine scripts/generate_character_model.py"},
        "scene": 0,
        "scenes": [{"nodes": scene + [mesh_node]}],
        "nodes": nodes,
        "meshes": [
            {
                "name": mesh_name,
                "primitives": [
                    {
                        "attributes": {
                            "POSITION": position,
                            "NORMAL": normal,
                            "COLOR_0": color,
                            "JOINTS_0": joint,
                            "WEIGHTS_0": weight,
                        },
                        "indices": index,
                        "material": 0,
                    }
                ],
            }
        ],
        "materials": [{"name": "vertex colors", "pbrMetallicRoughness": {"baseColorFactor": [1, 1, 1, 1], "metallicFactor": 0, "roughnessFactor": 1}}],
        "skins": [{"joints": list(range(len(joints))), "inverseBindMatrices": inverse, "skeleton": roots[0]}],
        "animations": gltf_animations,
        "accessors": b.accessors,
        "bufferViews": b.views,
        "buffers": [{"byteLength": len(b.data)}],
    }

    payload = json.dumps(document, separators=(",", ":"), sort_keys=True).encode()
    payload += b" " * (-len(payload) % 4)
    binary = bytes(b.data) + b"\0" * (-len(b.data) % 4)
    total = 12 + 8 + len(payload) + 8 + len(binary)
    out = struct.pack("<4sII", b"glTF", 2, total)
    out += struct.pack("<I4s", len(payload), b"JSON") + payload
    out += struct.pack("<I4s", len(binary), b"BIN\0") + binary
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(out)
    print(f"wrote {path.relative_to(ROOT)} ({len(out)} bytes)")


def character():
    # Y-up, metres, facing +Z. The character's left side is +X.
    joints = [
        ("hips", None, (0.0, 0.95, 0.0)),
        ("chest", 0, (0.0, 0.2, 0.0)),
        ("head", 1, (0.0, 0.45, 0.0)),
        ("arm_left", 1, (0.31, 0.43, 0.0)),
        ("arm_right", 1, (-0.31, 0.43, 0.0)),
        ("leg_left", 0, (0.12, -0.05, 0.0)),
        ("leg_right", 0, (-0.12, -0.05, 0.0)),
    ]
    skin = (232, 186, 140, 255)
    tunic = (38, 140, 128, 255)
    belt = (92, 58, 36, 255)
    sleeve = (240, 140, 40, 255)
    trousers = (52, 58, 92, 255)
    boots = (40, 30, 26, 255)
    parts = [
        box((0.0, 0.95, 0.0), (0.44, 0.2, 0.26), 0, belt),
        box((0.0, 1.31, 0.0), (0.5, 0.52, 0.28), 1, tunic),
        box((0.0, 1.78, 0.0), (0.32, 0.32, 0.3), 2, skin),
        box((0.0, 1.8, 0.16), (0.2, 0.06, 0.03), 2, boots),  # visor marks the face
        box((0.38, 1.36, 0.0), (0.14, 0.44, 0.14), 3, sleeve),
        box((0.38, 1.08, 0.0), (0.12, 0.14, 0.12), 3, skin),
        box((-0.38, 1.36, 0.0), (0.14, 0.44, 0.14), 4, sleeve),
        box((-0.38, 1.08, 0.0), (0.12, 0.14, 0.12), 4, skin),
        box((0.12, 0.5, 0.0), (0.17, 0.72, 0.18), 5, trousers),
        box((0.12, 0.07, 0.04), (0.18, 0.14, 0.26), 5, boots),
        box((-0.12, 0.5, 0.0), (0.17, 0.72, 0.18), 6, trousers),
        box((-0.12, 0.07, 0.04), (0.18, 0.14, 0.26), 6, boots),
    ]
    x, z = (1, 0, 0), (0, 0, 1)

    def cycle(duration, samples, value):
        return [(duration * i / samples, value(2.0 * math.pi * i / samples)) for i in range(samples + 1)]

    walk = [
        (5, "rotation", cycle(1.0, 8, lambda p: quat_axis(x, 32.0 * math.sin(p)))),
        (6, "rotation", cycle(1.0, 8, lambda p: quat_axis(x, -32.0 * math.sin(p)))),
        (3, "rotation", cycle(1.0, 8, lambda p: quat_axis(x, -26.0 * math.sin(p)))),
        (4, "rotation", cycle(1.0, 8, lambda p: quat_axis(x, 26.0 * math.sin(p)))),
        (0, "translation", cycle(1.0, 8, lambda p: (0.0, round(0.95 + 0.035 * abs(math.cos(p)) - 0.035, 6), 0.0))),
    ]
    idle = [
        (1, "rotation", cycle(2.0, 4, lambda p: quat_axis(x, 2.5 * math.sin(p)))),
        (3, "rotation", cycle(2.0, 4, lambda p: quat_axis(z, 4.0 + 2.0 * math.sin(p)))),
        (4, "rotation", cycle(2.0, 4, lambda p: quat_axis(z, -4.0 - 2.0 * math.sin(p)))),
    ]
    # One-shot greeting: raise the right arm outward, wave twice, then lower it.
    wave_keys = [(0.0, 0.0), (0.35, -150.0), (0.55, -128.0), (0.75, -162.0), (0.95, -128.0), (1.15, -158.0), (1.6, 0.0)]
    wave = [
        (4, "rotation", [(t, quat_axis(z, a)) for t, a in wave_keys]),
        (2, "rotation", [(0.0, quat_axis(z, 0.0)), (0.5, quat_axis(z, -8.0)), (1.2, quat_axis(z, -8.0)), (1.6, quat_axis(z, 0.0))]),
    ]
    build(CHARACTER, joints, parts, {"idle": idle, "walk": walk, "wave": wave}, "character")


def pendulum():
    # A deliberately different skeleton (two joints) for compatibility probes.
    joints = [("base", None, (0.0, 0.0, 0.0)), ("rod", 0, (0.0, 1.0, 0.0))]
    parts = [
        box((0.0, 0.5, 0.0), (0.2, 1.0, 0.2), 0, (200, 200, 200, 255)),
        box((0.0, 1.5, 0.0), (0.1, 1.0, 0.1), 1, (220, 60, 60, 255)),
    ]
    swing = [(1, "rotation", [(t, quat_axis((0, 0, 1), a)) for t, a in [(0.0, 0.0), (0.5, 40.0), (1.0, 0.0)]])]
    build(PENDULUM, joints, parts, {"swing": swing}, "pendulum")
    build(ROOT_JOINT, joints, parts, {"swing": swing}, "pendulum", armature=False)


if __name__ == "__main__":
    character()
    pendulum()
