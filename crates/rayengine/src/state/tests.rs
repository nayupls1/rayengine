use super::*;
use rayengine_core::{
    glam::Vec2,
    input::{Action, Axis},
    time::Tick,
    viewport::{ScaleMode, Viewport},
};
use std::{cell::RefCell, rc::Rc};

const ACTION: Action = Action(0);
#[derive(Debug, PartialEq)]
struct Seen {
    name: &'static str,
    down: bool,
    pressed: bool,
    released: bool,
    delta: Vec2,
    analog: f32,
    pointer: Option<Vec2>,
    reset: bool,
    focused: bool,
}
struct Probe {
    name: &'static str,
    policy: StatePolicy,
    log: Rc<RefCell<Vec<Seen>>>,
    transition: bool,
}
impl State for Probe {
    fn policy(&self) -> StatePolicy {
        self.policy
    }
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>, commands: &mut StateCommands) {
        self.log.borrow_mut().push(Seen {
            name: self.name,
            down: ctx.input.down(ACTION),
            pressed: ctx.input.pressed(ACTION),
            released: ctx.input.released(ACTION),
            delta: ctx.input.pointer_delta(),
            analog: ctx.input.value(Axis(0)),
            pointer: ctx.pointer,
            reset: ctx.input.reset_pending(),
            focused: ctx.window_focused,
        });
        if self.transition {
            assert!(commands.request(Transition::Pop).is_ok());
        }
    }
    fn draw(&mut self, _: &mut Frame<'_, '_>, _: &mut StateCommands) {}
}
fn entry(
    name: &'static str,
    policy: StatePolicy,
    log: &Rc<RefCell<Vec<Seen>>>,
    transition: bool,
) -> Entry {
    Entry {
        state: Box::new(Probe {
            name,
            policy,
            log: log.clone(),
            transition,
        }),
        resources: StateResources::default(),
    }
}
fn update(stack: &mut StateStack, input: &Input) {
    let assets = Assets::new(None);
    let mut quit = false;
    let mut bindings = Bindings::new();
    stack.fixed_update(&mut Update {
        tick: Tick {
            index: 0,
            dt: 1.0 / 120.0,
        },
        input,
        bindings: &mut bindings,
        pointer: Some(Vec2::ONE),
        window_focused: true,
        viewport: Viewport::new(Vec2::splat(100.0), Vec2::splat(100.0), ScaleMode::Fit).unwrap(),
        assets: &assets,
        quit: &mut quit,
    });
}
fn pass() -> StatePolicy {
    StatePolicy {
        update_below: true,
        draw_below: true,
        input_below: true,
    }
}

#[test]
fn modal_pause_stops_simulation_but_selects_world_then_overlay_for_drawing() {
    let log = Rc::default();
    let modal = StatePolicy {
        draw_below: true,
        ..StatePolicy::default()
    };
    let mut stack = StateStack {
        entries: vec![
            entry("world", StatePolicy::default(), &log, false),
            entry("pause", modal, &log, false),
        ],
        ..StateStack::default()
    };
    update(&mut stack, &Input::default());
    assert_eq!(
        log.borrow().iter().map(|s| s.name).collect::<Vec<_>>(),
        ["pause"]
    );
    assert_eq!(draw_start(&stack.policies()), 0);
    assert_eq!(draw_start(&[pass(), StatePolicy::default(), pass()]), 1);
    assert_eq!(draw_start(&[]), 0);
}

#[test]
fn input_blocking_is_cumulative_and_preserves_focus_reset() {
    let log = Rc::default();
    let mut stack = StateStack {
        entries: vec![
            entry("world", pass(), &log, false),
            entry("lower", pass(), &log, false),
            entry(
                "upper",
                StatePolicy {
                    input_below: false,
                    ..pass()
                },
                &log,
                false,
            ),
        ],
        ..StateStack::default()
    };
    let mut input = Input::default();
    input.set(ACTION, true);
    input.release_all();
    input.set(ACTION, true);
    input.add_pointer_delta(Vec2::ONE);
    input.set_axis(Axis(0), 0.6);
    update(&mut stack, &input);
    let seen = log.borrow();
    assert_eq!(
        seen.iter().map(|s| s.name).collect::<Vec<_>>(),
        ["upper", "lower", "world"]
    );
    assert!(seen[0].down && seen[0].pressed && seen[0].released);
    assert_eq!(seen[0].analog, 0.6);
    for state in &seen[1..] {
        assert!(!state.down && !state.pressed && !state.released);
        assert_eq!(state.delta, Vec2::ZERO);
        assert_eq!(state.analog, 0.0);
        assert_eq!(state.pointer, None);
        assert!(state.reset && state.focused);
    }
}

