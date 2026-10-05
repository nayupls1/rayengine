//! Residents walking a two-floor house. Stairs and a closable door are
//! navigation links; clicking places or removes furniture, invalidating the
//! routes it blocks. Residents meeting in the one-cell hallways step aside.
//! Run with `cargo run -p rayengine --example navigation`.
use rayengine::prelude::*;
use rayengine::raylib::prelude::MouseButton;

const PLACE: Action = Action(0);
const DOOR: Action = Action(1);
const RESET: Action = Action(2);
const GROUND: [&str; 8] = [
    "################",
    "#......#.......#",
    "#......#.......#",
    "#......#.......#",
    "#......#.......#",
    "####.######.####",
    "#..............#",
    "################",
];
const UPSTAIRS: [&str; 8] = [
    "################",
    "#.....#........#",
    "#.....#........#",
    "#.....#........#",
    "#..............#",
    "####.#####.#####",
    "#..............#",
    "################",
];
/// Fixed ticks per traffic step: residents walk one cell per step.
const STEP_TICKS: u32 = 8;
/// Traffic steps spent on the stairs, and resting at each goal.
const CLIMB_STEPS: u32 = 3;
const REST_STEPS: u32 = 6;
/// World units between the two floor plans.
const GAP: f32 = 2.0;

/// What a link means to the game; the engine only carries it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Passage {
    Door,
    Stairs,
}

struct Resident {
    id: AgentId,
    name: &'static str,
    color: Color,
    errands: [NavPoint; 3],
    next: usize,
    rest: u32,
    from: Vec2,
    to: Vec2,
}

struct House {
    floors: [CostGrid; 2],
    topology: NavTopology<Passage>,
    door: LinkId,
    traffic: Traffic,
    residents: Vec<Resident>,
    events: Vec<TrafficEvent>,
    layout: GridLayout,
    clock: u32,
}

fn parse(rows: &[&str]) -> CostGrid {
    let size = UVec2::new(rows[0].len() as u32, rows.len() as u32);
    let mut grid = CostGrid::new(size, 1.0);
    for (y, row) in rows.iter().enumerate() {
        for (x, symbol) in row.bytes().enumerate() {
            if symbol == b'#' {
                grid.set(UVec2::new(x as u32, y as u32), None);
            }
        }
    }
    grid
}

fn at(layer: u32, x: u32, y: u32) -> NavPoint {
    NavPoint::new(layer, UVec2::new(x, y))
}

impl House {
    fn new() -> Self {
        let floors = [parse(&GROUND), parse(&UPSTAIRS)];
        let size = floors[0].size();
        let width = 2.0 * size.x as f32 + GAP;
        let layout = GridLayout::new(Vec2::new(-width * 0.5, -(size.y as f32) * 0.5), Vec2::ONE);
        // The door spans the wall between the two downstairs rooms; stairs
        // join both ends of the hallways.
        let mut topology = NavTopology::new();
        let door = topology.add(NavLink::new(at(0, 6, 3), at(0, 8, 3), 2.0, Passage::Door));
        for x in [1, 14] {
            topology.add(NavLink::new(at(0, x, 6), at(1, x, 6), 4.0, Passage::Stairs));
        }
        let mut house = Self {
            floors,
            topology,
            door,
            traffic: Traffic::new(TrafficOptions {
                neighborhood: Neighborhood::Four,
                ..TrafficOptions::default()
            }),
            residents: Vec::new(),
            events: Vec::new(),
            layout,
            clock: 0,
        };
        let red = Color::new(230, 86, 92, 255);
        let residents = [
            (
                "Red",
                red,
                at(0, 2, 2),
                [at(0, 13, 2), at(1, 3, 2), at(0, 2, 2)],
            ),
            (
                "Blue",
                Color::SKYBLUE,
                at(0, 13, 3),
                [at(0, 2, 3), at(1, 12, 2), at(0, 13, 3)],
            ),
            (
                "Gold",
                Color::GOLD,
                at(1, 3, 1),
                [at(1, 13, 3), at(0, 4, 1), at(1, 3, 1)],
            ),
            (
                "Lime",
                Color::LIME,
                at(1, 12, 4),
                [at(0, 12, 1), at(1, 2, 4), at(1, 12, 4)],
            ),
        ];
        for (rank, (name, color, start, errands)) in residents.into_iter().enumerate() {
            // Earlier residents have priority in narrow hallways.
            let id = house.traffic.add(start, Vec2::ZERO, -(rank as i32));
            let here = house.world(start);
            house.residents.push(Resident {
                id,
                name,
                color,
                errands,
                next: 0,
                rest: rank as u32 * 4,
                from: here,
                to: here,
            });
        }
        house
    }

