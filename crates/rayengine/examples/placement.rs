//! Build mode: click to place, R to rotate, right-click to select/move,
//! Escape to cancel a move, Delete to remove. CPU helper; no assets required.
use rayengine::prelude::*;
use rayengine::raylib::prelude::MouseButton;
use std::collections::BTreeMap;

const PLACE: Action = Action(0);
const ROTATE: Action = Action(1);
const SELECT: Action = Action(2);
const CANCEL: Action = Action(3);
const REMOVE: Action = Action(4);

struct BuildMode {
    grid: PlacementGrid,
    terrain: CostGrid,
    shape: Footprint,
    layout: GridLayout,
    solids: BTreeMap<PlacementId, Vec<Aabb3>>,
    path: Vec<UVec2>,
    selected: Option<PlacementId>,
    rotation: QuarterTurn,
    hover: Option<UVec2>,
    next_id: u64,
}

impl BuildMode {
    fn new() -> Self {
        let size = UVec2::new(12, 10);
        let mut game = Self {
            grid: PlacementGrid::new(size),
            terrain: CostGrid::new(size, 1.0),
            shape: Footprint::new([IVec2::ZERO, IVec2::X], [IVec2::Y]).unwrap(),
            layout: GridLayout::new(-size.as_vec2() * 0.5, Vec2::ONE),
            solids: BTreeMap::new(),
            path: Vec::new(),
            selected: None,
            rotation: QuarterTurn::Zero,
            hover: None,
            next_id: 2,
        };
        let change = game
            .grid
            .place(
                PlacementId(1),
                &game.shape,
                PlacementPose {
                    anchor: IVec2::new(5, 4),
                    rotation: QuarterTurn::Zero,
                },
            )
            .unwrap();
        game.apply(change);
        game
    }

    fn bounds(&self, cell: UVec2, height: f32) -> Aabb3 {
        let bounds = self.layout.cell_bounds(cell);
        Aabb3 {
            min: Vec3::new(bounds.min.x + 0.03, 0.0, bounds.min.y + 0.03),
            max: Vec3::new(bounds.max.x - 0.03, height, bounds.max.y - 0.03),
        }
    }

    // Apply collision snapshots only after a successful edit. The navigation
    // adapter reads the same final occupancy, preserving game terrain costs.
    fn apply(&mut self, change: PlacementChange) {
        if let Some(before) = change.before {
            self.solids.remove(&before.id());
        }
        if let Some(after) = change.after {
            let bounds = after
                .occupied()
                .iter()
                .map(|&cell| self.bounds(cell, 0.8))
                .collect();
            self.solids.insert(after.id(), bounds);
        }
        self.path.clear();
        if let Some(goal) = self
            .grid
            .objects()
            .find_map(|object| object.access().first().copied())
        {
            // This path demonstrates reachability from an entry. Free access
            // alone does not guarantee it; an unreachable goal leaves no path.
            PathFinder::new()
                .find_path(
                    &self.grid.navigation(&self.terrain),
                    UVec2::ZERO,
                    goal,
                    &PathOptions::default(),
                    &mut self.path,
                )
                .unwrap();
        }
    }

    fn pose(&self) -> Option<PlacementPose> {
        Some(PlacementPose {
            anchor: self.hover?.as_ivec2(),
            rotation: self.rotation,
        })
    }

    fn preview(&self) -> Option<Result<PlacedObject, PlacementError>> {
        let pose = self.pose()?;
        Some(match self.selected {
            Some(id) => self.grid.validate_move(id, pose),
            None => self
                .grid
                .validate_place(PlacementId(self.next_id), &self.shape, pose),
        })
    }
}