#[test]
fn transparent_states_receive_input_and_transition_stops_lower_updates_that_tick() {
    let log = Rc::default();
    let mut stack = StateStack {
        entries: vec![
            entry("world", pass(), &log, false),
            entry("overlay", pass(), &log, false),
        ],
        ..StateStack::default()
    };
    let mut input = Input::default();
    input.set(ACTION, true);
    input.set_axis(Axis(0), 0.6);
    update(&mut stack, &input);
    assert!(
        log.borrow()
            .iter()
            .all(|s| s.pressed && s.pointer == Some(Vec2::ONE) && s.analog == 0.6)
    );
    stack.entries[1] = entry("closing", pass(), &log, true);
    log.borrow_mut().clear();
    update(&mut stack, &input);
    assert_eq!(log.borrow().len(), 1);
    assert!(stack.commands.is_pending());
    update(&mut stack, &input);
    assert_eq!(
        log.borrow().len(),
        1,
        "pending work never advances simulation again"
    );
}

#[test]
fn post_transition_guard_blocks_one_tick_then_held_input_resumes() {
    let log = Rc::default();
    let mut stack = StateStack {
        entries: vec![entry("world", pass(), &log, false)],
        suppress_input: true,
        ..StateStack::default()
    };
    let mut input = Input::default();
    input.set(ACTION, true);
    input.add_pointer_delta(Vec2::ONE);
    input.set_axis(Axis(0), 0.6);
    update(&mut stack, &input);
    assert!(!log.borrow()[0].down && !log.borrow()[0].pressed);
    assert_eq!(log.borrow()[0].pointer, None);
    assert_eq!(log.borrow()[0].delta, Vec2::ZERO);
    assert_eq!(log.borrow()[0].analog, 0.0);
    input.consume_edges();
    update(&mut stack, &input);
    assert!(log.borrow()[1].down && !log.borrow()[1].pressed);
    assert_eq!(log.borrow()[1].analog, 0.6);
}

#[test]
fn empty_routing_and_first_request_wins() {
    let mut stack = StateStack::default();
    update(&mut stack, &Input::default());
    assert!(stack.is_empty());
    assert!(stack.request(Transition::Pop).is_ok());
    assert!(matches!(
        stack.request(Transition::Clear),
        Err(Transition::Clear)
    ));
    assert!(matches!(stack.commands.pending, Some(Transition::Pop)));
}

