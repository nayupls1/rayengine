# Optional plugins

Each plugin lives in its own directory here and is an ordinary Cargo library.
Package names use `rayengine-<name>` for repository plugins. Engine crates do not
depend on plugins. Games select and own instances through normal dependencies.
The workspace lists members explicitly; adding a directory does not activate it.

- `beacons/` (`rayengine-beacons`): small generated-mesh/ECS example with two
  independently configured instances and a runnable composition game.

Read [the plugin authoring guide](../crates/rayengine/docs/plugins.md) for lifecycle,
public engine APIs, scaffolding, compatibility, ownership, and testing. Future
CPU/backend subcrates for a plugin should stay beneath that plugin's directory.
