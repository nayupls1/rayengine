//! Typed game events with explicit tick lifetime and reused storage.

/// An ordered, game-owned event queue. No hidden dispatch or subscriptions.
///
/// Producers call [`Self::send`], readers inspect [`Self::read`], and the game
/// clears or drains the queue at an explicit phase boundary. Several readers
/// can see the same events before clear. Preallocate for hot simulation paths.
///
/// ```
/// use rayengine_core::events::Events;
/// struct Hit { damage: u32 }
/// let mut hits = Events::with_capacity(32);
/// hits.send(Hit { damage: 12 });
/// assert_eq!(hits.read().iter().map(|h| h.damage).sum::<u32>(), 12);
/// // Consume once, retaining capacity for the next tick.
/// assert_eq!(hits.drain().count(), 1);
/// assert!(hits.read().is_empty());
/// ```
#[derive(Debug)]
pub struct Events<T> {
    items: Vec<T>,
}

impl<T> Default for Events<T> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

impl<T> Events<T> {
    /// Empty queue with reserved event capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: Vec::with_capacity(capacity),
        }
    }
    /// Appends a game event. Grows if reserved capacity is exceeded.
    pub fn send(&mut self, event: T) {
        self.items.push(event);
    }
    /// Events in production order. Reading does not consume them.
    pub fn read(&self) -> &[T] {
        &self.items
    }
    /// Consumes events in order, retaining the queue's storage.
    pub fn drain(&mut self) -> std::vec::Drain<'_, T> {
        self.items.drain(..)
    }
    /// Drops all events, retaining storage for the next phase/tick.
    pub fn clear(&mut self) {
        self.items.clear();
    }
    /// Reserved slots, useful when planning an allocation budget.
    pub fn capacity(&self) -> usize {
        self.items.capacity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiple_readers_see_ordered_events_until_the_explicit_boundary() {
        let mut queue = Events::with_capacity(4);
        queue.send(3);
        queue.send(7);
        assert_eq!(queue.read(), &[3, 7]);
        assert_eq!(queue.read().iter().sum::<i32>(), 10);
        assert_eq!(queue.drain().collect::<Vec<_>>(), vec![3, 7]);
        assert!(queue.read().is_empty());
        assert_eq!(queue.capacity(), 4);
        queue.send(9);
        queue.clear();
        assert_eq!(queue.capacity(), 4);
    }
}