#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_state_lifecycle_resources_and_runner_boundaries() {
    use crate::{App, Config, RunOptions};
    use raylib::prelude::{Color, Image};
    type Log = Rc<RefCell<Vec<String>>>;
    struct Life {
        name: &'static str,
        log: Log,
        fail: bool,
        owned: Rc<RefCell<Vec<TextureId>>>,
        shared: TextureId,
        draw_pop: bool,
        update_pop: bool,
        mesh: Option<MeshId>,
    }
    impl State for Life {
        fn enter(
            &mut self,
            ctx: &mut InitContext<'_, '_>,
            resources: &mut StateResources,
        ) -> Result<(), Error> {
            self.log.borrow_mut().push(format!("enter {}", self.name));
            assert!(ctx.assets.texture(self.shared).is_some());
            let id = ctx.texture_from_image(&Image::gen_image_color(4, 4, Color::BLUE))?;
            resources.own_texture(id);
            self.owned.borrow_mut().push(id);
            self.mesh = Some(resources.own_mesh(ctx.mesh(
                &rayengine_core::mesh::MeshData::new(vec![
                    rayengine_core::glam::Vec3::ZERO,
                    rayengine_core::glam::Vec3::X,
                    rayengine_core::glam::Vec3::Y,
                ]),
            )?));
            resources.own_texture(id);
            resources.own_texture(id); // Duplicate registration remains harmless.
            if self.fail {
                Err(Error::Asset("intentional entry failure".into()))
            } else {
                Ok(())
            }
        }
        fn exit(&mut self, ctx: &mut InitContext<'_, '_>) {
            self.log.borrow_mut().push(format!("exit {}", self.name));
            assert!(
                ctx.assets
                    .texture(*self.owned.borrow().last().unwrap())
                    .is_some()
            );
            assert!(ctx.assets.texture(self.shared).is_some());
            assert!(ctx.assets.mesh(self.mesh.unwrap()).is_some());
        }
        fn policy(&self) -> StatePolicy {
            StatePolicy {
                draw_below: true,
                ..StatePolicy::default()
            }
        }
        fn fixed_update(&mut self, _: &mut Update<'_, '_>, commands: &mut StateCommands) {
            self.log.borrow_mut().push(format!("update {}", self.name));
            if self.update_pop {
                assert!(commands.request(Transition::Pop).is_ok());
                self.update_pop = false;
            }
        }
        fn draw(&mut self, frame: &mut Frame<'_, '_>, commands: &mut StateCommands) {
            self.log.borrow_mut().push(format!("draw {}", self.name));
            frame.ui(|ui| ui.text(self.name, Vec2::splat(10.0), 20.0, Color::WHITE));
            if self.draw_pop && self.log.borrow().iter().any(|e| e == "exit update overlay") {
                assert!(commands.request(Transition::Pop).is_ok());
                self.draw_pop = false;
            }
        }
    }
    struct Harness {
        stack: StateStack,
        log: Log,
        shared: Option<TextureId>,
        handles: Vec<Rc<RefCell<Vec<TextureId>>>>,
        draws: usize,
    }
    impl Game for Harness {
        fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
            let shared = ctx.texture_from_image(&Image::gen_image_color(4, 4, Color::GREEN))?;
            self.shared = Some(shared);
            {
                let mut make = |name, fail, draw_pop, update_pop| {
                    let owned = Rc::new(RefCell::new(Vec::new()));
                    self.handles.push(owned.clone());
                    Box::new(Life {
                        mesh: None,
                        name,
                        fail,
                        draw_pop,
                        update_pop,
                        shared,
                        owned,
                        log: self.log.clone(),
                    }) as Box<dyn State>
                };
                let root = make("root", false, false, false);
                assert!(self.stack.request(Transition::Push(root)).is_ok());
                self.stack.init(ctx)?;
                assert_eq!(&*self.log.borrow(), &["enter root"]);
                for transition in [
                    Transition::Push(make("bad push", true, false, false)),
                    Transition::Replace(make("bad replace", true, false, false)),
                    Transition::Reset(make("bad reset", true, false, false)),
                ] {
                    assert!(self.stack.request(transition).is_ok());
                    self.stack.boundary(ctx)?;
                    assert!(matches!(self.stack.take_error(), Some(Error::Asset(_))));
                    assert!(self.stack.take_error().is_none());
                    assert_eq!(self.stack.len(), 1);
                    assert_eq!(ctx.assets.resource_counts().textures, 2);
                    assert_eq!(ctx.assets.resource_counts().meshes, 1);
                }
                assert_eq!(
                    &self.log.borrow()[1..],
                    &[
                        "enter bad push",
                        "exit bad push",
                        "enter bad replace",
                        "exit bad replace",
                        "enter bad reset",
                        "exit bad reset"
                    ]
                );
                for _ in 0..8 {
                    assert!(
                        self.stack
                            .request(Transition::Push(make("pause", false, false, false)))
                            .is_ok()
                    );
                    self.stack.apply(ctx)?;
                    assert_eq!(self.stack.len(), 2);
                    assert!(self.stack.request(Transition::Pop).is_ok());
                    self.stack.apply(ctx)?;
                    assert_eq!(self.stack.len(), 1);
                    assert_eq!(ctx.assets.resource_counts().textures, 2);
                    assert_eq!(ctx.assets.resource_counts().meshes, 1);
                }
                assert!(
                    self.stack
                        .request(Transition::Replace(make("play", false, false, false)))
                        .is_ok()
                );
                self.stack.apply(ctx)?;
                let tail = self.log.borrow();
                assert_eq!(&tail[tail.len() - 2..], &["enter play", "exit root"]);
                drop(tail);
                assert!(
                    self.stack
                        .request(Transition::Push(make("pause", false, false, false)))
                        .is_ok()
                );
                self.stack.apply(ctx)?;
                assert!(
                    self.stack
                        .request(Transition::Reset(make("title", false, false, false)))
                        .is_ok()
                );
                self.stack.apply(ctx)?;
                let tail = self.log.borrow();
                assert_eq!(
                    &tail[tail.len() - 3..],
                    &["enter title", "exit pause", "exit play"]
                );
                drop(tail);
                assert!(self.stack.request(Transition::Clear).is_ok());
                self.stack.apply(ctx)?;
                assert!(self.stack.request(Transition::Pop).is_ok());
                self.stack.apply(ctx)?;
                assert!(self.stack.is_empty());
                assert_eq!(ctx.assets.resource_counts().textures, 1);
                assert_eq!(ctx.assets.resource_counts().meshes, 0);
            }
            for list in &self.handles {
                for id in list.borrow().iter() {
                    assert!(ctx.assets.texture(*id).is_none());
                }
            }
            let mut make = |name, fail, draw_pop, update_pop| {
                let owned = Rc::new(RefCell::new(Vec::new()));
                self.handles.push(owned.clone());
                Box::new(Life {
                    mesh: None,
                    name,
                    fail,
                    draw_pop,
                    update_pop,
                    shared,
                    owned,
                    log: self.log.clone(),
                }) as Box<dyn State>
            };
            // Real runner boundaries: an update removes one overlay; a draw
            // removes another after the world and overlay have both drawn.
            assert!(
                self.stack
                    .request(Transition::Replace(make("world", false, false, false)))
                    .is_ok()
            );
            self.stack.apply(ctx)?;
            assert!(
                self.stack
                    .request(Transition::Push(make("draw overlay", false, true, false)))
                    .is_ok()
            );
            self.stack.apply(ctx)?;
            assert!(
                self.stack
                    .request(Transition::Push(make("update overlay", false, false, true)))
                    .is_ok()
            );
            self.stack.apply(ctx)?;
            self.log.borrow_mut().clear();
            Ok(())
        }
        fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
            self.stack.fixed_update(ctx);
            if self.draws >= 3 {
                ctx.quit();
            }
        }
        fn draw(&mut self, frame: &mut Frame<'_, '_>) {
            self.draws += 1;
            self.stack.draw(frame);
        }
        fn boundary(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
            self.stack.boundary(ctx)
        }
        fn shutdown(&mut self, ctx: &mut InitContext<'_, '_>) {
            self.stack.shutdown(ctx);
            assert!(self.stack.is_empty());
            assert_eq!(ctx.assets.resource_counts().textures, 1);
            assert_eq!(ctx.assets.resource_counts().meshes, 0);
            assert!(ctx.assets.texture(self.shared.unwrap()).is_some());
            self.log.borrow_mut().push("shutdown".into());
        }
    }
    let log: Log = Rc::default();
    let mut config = Config::new("state lifecycle probe");
    config.vsync = false;
    config.target_fps = 120;
    App::new(config.clone())
        .with_options(RunOptions {
            hidden: true,
            frames: Some(60),
            ..RunOptions::default()
        })
        .run(Harness {
            stack: StateStack::default(),
            log: log.clone(),
            shared: None,
            handles: Vec::new(),
            draws: 0,
        })
        .unwrap();
    let events = log.borrow();
    let pos = |s: &str| events.iter().position(|e| e == s).unwrap();
    assert!(pos("update update overlay") < pos("exit update overlay"));
    assert!(pos("exit update overlay") < pos("update world"));
    assert!(pos("draw world") < pos("draw draw overlay"));
    assert!(pos("draw draw overlay") < pos("exit draw overlay"));
    assert_eq!(&events[events.len() - 2..], &["exit world", "shutdown"]);
    drop(events);

    // Shutdown also runs exactly once after partial init or a boundary error.
    struct Fail {
        init_error: bool,
        shutdowns: Rc<RefCell<usize>>,
    }
    impl Game for Fail {
        fn init(&mut self, _: &mut InitContext<'_, '_>) -> Result<(), Error> {
            if self.init_error {
                Err(Error::Asset("init".into()))
            } else {
                Ok(())
            }
        }
        fn boundary(&mut self, _: &mut InitContext<'_, '_>) -> Result<(), Error> {
            Err(Error::Asset("boundary".into()))
        }
        fn shutdown(&mut self, _: &mut InitContext<'_, '_>) {
            *self.shutdowns.borrow_mut() += 1;
        }
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
        fn draw(&mut self, _: &mut Frame<'_, '_>) {}
    }
    for init_error in [true, false] {
        let shutdowns: Rc<RefCell<usize>> = Rc::default();
        assert!(
            App::new(config.clone())
                .with_options(RunOptions {
                    hidden: true,
                    frames: Some(2),
                    ..RunOptions::default()
                })
                .run(Fail {
                    init_error,
                    shutdowns: shutdowns.clone()
                })
                .is_err()
        );
        assert_eq!(*shutdowns.borrow(), 1);
    }
}
