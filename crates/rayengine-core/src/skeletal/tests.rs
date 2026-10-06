use super::*;

fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}
/// 61 glTF keyframes: exactly one second from the first to the last pose.
fn second() -> ClipTiming {
    ClipTiming::new(61, KeyframeRate::GLTF).unwrap()
}

#[test]
fn rates_and_timings_validate_counts_periods_and_overflow() {
    assert_eq!(KeyframeRate::new(0, ms(1)), Err(SkeletalError::InvalidRate));
    assert_eq!(
        KeyframeRate::new(1, Duration::ZERO),
        Err(SkeletalError::InvalidRate)
    );
    assert_eq!(KeyframeRate::GLTF, KeyframeRate::new(60, ms(1000)).unwrap());
    assert_eq!(KeyframeRate::M3D.frames(), 1);
    assert_eq!(KeyframeRate::M3D.period(), ms(17));
    assert_eq!(
        ClipTiming::new(0, KeyframeRate::GLTF),
        Err(SkeletalError::NoKeyframes)
    );
    let slow = KeyframeRate::new(1, Duration::MAX).unwrap();
    assert_eq!(ClipTiming::new(2, slow).unwrap().duration(), Duration::MAX);
    assert_eq!(
        ClipTiming::new(3, slow),
        Err(SkeletalError::DurationOverflow)
    );
    assert_eq!(second().duration(), ms(1000));
    assert_eq!(
        ClipTiming::new(4, KeyframeRate::M3D).unwrap().duration(),
        ms(51)
    );
    // 1/60 s is not a whole nanosecond count; the duration rounds up.
    assert_eq!(
        ClipTiming::new(2, KeyframeRate::GLTF).unwrap().duration(),
        Duration::from_nanos(16_666_667)
    );
    assert!(std::panic::catch_unwind(|| KeyframeRate::per_second(0)).is_err());
}

#[test]
fn skeleton_compatibility_requires_a_skeleton_with_equal_bone_count() {
    assert_eq!(check_skeleton(7, 7), Ok(()));
    assert_eq!(check_skeleton(0, 0), Err(SkeletalError::NoSkeleton));
    assert_eq!(check_skeleton(0, 7), Err(SkeletalError::NoSkeleton));
    let mismatch = check_skeleton(7, 2).unwrap_err();
    assert_eq!(
        mismatch,
        SkeletalError::BoneCountMismatch { model: 7, clip: 2 }
    );
    assert_eq!(
        mismatch.to_string(),
        "skeletal clip has 2 bones but the model skeleton has 7"
    );
}

#[test]
fn keyframes_interpolate_with_exact_time_and_boundaries() {
    let mut p = KeyframePlayer::new('w', second(), PlaybackMode::Loop);
    assert_eq!((p.keyframe(), p.elapsed()), (0.0, Duration::ZERO));
    p.advance(ms(25)); // 1.5 keyframes at 60 Hz
    assert_eq!(p.keyframe(), 1.5);
    assert_eq!(p.elapsed(), ms(25));
    // Thirds of a millisecond accumulate exactly instead of drifting.
    for _ in 0..3 {
        p.advance(Duration::from_nanos(333_333));
    }
    p.advance(Duration::from_nanos(1));
    assert_eq!(p.elapsed(), ms(26));
    let mut exact = KeyframePlayer::new((), second(), PlaybackMode::Loop);
    for _ in 0..60 {
        exact.advance(Duration::from_nanos(16_666_666));
        exact.advance(Duration::from_nanos(1));
    }
    // 60 * 16_666_667 ns = 1.00000002 s, which wraps just past zero.
    assert!(exact.keyframe() < 0.001);
    assert_eq!(exact.elapsed(), Duration::from_nanos(20));
}

#[test]
fn loops_wrap_exact_and_large_steps_without_completion() {
    let mut p = KeyframePlayer::new(1_u8, second(), PlaybackMode::Loop);
    assert_eq!(p.advance(ms(1000)), None);
    assert_eq!(p.keyframe(), 0.0);
    assert_eq!(p.advance(ms(2250)), None);
    assert_eq!(p.keyframe(), 15.0);
    assert_eq!(p.elapsed(), ms(250));
    // Just before the end, a loop is still between the last two keyframes.
    p.reset();
    p.advance(ms(999));
    assert!(p.keyframe() > 59.9 && p.keyframe() < 60.0);
    assert_eq!(p.advance(Duration::MAX), None);
    assert!(p.keyframe() < 60.0);
    assert!(!p.is_finished());
}

#[test]
fn one_shots_complete_once_hold_the_last_pose_and_rearm() {
    let mut p = KeyframePlayer::new("wave", second(), PlaybackMode::Once);
    assert_eq!(p.advance(ms(999)), None);
    assert_eq!(p.advance(ms(1)), Some(ClipCompleted { clip: "wave" }));
    assert!(p.is_finished());
    assert_eq!((p.keyframe(), p.elapsed()), (60.0, ms(1000)));
    assert_eq!(p.advance(ms(500)), None);
    p.pause();
    p.resume();
    assert_eq!(p.advance(ms(500)), None);
    p.reset();
    assert!(!p.is_finished());
    assert_eq!(p.keyframe(), 0.0);
    // Excess time is discarded and still reports one completion.
    assert_eq!(
        p.advance(Duration::MAX),
        Some(ClipCompleted { clip: "wave" })
    );
    assert_eq!(p.keyframe(), 60.0);
    // Rounded-up durations always complete a clip whose span is fractional.
    let short = ClipTiming::new(2, KeyframeRate::GLTF).unwrap();
    let mut q = KeyframePlayer::new(0, short, PlaybackMode::Once);
    assert!(q.advance(short.duration()).is_some());
    assert_eq!(q.elapsed(), short.duration());
}

#[test]
fn pause_reset_and_play_follow_sprite_conventions() {
    let mut p = KeyframePlayer::new(1, second(), PlaybackMode::Loop);
    p.advance(ms(100));
    p.pause();
    assert_eq!(p.advance(ms(400)), None);
    assert_eq!(p.keyframe(), 6.0);
    p.reset();
    assert!(p.is_paused());
    assert_eq!(p.keyframe(), 0.0);
    p.resume();
    assert_eq!(p.advance(Duration::ZERO), None);
    p.advance(ms(50));
    p.pause();
    let walk = ClipTiming::new(31, KeyframeRate::per_second(30)).unwrap();
    p.play(2, walk, PlaybackMode::Once);
    assert_eq!(
        (p.clip(), p.timing(), p.mode()),
        (2, walk, PlaybackMode::Once)
    );
    assert!(!p.is_paused() && !p.is_finished());
    p.advance(ms(500));
    assert_eq!(p.keyframe(), 15.0);
}

#[test]
fn single_keyframe_clips_are_static_poses() {
    let pose = ClipTiming::new(1, KeyframeRate::GLTF).unwrap();
    assert_eq!(pose.duration(), Duration::ZERO);
    let mut held = KeyframePlayer::new((), pose, PlaybackMode::Loop);
    assert_eq!(held.advance(ms(10)), None);
    assert_eq!(held.keyframe(), 0.0);
    let mut once = KeyframePlayer::new((), pose, PlaybackMode::Once);
    // Like sprites, a one-shot completes on its first nonzero advance.
    assert_eq!(once.advance(Duration::ZERO), None);
    assert!(once.advance(Duration::from_nanos(1)).is_some());
    assert_eq!(once.keyframe(), 0.0);
}