    /// Center of a point's cell, with upstairs drawn to the right.
    fn world(&self, point: NavPoint) -> Vec2 {
        let shift = point.layer as f32 * (self.floors[0].size().x as f32 + GAP);
        self.layout.cell_center(point.cell) + Vec2::new(shift, 0.0)
    }

    fn point_at(&self, world: Vec2) -> Option<NavPoint> {
        let shift = self.floors[0].size().x as f32 + GAP;
        (0..2).find_map(|layer| {
            let local = world - Vec2::new(layer as f32 * shift, 0.0);
            let cell = self.layout.cell_at(local, self.floors[layer].size())?;
            Some(NavPoint::new(layer as u32, cell))
        })
    }

    /// Places or removes furniture. Agents' cells and walls stay as they are.
    fn toggle_furniture(&mut self, point: NavPoint) {
        let edge = self.floors[0].size() - 1;
        let border = point.cell.x == 0 || point.cell.y == 0 || point.cell.cmpeq(edge).any();
        let occupied = self
            .traffic
            .agents()
            .any(|id| self.traffic.position(id) == Some(point));
        let linked = self
            .topology
            .iter()
            .any(|(_, link)| link.from == point || link.to == point);
        if border || occupied || linked {
            return;
        }
        let grid = &mut self.floors[point.layer as usize];
        let blocked = !grid.walkable(point.cell);
        grid.set(point.cell, blocked.then_some(1.0));
        // Grid edits happen outside the topology, so report them.
        self.topology.mark_changed();
    }

    fn step(&mut self) {
        for resident in &mut self.residents {
            let state = self.traffic.state(resident.id);
            if matches!(state, Some(AgentState::Idle | AgentState::Arrived)) {
                if resident.rest > 0 {
                    resident.rest -= 1;
                } else {
                    let goal = resident.errands[resident.next];
                    resident.next = (resident.next + 1) % resident.errands.len();
                    self.traffic.set_goal(resident.id, Some(goal));
                }
            }
        }
        self.traffic
            .tick(&self.floors, &self.topology, &mut self.events)
            .expect("residents and links stay inside the house");
        for resident in &mut self.residents {
            resident.from = resident.to;
        }
        for event in &self.events {
            match *event {
                TrafficEvent::Moved {
                    agent, to, link, ..
                } => {
                    let world = self.world(to);
                    let Some(resident) = self.residents.iter_mut().find(|r| r.id == agent) else {
                        continue;
                    };
                    resident.to = world;
                    // Game-owned traversal effects: climbing takes a while.
                    let passage = link.and_then(|link| self.topology.get(link)).map(|l| l.tag);
                    if passage == Some(Passage::Stairs) {
                        resident.from = world;
                        self.traffic.hold(agent, CLIMB_STEPS);
                    }
                }
                TrafficEvent::Arrived(agent) => {
                    if let Some(resident) = self.residents.iter_mut().find(|r| r.id == agent) {
                        resident.rest = REST_STEPS;
                    }
                }
                _ => {}
            }
        }
    }
}