impl Game for BuildMode {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(PLACE, Button::Mouse(MouseButton::MOUSE_BUTTON_LEFT))
            .bind(SELECT, Button::Mouse(MouseButton::MOUSE_BUTTON_RIGHT))
            .bind(ROTATE, KeyboardKey::KEY_R)
            .bind(CANCEL, KeyboardKey::KEY_ESCAPE)
            .bind(REMOVE, KeyboardKey::KEY_DELETE)
    }

    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
        self.hover = ctx
            .pointer
            .and_then(|pointer| {
                camera().screen_to_plane(
                    ctx.viewport.ui_to_screen(pointer),
                    &ctx.viewport,
                    Vec3::ZERO,
                    Vec3::Y,
                )
            })
            .and_then(|point| {
                self.layout
                    .cell_at(Vec2::new(point.x, point.z), self.grid.size())
            });
        if ctx.input.pressed(CANCEL) {
            self.selected = None;
        }
        if ctx.input.pressed(SELECT) {
            self.selected = self.hover.and_then(|cell| self.grid.occupant(cell));
            if let Some(object) = self.selected.and_then(|id| self.grid.object(id)) {
                self.rotation = object.pose().rotation;
            }
        }
        if ctx.input.pressed(ROTATE) {
            self.rotation = self.rotation.next();
        }
        if ctx.input.pressed(REMOVE) {
            let id = self
                .selected
                .or_else(|| self.hover.and_then(|cell| self.grid.occupant(cell)));
            if let Some(id) = id {
                let change = self.grid.remove(id).unwrap();
                self.apply(change);
                self.selected = None;
            }
        } else if ctx.input.pressed(PLACE)
            && let Some(pose) = self.pose()
        {
            let change = match self.selected {
                Some(id) => self.grid.move_object(id, pose),
                None => self
                    .grid
                    .place(PlacementId(self.next_id), &self.shape, pose),
            };
            if let Ok(change) = change {
                if self.selected.is_none() {
                    self.next_id += 1;
                }
                self.apply(change);
                self.selected = None;
            }
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::new(20, 26, 36, 255));
        let preview = self.preview();
        frame.world_3d(camera(), |canvas| {
            for y in 0..self.grid.size().y {
                for x in 0..self.grid.size().x {
                    canvas.cube(self.bounds(UVec2::new(x, y), 0.03), Color::DARKGRAY);
                }
            }
            for (&id, solids) in &self.solids {
                for &solid in solids {
                    canvas.cube(
                        solid,
                        if Some(id) == self.selected {
                            Color::ORANGE
                        } else {
                            Color::BEIGE
                        },
                    );
                }
            }
            for object in self.grid.objects() {
                for &cell in object.access() {
                    canvas.cube(self.bounds(cell, 0.08), Color::SKYBLUE);
                }
            }
            for pair in self.path.windows(2) {
                let point = |cell| {
                    let p = self.layout.cell_center(cell);
                    Vec3::new(p.x, 0.12, p.y)
                };
                canvas.line(point(pair[0]), point(pair[1]), Color::GOLD);
            }
            if let Some(Ok(object)) = &preview {
                for &cell in object.occupied() {
                    canvas.cube(self.bounds(cell, 0.95), Color::GREEN);
                }
                for &cell in object.access() {
                    canvas.cube(self.bounds(cell, 0.12), Color::LIME);
                }
            } else if let Some(cell) = self.hover {
                // Rejected footprint (including out-of-bounds rotation): red
                // anchor marks where the attempted edit would be made.
                canvas.cube(self.bounds(cell, 0.95), Color::RED);
            }
        });
        let status = match preview {
            Some(Ok(_)) => "Valid placement — click to commit".to_owned(),
            Some(Err(error)) => error.to_string(),
            None => "Point at the floor to preview".to_owned(),
        };
        frame.ui(|ui| {
            ui.text(
                "Click place / move   R rotate   Right-click select   Esc cancel   Delete remove",
                Vec2::splat(16.0),
                16.0,
                Color::WHITE,
            );
            ui.text(&status, Vec2::new(16.0, 42.0), 16.0, Color::WHITE);
            ui.text(
                "Blue: free interaction cells   Gold: navigation from entry to first object",
                Vec2::new(16.0, 66.0),
                16.0,
                Color::LIGHTGRAY,
            );
        });
    }
}

fn camera() -> Camera3D {
    Camera3D {
        position: Vec3::new(10.0, 14.0, 14.0),
        target: Vec3::ZERO,
        up: Vec3::Y,
        vertical_fov: 50.0,
    }
}

fn config() -> Config {
    let mut config = Config::new("rayengine / Build mode");
    config.exit_key = None; // Escape cancels a move; close the window to exit.
    config
}

fn main() -> Result<(), Error> {
    App::new(config())
        .with_options(RunOptions::from_env()?)
        .run(BuildMode::new())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escape_is_available_to_cancel_moves() {
        assert_eq!(config().exit_key, None);
    }

    #[test]
    fn collision_refresh_tracks_moves_removals_and_failed_edits() {
        let mut game = BuildMode::new();
        let id = PlacementId(1);
        let old_solids = game.solids.clone();
        assert!(
            game.grid
                .move_object(
                    id,
                    PlacementPose {
                        anchor: IVec2::new(11, 9),
                        rotation: QuarterTurn::Zero
                    }
                )
                .is_err()
        );
        assert_eq!(game.solids, old_solids);
        let change = game
            .grid
            .move_object(
                id,
                PlacementPose {
                    anchor: IVec2::new(2, 3),
                    rotation: QuarterTurn::One,
                },
            )
            .unwrap();
        game.apply(change);
        let expected: Vec<_> = game
            .grid
            .object(id)
            .unwrap()
            .occupied()
            .iter()
            .map(|&cell| game.bounds(cell, 0.8))
            .collect();
        assert_eq!(game.solids[&id], expected);
        assert!(!game.path.is_empty());
        let change = game.grid.remove(id).unwrap();
        game.apply(change);
        assert!(game.solids.is_empty());
        assert!(game.path.is_empty());
    }
}
