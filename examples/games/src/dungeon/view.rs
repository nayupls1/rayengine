use super::{Shared, model::*};
use rayengine::{
    prelude::*,
    raylib::prelude::RaylibDraw,
    render::{Canvas2D, UiCanvas},
};

pub const INK: Color = Color::new(12, 18, 28, 255);
pub const PAPER: Color = Color::new(237, 225, 195, 255);
pub const GOLD: Color = Color::new(235, 181, 94, 255);
pub const MUTED: Color = Color::new(140, 163, 171, 255);
pub const TEAL: Color = Color::new(88, 205, 184, 255);
pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Aabb2 {
    Aabb2 {
        min: Vec2::new(x, y),
        max: Vec2::new(x + w, y + h),
    }
}
pub fn text<D: RaylibDraw>(
    ui: &mut UiCanvas<'_, D>,
    font: FontId,
    s: &str,
    x: f32,
    y: f32,
    size: f32,
    color: Color,
) {
    ui.text_with(s, Vec2::new(x, y), TextStyle::new(font, size), color)
        .expect("valid Ember Mono text");
}
fn diamond<D: RaylibDraw>(world: &mut Canvas2D<'_, D>, p: Vec2, r: f32, c: Color) {
    let points = [
        p + Vec2::new(0.0, -r),
        p + Vec2::new(r, 0.0),
        p + Vec2::new(0.0, r),
        p + Vec2::new(-r, 0.0),
    ];
    for i in 0..4 {
        world.line(points[i], points[(i + 1) % 4], 2.0, c);
    }
}
fn ring<D: RaylibDraw>(world: &mut Canvas2D<'_, D>, p: Vec2, r: f32, c: Color) {
    for i in 0..32 {
        let a = i as f32 / 32.0 * std::f32::consts::TAU;
        let b = (i + 1) as f32 / 32.0 * std::f32::consts::TAU;
        world.line(
            p + Vec2::from_angle(a) * r,
            p + Vec2::from_angle(b) * r,
            1.5,
            c,
        );
    }
}
pub fn camera(room: &Room) -> Camera2D {
    room.shake.apply_2d(Camera2D {
        target: Vec2::new(336.0, 184.0),
        view_height: 640.0,
        ..Default::default()
    })
}
pub fn world_to_ui(p: Vec2) -> Vec2 {
    p + Vec2::new(144.0, 136.0)
}

