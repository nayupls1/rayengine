//! Agents navigating around walls. A distance field steers the pack toward the
//! gold target; the scout follows a smoothed A* path planned a few cells per tick.
//! Run with `cargo run -p rayengine --example pathfinding`.
use rayengine::pathfinding::smooth_path;
use rayengine::prelude::*;
use rayengine::raylib::prelude::MouseButton;

const PLACE: Action = Action(0);
const RESET: Action = Action(1);
const MAP: [&str; 14] = [
    "........................",
    "..######.......######...",
    "..#....#............#...",
    "..#....#....####....#...",
    "..#.........#..#........",
    "..######....#..#...#####",
    "............#..#........",
    "#######.....#..#....###.",
    "......#.........#...#...",
    "......#.........#...#...",
    "..#####...#######...#..#",
    "......................#.",
    "..####.......#####....#.",
    ".............#..........",
];
const SPEED: f32 = 4.0;
/// Planning work per fixed tick, so long searches never stall a frame.
const BUDGET: u32 = 48;

struct Navigation {
    grid: CostGrid,
    layout: GridLayout,
    walls: Vec<Aabb2>,
    field: DistanceField,
    finder: PathFinder,
    path: Vec<UVec2>,
    follower: PathFollower,
    target: Vec2,
    pack: Vec<Body2D>,
    previous: Vec<Vec2>,
    scout: Body2D,
    previous_scout: Vec2,
    replan_in: f32,
}

impl Navigation {
    fn new() -> Self {
        let size = UVec2::new(MAP[0].len() as u32, MAP.len() as u32);
        let mut grid = CostGrid::new(size, 1.0);
        for (y, row) in MAP.iter().enumerate() {
            for (x, symbol) in row.bytes().enumerate() {
                if symbol == b'#' {
                    grid.set(UVec2::new(x as u32, y as u32), None);
                }
            }
        }
        let layout = GridLayout::new(-size.as_vec2() * 0.5, 1.0);
        let walls = (0..size.y)
            .flat_map(|y| (0..size.x).map(move |x| UVec2::new(x, y)))
            .filter(|&cell| !grid.walkable(cell))
            .map(|cell| layout.cell_bounds(cell))
            .collect();
        let mut navigation = Self {
            grid,
            layout,
            walls,
            field: DistanceField::new(),
            finder: PathFinder::new(),
            path: Vec::new(),
            follower: PathFollower::new(0.1),
            target: Vec2::ZERO,
            pack: Vec::new(),
            previous: Vec::new(),
            scout: Body2D::new(Vec2::ZERO, Vec2::splat(0.6)),
            previous_scout: Vec2::ZERO,
            replan_in: 0.0,
        };
        navigation.reset();
        navigation
    }

    fn reset(&mut self) {
        let body = |x, y| Body2D::new(self.layout.cell_center(UVec2::new(x, y)), Vec2::splat(0.6));
        self.pack = vec![
            body(0, 0),
            body(0, 13),
            body(23, 0),
            body(23, 13),
            body(4, 3),
        ];
        self.previous = self.pack.iter().map(|body| body.position).collect();
        self.scout = body(9, 0);
        self.previous_scout = self.scout.position;
        self.set_target(self.layout.cell_center(UVec2::new(14, 5)));
    }

    fn cell(&self, position: Vec2) -> Option<UVec2> {
        self.layout.cell_at(position, self.grid.size())
    }

    fn set_target(&mut self, point: Vec2) {
        let Some(goal) = self.cell(point).filter(|&cell| self.grid.walkable(cell)) else {
            return;
        };
        self.target = self.layout.cell_center(goal);
        // One field serves every pack member.
        self.field
            .compute(&self.grid, [goal], Neighborhood::default())
            .expect("the map is inside its own grid");
        self.replan_in = 0.0;
    }

    fn plan_scout(&mut self) {
        let options = PathOptions {
            budget: Some(BUDGET),
            ..PathOptions::default()
        };
        let status = if self.finder.is_pending() {
            self.finder.resume(&self.grid, Some(BUDGET), &mut self.path)
        } else if self.replan_in <= 0.0 {
            self.replan_in = 0.5;
            let (Some(start), Some(goal)) =
                (self.cell(self.scout.position), self.cell(self.target))
            else {
                return;
            };
            self.finder
                .find_path(&self.grid, start, goal, &options, &mut self.path)
        } else {
            return;
        };
        if let Ok(PathStatus::Found { .. }) = status {
            smooth_path(&self.grid, &mut self.path, options.neighborhood)
                .expect("paths stay inside the grid");
            self.follower.set_cells(&self.layout, &self.path);
        }
    }
}