impl Game for House {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(PLACE, Button::Mouse(MouseButton::MOUSE_BUTTON_LEFT))
            .bind(DOOR, KeyboardKey::KEY_D)
            .bind(RESET, KeyboardKey::KEY_R)
    }

    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
        if ctx.input.pressed(RESET) {
            *self = Self::new();
        }
        if ctx.input.pressed(DOOR) {
            let open = self
                .topology
                .get(self.door)
                .is_some_and(|door| door.enabled);
            self.topology.set_enabled(self.door, !open);
        }
        if ctx.input.pressed(PLACE)
            && let Some(pointer) = ctx.pointer
        {
            let screen = ctx.viewport.ui_to_screen(pointer);
            if let Some(point) = camera(&ctx.viewport)
                .screen_to_world(screen, &ctx.viewport)
                .and_then(|world| self.point_at(world))
            {
                self.toggle_furniture(point);
            }
        }
        if self.clock.is_multiple_of(STEP_TICKS) {
            self.step();
        }
        self.clock += 1;
    }

    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::new(18, 24, 33, 255));
        let progress = ((self.clock % STEP_TICKS) as f32 + frame.alpha) / STEP_TICKS as f32;
        let camera = camera(&frame.viewport);
        let labels = [at(0, 0, 0), at(1, 0, 0)].map(|corner| {
            let above = self.world(corner) - Vec2::new(0.5, 0.0) - Vec2::Y * 0.6;
            camera.world_to_ui(above, &frame.viewport)
        });
        frame.world_2d(camera, |canvas| {
            for (layer, grid) in self.floors.iter().enumerate() {
                let size = grid.size();
                for y in 0..size.y {
                    for x in 0..size.x {
                        let point = at(layer as u32, x, y);
                        let wall = [GROUND, UPSTAIRS][layer][y as usize].as_bytes()[x as usize];
                        let color = match (grid.walkable(point.cell), wall) {
                            (true, _) => Color::new(40, 46, 58, 255),
                            (false, b'#') => Color::new(92, 104, 122, 255),
                            // Furniture placed by clicking.
                            (false, _) => Color::new(150, 112, 70, 255),
                        };
                        let center = self.world(point);
                        canvas.rectangle(
                            Aabb2 {
                                min: center - 0.47,
                                max: center + 0.47,
                            },
                            color,
                        );
                    }
                }
            }
            for (_, link) in self.topology.iter() {
                let color = match (link.tag, link.enabled) {
                    (Passage::Stairs, _) => Color::new(180, 140, 255, 255),
                    (Passage::Door, true) => Color::new(120, 220, 140, 255),
                    (Passage::Door, false) => Color::new(220, 90, 90, 255),
                };
                let (from, to) = (self.world(link.from), self.world(link.to));
                if link.tag == Passage::Stairs {
                    canvas.circle(from, 0.25, color);
                    canvas.circle(to, 0.25, color);
                } else {
                    canvas.line(from, to, 0.18, color);
                }
            }
            for resident in &self.residents {
                let mut from = resident.to;
                for step in self.traffic.remaining(resident.id).iter().skip(1) {
                    let point = self.world(step.point);
                    if step.link.is_none() {
                        canvas.line(from, point, 0.06, resident.color);
                    }
                    from = point;
                }
                let position = resident.from.lerp(resident.to, progress.min(1.0));
                canvas.circle(position, 0.36, resident.color);
            }
        });
        frame.ui(|ui| {
            let door = match self.topology.get(self.door) {
                Some(link) if link.enabled => "open",
                _ => "closed",
            };
            ui.text(
                &format!("Click: place/remove furniture   D: door ({door})   R: reset"),
                Vec2::new(16.0, 16.0),
                18.0,
                Color::WHITE,
            );
            for (label, position) in ["Ground floor", "Upstairs"].into_iter().zip(labels) {
                ui.text(label, position - Vec2::Y * 18.0, 16.0, Color::LIGHTGRAY);
            }
            for (row, resident) in self.residents.iter().enumerate() {
                let state = self.traffic.state(resident.id).unwrap_or(AgentState::Idle);
                ui.text(
                    &format!("{}: {state:?}", resident.name),
                    Vec2::new(16.0, 64.0 + row as f32 * 20.0),
                    16.0,
                    resident.color,
                );
            }
        });
    }
}

/// Fits both floor plans, with room for the text above them.
fn camera(viewport: &Viewport) -> Camera2D {
    let size = Vec2::new(GROUND[0].len() as f32, GROUND.len() as f32);
    let world = Vec2::new(2.0 * size.x + GAP + 1.0, size.y + 6.0);
    let aspect = viewport.logical_size.x / viewport.logical_size.y;
    Camera2D {
        target: Vec2::new(0.0, -2.0),
        view_height: world.y.max(world.x / aspect),
        ..Camera2D::default()
    }
}

fn main() -> Result<(), Error> {
    App::new(Config::new("rayengine / Navigation"))
        .with_options(RunOptions::from_env()?)
        .run(House::new())?;
    Ok(())
}