pub fn draw_world(shared: &Shared, frame: &mut Frame<'_, '_>) {
    let room = &shared.room;
    let alpha = frame.alpha;
    frame.clear(INK);
    frame.world_2d(camera(room), |world| {
        world.rectangle(rect(-8.0, -8.0, 688.0, 437.0), Color::new(5, 10, 19, 255));
        for y in 0..13 {
            for x in 0..21 {
                let at = Vec2::new(x as f32 * TILE, y as f32 * TILE);
                let kind = room.tile_kind(x, y);
                let hash = (x * 17 + y * 31 + x * y * 13) % 13;
                let floor = Color::new(
                    43 + hash as u8 / 2,
                    55 + hash as u8 / 2,
                    63 + hash as u8 / 2,
                    255,
                );
                world.rectangle(rect(at.x, at.y, 32.0, 32.0), Color::new(28, 39, 49, 255));
                world.rectangle(rect(at.x + 1.0, at.y + 1.0, 30.0, 30.0), floor);
                if hash < 3 {
                    world.line(
                        at + Vec2::new(8.0, 7.0),
                        at + Vec2::new(13.0, 11.0),
                        1.0,
                        Color::new(57, 70, 77, 255),
                    );
                }
                if y == 6 && (4..19).contains(&x) {
                    world.rectangle(
                        rect(at.x + 4.0, at.y + 13.0, 3.0, 3.0),
                        Color::new(94, 86, 63, 255),
                    );
                }
                match kind {
                    1 | 4 => {
                        world.rectangle(
                            rect(at.x, at.y + 7.0, 32.0, 30.0),
                            Color::new(13, 22, 33, 255),
                        );
                        world.rectangle(
                            rect(at.x + 1.0, at.y - 3.0, 30.0, 30.0),
                            Color::new(62, 78, 88, 255),
                        );
                        world.rectangle(
                            rect(at.x + 2.0, at.y - 2.0, 28.0, 4.0),
                            Color::new(85, 103, 109, 255),
                        );
                        world.rectangle(
                            rect(at.x + 2.0, at.y + 19.0, 28.0, 6.0),
                            Color::new(38, 51, 66, 255),
                        );
                        world.line(
                            at + Vec2::new(2.0, 10.0),
                            at + Vec2::new(30.0, 10.0),
                            1.0,
                            Color::new(45, 60, 72, 255),
                        );
                        if hash < 5 {
                            world.rectangle(
                                rect(at.x + 3.0, at.y + 3.0, 6.0, 3.0),
                                Color::new(64, 99, 86, 255),
                            );
                        }
                    }
                    2 => {
                        world.rectangle(
                            rect(at.x + 1.0, at.y + 1.0, 30.0, 30.0),
                            Color::new(8, 13, 24, 255),
                        );
                        world.line(
                            at + Vec2::ONE,
                            at + Vec2::new(31.0, 1.0),
                            2.0,
                            Color::new(151, 73, 51, 255),
                        );
                        world.rectangle(
                            rect(
                                at.x + 8.0,
                                at.y + 15.0 + (room.time * 1.4 + x as f32).sin() * 3.0,
                                3.0,
                                2.0,
                            ),
                            Color::new(184, 81, 48, 255),
                        );
                    }
                    3 => {
                        let slide = room.door_slide.value() * 31.0;
                        world.rectangle(rect(at.x, at.y, 32.0, 32.0), Color::new(10, 20, 27, 255));
                        for i in 0..3 {
                            world.rectangle(
                                rect(at.x + 4.0 + i as f32 * 9.0, at.y, 5.0, 32.0 - slide),
                                GOLD,
                            );
                        }
                        world.line(
                            at + Vec2::new(0.0, 33.0),
                            at + Vec2::new(32.0, 33.0),
                            3.0,
                            if room.opened { TEAL } else { GOLD },
                        );
                    }
                    _ => (),
                }
            }
        }
        if room.opened {
            diamond(world, room.door, 10.0, TEAL);
        }
        if let Some(p) = room.plate {
            diamond(world, p, 13.0, if room.plate_latched { TEAL } else { GOLD });
            diamond(world, p, 7.0, if room.plate_latched { TEAL } else { GOLD });
        }
        for (id, _) in &room.blocks {
            let p = room.world.body(*id).unwrap().position;
            world.rectangle(
                rect(p.x - 14.0, p.y - 9.0, 28.0, 28.0),
                Color::new(15, 23, 32, 200),
            );
            world.rectangle(
                rect(p.x - 13.0, p.y - 15.0, 26.0, 26.0),
                Color::new(154, 111, 66, 255),
            );
            world.rectangle(rect(p.x - 10.0, p.y - 12.0, 20.0, 3.0), GOLD);
            diamond(world, p - Vec2::new(0.0, 2.0), 8.0, GOLD);
        }
        let p = room.shrine;
        world.circle(p, 17.0, Color::new(24, 49, 55, 255));
        diamond(world, p, 18.0, if room.shrine_used { MUTED } else { TEAL });
        world.rectangle(
            rect(p.x - 7.0, p.y - 8.0, 14.0, 16.0),
            Color::new(68, 107, 109, 255),
        );
        world.circle(
            p - Vec2::new(0.0, 9.0),
            4.0,
            if room.shrine_used { MUTED } else { TEAL },
        );
        for p in &room.torches {
            world.rectangle(
                rect(p.x - 4.0, p.y - 2.0, 8.0, 13.0),
                Color::new(93, 60, 43, 255),
            );
            let flutter = (room.time * 12.0 + p.x).sin() * 1.5;
            world.circle(
                *p - Vec2::new(0.0, 6.0),
                7.0 + flutter,
                Color::new(231, 113, 47, 255),
            );
            world.circle(*p - Vec2::new(0.0, 7.0), 3.5, GOLD);
        }
        for enemy in &room.enemies {
            let p = enemy
                .previous
                .lerp(room.world.body(enemy.body).unwrap().position, alpha);
            if enemy.windup > 0.0 {
                let radius = if enemy.boss { 86.0 } else { 42.0 };
                world.circle(p, radius, Color::new(195, 55, 59, 35));
                ring(world, p, radius, Color::new(255, 116, 90, 210));
                ring(
                    world,
                    p,
                    radius * (1.0 - enemy.windup / if enemy.boss { 0.65 } else { 0.45 }),
                    GOLD,
                );
            }
            world.circle(
                p + Vec2::new(0.0, 9.0),
                if enemy.boss { 19.0 } else { 13.0 },
                Color::new(8, 14, 24, 150),
            );
            let size = if enemy.boss { 60.0 } else { 40.0 };
            world.sprite(
                shared.actors,
                enemy.animation.player.frame().region,
                SpriteTransform {
                    position: p,
                    size: Vec2::splat(size),
                    origin: Vec2::new(size * 0.5, size * 0.65),
                    flip_x: enemy.facing.x < 0.0,
                    ..Default::default()
                },
                if enemy.flash.value() > 0.4 {
                    Color::new(255, 245, 205, 255)
                } else {
                    Color::WHITE
                },
            );
            let max = if enemy.boss { 12.0 } else { 3.0 };
            world.rectangle(rect(p.x - 14.0, p.y - 29.0, 28.0, 3.0), INK);
            world.rectangle(
                rect(p.x - 14.0, p.y - 29.0, 28.0 * enemy.hp as f32 / max, 3.0),
                Color::new(228, 111, 102, 255),
            );
        }
        let p = room.previous.lerp(room.position(), alpha);
        world.circle(p + Vec2::new(0.0, 9.0), 13.0, Color::new(8, 14, 24, 160));
        if room.dash_time > 0.0 {
            ring(world, p, 19.0, TEAL);
        }
        let tint = if room.flash.value() > 0.2 {
            Color::new(255, 181, 163, 255)
        } else if room.invulnerable > 0.0 && (room.time * 16.0).sin() > 0.0 {
            Color::new(255, 255, 255, 130)
        } else {
            Color::WHITE
        };
        world.sprite(
            shared.actors,
            room.animation.player.frame().region,
            SpriteTransform {
                position: p,
                size: Vec2::splat(40.0),
                origin: Vec2::new(20.0, 26.0),
                flip_x: room.facing.x < 0.0,
                ..Default::default()
            },
            tint,
        );
        let aim = p + room.facing * 27.0;
        diamond(world, aim, 2.5, Color::new(237, 225, 195, 160));
        if room.swing > 0.0 {
            let angle = room.facing.y.atan2(room.facing.x);
            let progress = 1.0 - room.swing / 0.22;
            for i in 0..10 {
                let a = angle - 1.15 + progress * 2.3 - i as f32 * 0.07;
                let b = a - 0.10;
                world.line(
                    p + Vec2::from_angle(a) * 44.0,
                    p + Vec2::from_angle(b) * 44.0,
                    3.0,
                    Color::new(255, 233, 177, 255 - i * 20),
                );
            }
        }
        for e in std::iter::once(&room.sparks)
            .chain(std::iter::once(&room.dust))
            .chain(room.flames.iter())
        {
            for particle in e.particles() {
                let a = e.appearance(particle, alpha);
                let p = particle.interpolated_position(alpha).truncate();
                world.rectangle(
                    Aabb2::from_center(p, Vec2::splat(a.size)),
                    Color::new(
                        (a.color.x * 255.0) as u8,
                        (a.color.y * 255.0) as u8,
                        (a.color.z * 255.0) as u8,
                        (a.color.w * 255.0) as u8,
                    ),
                );
            }
        }
    });
}