impl Game for Navigation {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(PLACE, Button::Mouse(MouseButton::MOUSE_BUTTON_LEFT))
            .bind(RESET, KeyboardKey::KEY_R)
    }

    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
        let dt = ctx.tick.dt;
        if ctx.input.pressed(RESET) {
            self.reset();
        }
        if ctx.input.pressed(PLACE)
            && let Some(pointer) = ctx.pointer
        {
            let camera = camera();
            let screen = ctx.viewport.ui_to_screen(pointer);
            if let Some(world) = camera.screen_to_world(screen, &ctx.viewport) {
                self.set_target(world);
            }
        }

        for (body, previous) in self.pack.iter_mut().zip(&mut self.previous) {
            *previous = body.position;
            let here = self.layout.cell_at(body.position, self.grid.size());
            let next = here.and_then(|cell| self.field.next_step(&self.grid, cell));
            // Head for the next cell's center, or the target from its own cell.
            let aim = next.map_or(self.target, |cell| self.layout.cell_center(cell));
            let offset = aim - body.position;
            let distance = offset.length();
            body.velocity = if distance > 0.05 && (next.is_some() || distance > 0.6) {
                offset / distance * SPEED * 0.8
            } else {
                Vec2::ZERO
            };
            body.move_and_slide(dt, &self.walls);
        }

        self.previous_scout = self.scout.position;
        self.replan_in -= dt;
        self.plan_scout();
        self.follower.steer_body_2d(&mut self.scout, SPEED, dt);
        self.scout.move_and_slide(dt, &self.walls);
    }

    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::new(18, 24, 33, 255));
        let alpha = frame.alpha;
        frame.world_2d(camera(), |canvas| {
            let size = self.grid.size();
            for y in 0..size.y {
                for x in 0..size.x {
                    let cell = UVec2::new(x, y);
                    let color = match self.field.distance(cell) {
                        _ if !self.grid.walkable(cell) => Color::new(92, 104, 122, 255),
                        // Shade the distance field so its gradient is visible.
                        Some(distance) => {
                            let shade = ((1.0 - distance / 24.0).max(0.0) * 90.0) as u8;
                            Color::new(24, 30 + shade / 2, 40 + shade, 255)
                        }
                        None => Color::new(24, 28, 36, 255),
                    };
                    let bounds = self.layout.cell_bounds(cell);
                    canvas.rectangle(
                        Aabb2 {
                            min: bounds.min + 0.03,
                            max: bounds.max - 0.03,
                        },
                        color,
                    );
                }
            }
            let mut from = self.previous_scout.lerp(self.scout.position, alpha);
            for &point in self.follower.remaining() {
                canvas.line(from, point, 0.08, Color::SKYBLUE);
                from = point;
            }
            canvas.circle(self.target, 0.35, Color::GOLD);
            for (body, previous) in self.pack.iter().zip(&self.previous) {
                let position = previous.lerp(body.position, alpha);
                canvas.circle(position, 0.3, Color::new(230, 86, 92, 255));
            }
            let scout = self.previous_scout.lerp(self.scout.position, alpha);
            canvas.circle(scout, 0.32, Color::SKYBLUE);
        });
        frame.ui(|ui| {
            ui.text(
                "Click to move the target   R reset",
                Vec2::new(16.0, 16.0),
                18.0,
                Color::WHITE,
            );
            ui.text(
                "Red: distance-field pack   Blue: smoothed A* scout",
                Vec2::new(16.0, 40.0),
                16.0,
                Color::LIGHTGRAY,
            );
        });
    }
}

fn camera() -> Camera2D {
    Camera2D {
        view_height: 16.0,
        ..Camera2D::default()
    }
}

fn main() -> Result<(), Error> {
    App::new(Config::new("rayengine / Pathfinding"))
        .with_options(RunOptions::from_env()?)
        .run(Navigation::new())?;
    Ok(())
}
