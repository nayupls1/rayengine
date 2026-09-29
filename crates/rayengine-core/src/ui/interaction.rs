//! Interaction only: game-owned region IDs/layout and no renderer dependency.

use crate::{
    collision::Aabb2,
    input::{Action, Input},
};
use glam::Vec2;

/// Stable game-owned identity. IDs must be unique within an update's regions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UiId(pub u64);

/// Hit region in reference UI units. Last listed region wins overlapping hits,
/// including disabled regions, which block clicks reaching lower regions.
#[derive(Clone, Copy, Debug)]
pub struct UiRegion {
    /// Stable identity, independent of region order.
    pub id: UiId,
    /// Resolved bounds using the current viewport's logical size.
    pub bounds: Aabb2,
    /// Disabled regions cannot activate, focus, or begin dragging.
    pub enabled: bool,
    /// Eligible for pointer and keyboard focus.
    pub focusable: bool,
    /// Starts a drag immediately on pointer press; does not activate on release.
    pub draggable: bool,
}

impl UiRegion {
    /// Enabled, focusable button/region. Set `draggable` for a drag surface.
    pub fn new(id: UiId, bounds: Aabb2) -> Self {
        Self {
            id,
            bounds,
            enabled: true,
            focusable: true,
            draggable: false,
        }
    }
}

/// Physical bindings remain game-owned; these actions drive the interaction state.
#[derive(Clone, Copy, Debug)]
pub struct UiActions {
    /// Pointer's primary button action.
    pub primary: Action,
    /// Move focus forward in region order.
    pub next: Action,
    /// Move focus backward in region order.
    pub previous: Action,
    /// Activate the focused region on a press edge.
    pub activate: Action,
    /// Cancel pointer capture and clear keyboard focus.
    pub cancel: Action,
}

/// Button snapshot, retaining quick taps with both edges between fixed ticks.
#[derive(Clone, Copy, Debug, Default)]
pub struct UiButton {
    /// Currently held.
    pub down: bool,
    /// Pressed since the previous consumed tick.
    pub pressed: bool,
    /// Released since the previous consumed tick.
    pub released: bool,
}

/// One fixed tick's UI input. Supply viewport-mapped positions; `None` denotes
/// bars/unavailable motion. Focus loss cancels interactions and clears focus.
#[derive(Clone, Copy, Debug, Default)]
pub struct UiInput {
    /// Pointer in reference UI units, when available.
    pub pointer: Option<Vec2>,
    /// Primary pointer button.
    pub primary: UiButton,
    /// Forward focus press edge.
    pub next: bool,
    /// Backward focus press edge.
    pub previous: bool,
    /// Focused activation press edge.
    pub activate: bool,
    /// Cancellation press edge.
    pub cancel: bool,
    /// Whether the native window can accept input.
    pub window_focused: bool,
}

impl UiInput {
    /// Samples game-defined actions without consuming the original input.
    /// A pending input reset cancels interaction even if focus already returned.
    pub fn from_actions(
        input: &Input,
        pointer: Option<Vec2>,
        actions: UiActions,
        window_focused: bool,
    ) -> Self {
        Self {
            pointer,
            primary: UiButton {
                down: input.down(actions.primary),
                pressed: input.pressed(actions.primary),
                released: input.released(actions.primary),
            },
            next: input.pressed(actions.next),
            previous: input.pressed(actions.previous),
            activate: input.pressed(actions.activate),
            cancel: input.pressed(actions.cancel),
            window_focused: window_focused && !input.reset_pending(),
        }
    }
}

/// Ownership signals for explicit gameplay routing. Modal games can block all
/// gameplay actions; overlays can block only shared click/navigation actions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiCapture {
    /// A region is hovered or owns this tick's pointer press/drag/release.
    pub pointer: bool,
    /// A region has keyboard focus or this tick cancelled an existing focus.
    pub keyboard: bool,
}

/// Current presentation state and one-tick events for an identified region.
#[derive(Clone, Copy, Debug)]
pub struct UiResponse {
    /// Original stable identity.
    pub id: UiId,
    /// Whether the current region is enabled.
    pub enabled: bool,
    /// Topmost pointer hit, including disabled regions.
    pub hovered: bool,
    /// Keyboard focus.
    pub focused: bool,
    /// Pointer press began here.
    pub pressed: bool,
    /// Pointer is held by this region, including outside its bounds.
    pub held: bool,
    /// Pointer capture ended, including cancellation.
    pub released: bool,
    /// Button release inside its region, or focused keyboard activation.
    pub activated: bool,
    /// Drag began on this tick.
    pub drag_started: bool,
    /// UI-unit motion since the preceding update; zero for unavailable positions.
    pub drag_delta: Vec2,
    /// UI-unit motion from drag start to the latest available position.
    pub drag_total: Vec2,
    /// Drag ended, including cancellation.
    pub drag_ended: bool,
    /// Capture cancelled by focus loss, removal, disable, cancel, or lost button state.
    pub cancelled: bool,
}

impl UiResponse {
    fn new(id: UiId, enabled: bool) -> Self {
        Self {
            id,
            enabled,
            hovered: false,
            focused: false,
            pressed: false,
            held: false,
            released: false,
            activated: false,
            drag_started: false,
            drag_delta: Vec2::ZERO,
            drag_total: Vec2::ZERO,
            drag_ended: false,
            cancelled: false,
        }
    }
}

#[derive(Clone, Copy)]
struct Active {
    id: UiId,
    draggable: bool,
    origin: Vec2,
    last: Vec2,
}