pub fn hud(shared: &Shared, frame: &mut Frame<'_, '_>) {
    let r = &shared.room;
    let font = shared.font;
    frame.ui(|ui| {
        text(ui,font,"EMBERVAULT",34.0,24.0,25.0,GOLD);
        text(ui,font,&format!("0{} / {}",r.number+1,NAMES[r.number]),34.0,62.0,18.0,PAPER);
        for i in 0..6 {
            let x=36.0+i as f32*27.0; let c=if i<r.hp { Color::new(239,126,112,255) } else { Color::new(62,54,65,255) };
            ui.rectangle(rect(x,100.0,8.0,5.0),c); ui.rectangle(rect(x+10.0,100.0,8.0,5.0),c);
            ui.rectangle(rect(x,105.0,18.0,5.0),c); ui.rectangle(rect(x+3.0,110.0,12.0,4.0),c); ui.rectangle(rect(x+6.0,114.0,6.0,3.0),c);
        }
        text(ui,font,"DASH",228.0,100.0,13.0,MUTED);
        ui.rectangle(rect(273.0,104.0,81.0,6.0),Color::new(39,56,66,255));
        ui.rectangle(rect(273.0,104.0,81.0*(1.0-r.dash_cooldown/0.85),6.0),TEAL);
        for i in 0..6 { let x=722.0+i as f32*32.0;
            ui.rectangle(rect(x,37.0,20.0,20.0),if i<r.number { TEAL } else if i==r.number { GOLD } else { Color::new(39,53,65,255) });
            if i<5 { ui.rectangle(rect(x+20.0,46.0,12.0,2.0),MUTED); }
        }
        text(ui,font,&format!("{} WATCHERS",r.enemies.len()),733.0,78.0,16.0,MUTED);
        text(ui,font,if r.opened { "GATE OPEN >" } else { "GATE SEALED" },733.0,104.0,15.0,if r.opened { TEAL } else { GOLD });
        let objective=if r.opened { if r.number==5 { "The heart is yours. Step through the golden gate." } else { "Room cleared. Follow the light to the eastern gate." } }
            else if r.enemies.is_empty() && !r.plate_latched { "Push the brass block onto the switch. R / LB resets it." } else { HINTS[r.number] };
        ui.rectangle(rect(34.0,568.0,892.0,1.0),Color::new(67,73,72,255));
        text(ui,font,objective,34.0,581.0,16.0,PAPER);
        let keys=if shared.profile.arrows { "ARROWS" } else { "WASD" };
        let attack=if shared.profile.alternate_attack { "K" } else { "J" };
        text(ui,font,&format!("{keys} MOVE   {attack}/CLICK STRIKE   SPACE DASH   E SHRINE   TAB JOURNAL   ESC PAUSE"),34.0,616.0,12.0,MUTED);
        if r.position().distance(r.shrine)<48.0 && !r.shrine_used { text(ui,font,"E / Y : RESTORE HEARTS",354.0,100.0,14.0,TEAL); }
        if !shared.message.is_empty() { text(ui,font,&shared.message,34.0,544.0,13.0,GOLD); }
    });
}
