use super::*;
use crate::time::Timer;

#[test]
fn equivalent_simulated_durations_produce_identical_state() {
    fn run(speed: f64, updates: u32) -> (u64, Duration, f32, f32, u32) {
        let mut clock = SimulationClock::new(100, 16);
        clock.set_speed(speed).unwrap();
        let mut timer = Timer::repeating(Duration::from_millis(250));
        let (mut position, mut velocity, mut events) = (0.0, 0.0, 0);
        for _ in 0..updates {
            let plan = clock.advance(Duration::from_millis(10));
            assert!(plan.dropped.is_zero());
            for tick in plan.ticks(clock.step()) {
                assert_eq!(tick.dt, 0.01);
                velocity += 3.0 * tick.dt;
                position += velocity * tick.dt;
                events += timer.advance(clock.step());
            }
        }
        (clock.ticks(), clock.elapsed(), position, velocity, events)
    }
    let expected = run(1.0, 200);
    assert_eq!(expected.0, 200);
    assert_eq!(expected.1, Duration::from_secs(2));
    assert_eq!(expected.4, 8);
    assert_eq!(run(0.25, 800), expected);
    assert_eq!(run(4.0, 50), expected);
    assert_eq!(run(8.0, 25), expected);
}

#[test]
fn pause_preserves_interpolation_and_does_not_bank_time() {
    let mut clock = SimulationClock::new(100, 8);
    clock.advance(Duration::from_millis(15));
    clock.set_paused(true);
    assert!(clock.is_paused());
    clock.set_speed(4.0).unwrap();
    let paused = clock.advance(Duration::MAX);
    assert_eq!(paused.steps, 0);
    assert_eq!(paused.first_tick, 1);
    assert_eq!(paused.alpha, 0.5);
    assert_eq!(paused.dropped, Duration::ZERO);
    assert_eq!(clock.elapsed(), Duration::from_millis(10));
    clock.set_paused(false);
    let resumed = clock.advance(Duration::from_micros(1250));
    assert_eq!(resumed.steps, 1);
    assert_eq!(resumed.first_tick, 1);
    assert_eq!(resumed.alpha, 0.0);
}

#[test]
fn speed_changes_preserve_partial_ticks_and_tick_duration() {
    let mut clock = SimulationClock::new(100, 8);
    clock.set_speed(0.5).unwrap();
    assert_eq!(clock.advance(Duration::from_millis(10)).alpha, 0.5);
    clock.set_speed(2.0).unwrap();
    let plan = clock.advance(Duration::from_millis(10));
    assert_eq!(plan.steps, 2);
    assert_eq!(plan.alpha, 0.5);
    assert_eq!(clock.elapsed(), Duration::from_millis(20));
    assert_eq!(clock.step(), Duration::from_millis(10));
    assert_eq!(
        plan.ticks(clock.step())
            .map(|t| t.index)
            .collect::<Vec<_>>(),
        [0, 1]
    );
}

#[test]
fn scaling_carries_subnanoseconds_across_partitions_and_speed_changes() {
    let mut whole = SimulationClock::new(1000, 1000);
    let mut pieces = SimulationClock::new(1000, 1000);
    whole.set_speed(0.1).unwrap();
    pieces.set_speed(0.1).unwrap();
    let expected = whole.advance(Duration::from_nanos(10_000_009));
    for _ in 0..100_000 {
        pieces.advance(Duration::from_nanos(100));
    }
    for _ in 0..9 {
        pieces.advance(Duration::from_nanos(1));
    }
    assert_eq!(pieces.ticks(), whole.ticks());
    assert_eq!(pieces.alpha(), expected.alpha);
    assert_eq!(pieces.subnanoseconds, whole.subnanoseconds);
    assert_eq!(pieces.subnanoseconds, 900_000);
    pieces.set_paused(true);
    pieces.advance(Duration::MAX);
    pieces.set_paused(false);
    pieces.set_speed(0.5).unwrap();
    pieces.advance(Duration::from_nanos(1));
    assert_eq!(pieces.subnanoseconds, 400_000);
    assert_eq!(pieces.fixed.accumulator, Duration::from_nanos(1));
}

#[test]
fn accelerated_overload_discards_whole_ticks_and_retains_only_fraction() {
    let mut clock = SimulationClock::new(100, 4);
    clock.set_speed(4.0).unwrap();
    let plan = clock.advance(Duration::from_micros(26_250));
    assert_eq!(plan.steps, 4);
    assert_eq!(plan.dropped, Duration::from_millis(60));
    assert_eq!(plan.alpha, 0.5);
    assert_eq!(clock.elapsed(), Duration::from_millis(40));
    assert_eq!(clock.advance(Duration::ZERO).steps, 0);
    let next = clock.advance(Duration::from_micros(1250));
    assert_eq!(next.first_tick, 4);
    assert_eq!(next.steps, 1);
    assert_eq!(next.dropped, Duration::ZERO);
}

#[test]
fn maximum_duration_and_speed_have_bounded_work_without_overflow() {
    for speed in [0.000001, 1.0, 1000.0] {
        let mut clock = SimulationClock::new(1000, 8);
        clock.set_speed(speed).unwrap();
        let plan = clock.advance(Duration::MAX);
        assert_eq!(plan.steps, 8);
        assert!(!plan.dropped.is_zero());
        assert!((0.0..1.0).contains(&plan.alpha));
        assert_eq!(clock.elapsed(), Duration::from_millis(8));
        assert_eq!(clock.advance(Duration::ZERO).steps, 0);
    }
}

#[test]
fn zero_speed_and_zero_elapsed_do_not_advance() {
    let mut clock = SimulationClock::new(100, 4);
    assert_eq!(clock.advance(Duration::ZERO).steps, 0);
    clock.advance(Duration::from_millis(5));
    clock.set_speed(0.0).unwrap();
    assert!(!clock.is_paused());
    assert_eq!(clock.advance(Duration::MAX).alpha, 0.5);
    clock.set_speed(1.0).unwrap();
    assert_eq!(clock.advance(Duration::from_millis(5)).steps, 1);
}

#[test]
fn invalid_speed_is_rejected_without_changes() {
    let mut clock = SimulationClock::new(60, 8);
    clock.set_speed(4.0).unwrap();
    for speed in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -0.1,
        1000.000001,
    ] {
        assert_eq!(clock.set_speed(speed), Err(InvalidSimulationSpeed));
        assert_eq!(clock.speed(), 4.0);
    }
    clock.set_speed(1.2345674).unwrap();
    assert_eq!(clock.speed(), 1.234567);
}

#[test]
fn alpha_stays_below_one_even_at_nanosecond_boundary() {
    let mut clock = SimulationClock::new(1, 1);
    let plan = clock.advance(Duration::from_nanos(999_999_999));
    assert_eq!(plan.steps, 0);
    assert!(plan.alpha < 1.0);
    assert_eq!(plan.alpha, clock.alpha());
}