/// Small immediate UI interaction state. Response storage is reused after its
/// high-water mark; hit testing/navigation is linear in the submitted regions.
/// Update once per fixed tick before gameplay, then draw the responses.
#[derive(Default)]
pub struct UiState {
    focus: Option<UiId>,
    active: Option<Active>,
    responses: Vec<UiResponse>,
}

impl UiState {
    /// Preallocates response storage, including one removed-capture cancellation.
    pub fn with_capacity(regions: usize) -> Self {
        Self {
            responses: Vec::with_capacity(regions.saturating_add(1)),
            ..Self::default()
        }
    }

    /// Focused region, retained by identity across reordering and resizing.
    pub fn focused(&self) -> Option<UiId> {
        self.focus
    }

    /// Current response. A removed captured region is retained for one update
    /// with `cancelled = true`, allowing game-owned drag cleanup.
    pub fn response(&self, id: UiId) -> Option<&UiResponse> {
        self.responses.iter().find(|response| response.id == id)
    }

    /// All current responses in region order, then any removed capture event.
    pub fn responses(&self) -> &[UiResponse] {
        &self.responses
    }

    /// Resolves focus and pointer capture using the current layout. Releasing
    /// outside does not activate a button; drags retain capture outside/bars.
    /// Simultaneous next/previous presses cancel navigation. Pointer capture
    /// suspends keyboard navigation/activation until it ends.
    pub fn update(&mut self, regions: &[UiRegion], input: UiInput) -> UiCapture {
        debug_assert!(
            regions
                .iter()
                .enumerate()
                .all(|(i, r)| regions[..i].iter().all(|other| other.id != r.id)),
            "UI region IDs must be unique"
        );
        self.responses.clear();
        self.responses
            .extend(regions.iter().map(|r| UiResponse::new(r.id, r.enabled)));
        let had_focus = self.focus.is_some();
        let had_active = self.active.is_some();
        if self.focus.is_some_and(|id| {
            !regions
                .iter()
                .any(|r| r.id == id && r.enabled && r.focusable)
        }) {
            self.focus = None;
        }
        let invalid_active = self
            .active
            .is_some_and(|a| !regions.iter().any(|r| r.id == a.id && r.enabled));
        if (invalid_active || !input.window_focused || input.cancel)
            && let Some(active) = self.active.take()
        {
            if self.response(active.id).is_none() {
                self.responses.push(UiResponse::new(active.id, false));
            }
            let response = self.response_mut(active.id);
            response.released = true;
            response.cancelled = true;
            response.drag_ended = active.draggable;
            response.drag_total = active.last - active.origin;
        }
        if !input.window_focused || input.cancel {
            self.focus = None;
        }
        if !input.window_focused {
            return UiCapture {
                pointer: had_active,
                keyboard: had_focus,
            };
        }
        let pointer = input.pointer.filter(|p| p.is_finite());
        let hovered = pointer.and_then(|p| regions.iter().rposition(|r| r.bounds.contains(p)));
        if !input.cancel && self.active.is_none() && input.next != input.previous {
            let current = self
                .focus
                .and_then(|id| regions.iter().position(|r| r.id == id));
            let count = regions.len();
            for offset in 0..count {
                let index = if input.next {
                    current.map_or(offset, |i| (i + 1 + offset) % count)
                } else {
                    current.map_or(count - 1 - offset, |i| (i + count - 1 - offset) % count)
                };
                if regions[index].enabled && regions[index].focusable {
                    self.focus = Some(regions[index].id);
                    break;
                }
            }
        }
        if !input.cancel && input.primary.pressed && self.active.is_none() {
            self.focus = None;
            if let Some(index) = hovered.filter(|&i| regions[i].enabled) {
                let region = regions[index];
                if region.focusable {
                    self.focus = Some(region.id);
                }
                let position = pointer.expect("hover requires a pointer");
                self.active = Some(Active {
                    id: region.id,
                    draggable: region.draggable,
                    origin: position,
                    last: position,
                });
                let response = self.response_mut(region.id);
                response.pressed = true;
                response.drag_started = region.draggable;
            }
        }
        if let Some(mut active) = self.active {
            let response = self.response_mut(active.id);
            response.held = input.primary.down;
            if active.draggable {
                if let Some(position) = pointer {
                    response.drag_delta = position - active.last;
                    active.last = position;
                }
                response.drag_total = active.last - active.origin;
            }
            if input.primary.released || !input.primary.down {
                response.released = true;
                response.held = false;
                response.drag_ended = active.draggable;
                response.cancelled = !input.primary.released;
                response.activated = !active.draggable
                    && input.primary.released
                    && hovered.is_some_and(|i| regions[i].id == active.id);
                self.active = None;
            } else {
                self.active = Some(active);
            }
        }
        if !input.cancel
            && !input.primary.pressed
            && self.active.is_none()
            && input.activate
            && let Some(id) = self.focus
        {
            // A drag surface's Enter/Space press can still be handled by game code.
            self.response_mut(id).activated = true;
        }
        if let Some(index) = hovered {
            self.responses[index].hovered = true;
        }
        if let Some(id) = self.focus {
            self.response_mut(id).focused = true;
        }
        UiCapture {
            pointer: hovered.is_some() || had_active || self.active.is_some(),
            keyboard: self.focus.is_some() || (had_focus && input.cancel),
        }
    }

    fn response_mut(&mut self, id: UiId) -> &mut UiResponse {
        self.responses
            .iter_mut()
            .find(|r| r.id == id)
            .expect("active/focused region has a response")
    }
}

#[cfg(test)]
mod tests;
