use super::*;
use crate::survival::{Item, MAX_HEALTH};
fn focused() -> MenuInput {
    MenuInput {
        ui: UiInput {
            window_focused: true,
            ..Default::default()
        },
        ..Default::default()
    }
}
fn tap(menu: &mut Menu, s: &mut Survival, p: Vec2) -> MenuReport {
    menu.update(
        Vec2::new(960.0, 540.0),
        MenuInput {
            ui: UiInput {
                pointer: Some(p),
                primary: UiButton {
                    pressed: true,
                    down: true,
                    ..Default::default()
                },
                window_focused: true,
                ..Default::default()
            },
            ..Default::default()
        },
        s,
    );
    menu.update(
        Vec2::new(960.0, 540.0),
        MenuInput {
            ui: UiInput {
                pointer: Some(p),
                primary: UiButton {
                    released: true,
                    ..Default::default()
                },
                window_focused: true,
                ..Default::default()
            },
            ..Default::default()
        },
        s,
    )
}
#[test]
fn layout_is_inside_wide_and_portrait_viewports_and_regions_do_not_overlap() {
    for size in [
        Vec2::new(960.0, 540.0),
        Vec2::new(432.0, 540.0),
        Vec2::new(1600.0, 900.0),
        Vec2::new(320.0, 480.0),
    ] {
        let l = Layout::new(size);
        assert!(l.panel.min.min_element() >= 0.0 && l.panel.max.cmple(size).all());
        let boxes: Vec<_> = l
            .slots
            .into_iter()
            .chain(l.recipes)
            .chain([l.close, l.quit])
            .collect();
        for (i, b) in boxes.iter().enumerate() {
            assert!(b.min.cmpge(l.panel.min).all() && b.max.cmple(l.panel.max).all());
            for other in &boxes[..i] {
                assert!(!b.intersects(other));
            }
        }
        for b in l.hotbar {
            assert!(b.min.min_element() >= 0.0 && b.max.cmple(size).all());
        }
    }
}
#[test]
fn opening_closing_death_and_held_buttons_never_leak_into_gameplay() {
    let mut menu = Menu::default();
    let mut s = Survival::default();
    let size = Vec2::new(960.0, 540.0);
    let mut input = focused();
    input.toggle = true;
    input.mining_down = true;
    input.place_down = true;
    input.jump_down = true;
    assert!(menu.update(size, input, &mut s).modal);
    assert!(menu.open());
    assert!(menu.update(size, input, &mut s).modal);
    assert!(!menu.open());
    input.toggle = false;
    let r = menu.update(size, input, &mut s);
    assert!(!r.modal && !r.mining_allowed && !r.place_allowed && !r.jump_allowed);
    let r = menu.update(size, focused(), &mut s);
    assert!(r.mining_allowed && r.place_allowed && r.jump_allowed);
    s.health.damage(MAX_HEALTH);
    assert!(menu.update(size, focused(), &mut s).modal);
    s.health.respawn();
    assert!(menu.update(size, focused(), &mut s).modal);
    assert!(!menu.update(size, focused(), &mut s).modal);
}
#[test]
fn two_click_exchange_crafting_and_focus_loss_use_same_hit_regions() {
    let mut menu = Menu::default();
    let mut s = Survival::default();
    let size = Vec2::new(960.0, 540.0);
    s.inventory.insert(Item::Log, 2);
    menu.update(
        size,
        MenuInput {
            toggle: true,
            ..focused()
        },
        &mut s,
    );
    let l = Layout::new(size);
    tap(&mut menu, &mut s, l.slots[0].center());
    assert_eq!(menu.source(), Some(0));
    tap(&mut menu, &mut s, l.slots[10].center());
    assert_eq!(s.inventory.slots()[10].unwrap().item(), Item::Log);
    assert!(s.inventory.slots()[0].is_none());
    assert!(menu.source().is_none());
    tap(&mut menu, &mut s, l.recipes[0].center());
    assert_eq!(s.inventory.count(Item::Planks), 4);
    tap(&mut menu, &mut s, l.recipes[3].center());
    assert_eq!(s.inventory.count(Item::StonePickaxe), 0);
    tap(&mut menu, &mut s, l.slots[0].center());
    assert_eq!(menu.source(), Some(0));
    menu.update(size, MenuInput::default(), &mut s);
    assert!(menu.source().is_none());
    let r = tap(&mut menu, &mut s, l.close.center());
    assert!(r.modal);
    assert!(!menu.open());
    assert_eq!(s.inventory.count(Item::Planks), 4);
}
#[test]
fn cancelling_a_pointer_press_cannot_craft_on_release_and_death_has_explicit_actions() {
    let mut menu = Menu::default();
    let mut s = Survival::default();
    let size = Vec2::new(960.0, 540.0);
    s.inventory.insert(Item::Log, 1);
    menu.update(
        size,
        MenuInput {
            toggle: true,
            ..focused()
        },
        &mut s,
    );
    let l = Layout::new(size);
    let mut press = focused();
    press.ui.pointer = Some(l.recipes[0].center());
    press.ui.primary = UiButton {
        pressed: true,
        down: true,
        ..Default::default()
    };
    menu.update(size, press, &mut s);
    menu.update(size, MenuInput::default(), &mut s);
    let mut release = focused();
    release.ui.pointer = press.ui.pointer;
    release.ui.primary.released = true;
    menu.update(size, release, &mut s);
    assert_eq!(s.inventory.count(Item::Log), 1);
    s.health.damage(MAX_HEALTH);
    menu.update(size, focused(), &mut s);
    assert!(tap(&mut menu, &mut s, l.close.center()).respawn);
    assert_eq!(s.health.value(), 0); // caller must find a safe spawn before reset
    assert!(tap(&mut menu, &mut s, l.quit.center()).quit);
}
