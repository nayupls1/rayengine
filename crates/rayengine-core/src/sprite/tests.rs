use super::*;

fn region() -> SpriteRegion {
    SpriteRegion::new(4, 8, 16, 12).unwrap()
}
fn clip(mode: PlaybackMode) -> Arc<AnimationClip> {
    Arc::new(
        AnimationClip::new(
            "walk",
            [100, 200, 300]
                .into_iter()
                .map(|ms| SpriteFrame {
                    region: region(),
                    duration: Duration::from_millis(ms),
                })
                .collect(),
            mode,
        )
        .unwrap(),
    )
}

#[test]
fn regions_validate_dimensions_overflow_and_texture_edges() {
    for (x, y, w, h) in [
        (0, 0, 0, 1),
        (0, 0, 1, 0),
        (u32::MAX, 0, 1, 1),
        (0, u32::MAX, 1, 1),
    ] {
        assert_eq!(
            SpriteRegion::new(x, y, w, h),
            Err(SpriteError::InvalidRegion)
        );
    }
    let r = region();
    assert!(r.fits(20, 20));
    assert!(!r.fits(19, 20));
    assert!(!r.fits(20, 19));
    assert!(!r.fits(0, 0));
}

#[test]
fn clips_reject_blank_empty_zero_and_overflowing_timing() {
    assert_eq!(
        AnimationClip::new(" \n", vec![], PlaybackMode::Loop).unwrap_err(),
        SpriteError::EmptyName
    );
    assert_eq!(
        AnimationClip::new("idle", vec![], PlaybackMode::Loop).unwrap_err(),
        SpriteError::EmptyClip
    );
    let frame = SpriteFrame {
        region: region(),
        duration: Duration::ZERO,
    };
    assert_eq!(
        AnimationClip::new("idle", vec![frame], PlaybackMode::Once).unwrap_err(),
        SpriteError::ZeroDuration
    );
    let frames = vec![
        SpriteFrame {
            duration: Duration::MAX,
            ..frame
        },
        SpriteFrame {
            duration: Duration::from_nanos(1),
            ..frame
        },
    ];
    assert_eq!(
        AnimationClip::new("idle", frames, PlaybackMode::Loop).unwrap_err(),
        SpriteError::DurationOverflow
    );
}

#[test]
fn variable_timing_exact_boundaries_and_multiple_loops() {
    let mut p = AnimationPlayer::new(clip(PlaybackMode::Loop));
    assert_eq!(p.frame_index(), 0);
    assert!(p.advance(Duration::from_millis(100)).is_none());
    assert_eq!(p.frame_index(), 1);
    p.advance(Duration::from_millis(199));
    assert_eq!(p.frame_index(), 1);
    p.advance(Duration::from_millis(1));
    assert_eq!(p.frame_index(), 2);
    p.advance(Duration::from_millis(300));
    assert_eq!(p.frame_index(), 0);
    assert_eq!(p.elapsed(), Duration::ZERO);
    p.advance(Duration::from_millis(600 * 10_000 + 350));
    assert_eq!(p.frame_index(), 2);
    assert_eq!(p.elapsed(), Duration::from_millis(350));
    assert!(!p.is_finished());
}

#[test]
fn one_shot_holds_last_frame_and_completes_only_once_until_reset() {
    for dt in [Duration::from_millis(600), Duration::MAX] {
        let mut p = AnimationPlayer::new(clip(PlaybackMode::Once));
        let event = p.advance(dt).unwrap();
        assert_eq!(event.clip.name(), "walk");
        assert!(Arc::ptr_eq(&event.clip, p.clip()));
        assert!(p.is_finished());
        assert_eq!(p.frame_index(), 2);
        assert_eq!(p.elapsed(), p.clip().duration());
        assert!(p.advance(Duration::MAX).is_none());
        p.pause();
        p.resume();
        assert!(p.advance(dt).is_none());
        p.reset();
        assert!(!p.is_finished());
        assert_eq!(p.frame_index(), 0);
        assert!(p.advance(dt).is_some());
    }
}

#[test]
fn pause_resume_reset_and_play_have_explicit_state() {
    let mut p = AnimationPlayer::new(clip(PlaybackMode::Once));
    p.advance(Duration::from_millis(120));
    p.pause();
    assert!(p.advance(Duration::MAX).is_none());
    assert_eq!(p.elapsed(), Duration::from_millis(120));
    p.resume();
    p.advance(Duration::from_millis(180));
    assert_eq!(p.frame_index(), 2);
    p.pause();
    p.reset();
    assert!(p.is_paused());
    assert_eq!(p.elapsed(), Duration::ZERO);
    p.play(clip(PlaybackMode::Loop));
    assert!(!p.is_paused());
    assert!(!p.is_finished());
    assert_eq!(p.frame_index(), 0);
}

#[test]
fn huge_time_steps_and_single_nanosecond_frames_do_not_overflow() {
    let tiny = AnimationClip::new(
        "tiny",
        vec![SpriteFrame {
            region: region(),
            duration: Duration::from_nanos(1),
        }],
        PlaybackMode::Loop,
    )
    .unwrap();
    let mut p = AnimationPlayer::new(tiny);
    assert!(p.advance(Duration::MAX).is_none());
    assert_eq!(p.elapsed(), Duration::ZERO);
    assert_eq!(p.frame_index(), 0);
    let long = AnimationClip::new(
        "long",
        vec![SpriteFrame {
            region: region(),
            duration: Duration::MAX,
        }],
        PlaybackMode::Loop,
    )
    .unwrap();
    p.play(long);
    p.advance(Duration::MAX - Duration::from_nanos(1));
    p.advance(Duration::MAX);
    assert_eq!(p.elapsed(), Duration::MAX - Duration::from_nanos(1));
}

#[test]
fn reads_are_inert_and_shared_players_are_independent() {
    let clip = clip(PlaybackMode::Loop);
    let mut a = AnimationPlayer::new(Arc::clone(&clip));
    let b = AnimationPlayer::new(clip);
    a.advance(Duration::from_millis(350));
    for _ in 0..100 {
        assert_eq!(a.frame_index(), 2);
        assert_eq!(a.frame().region, region());
    }
    assert_eq!(a.elapsed(), Duration::from_millis(350));
    assert_eq!(b.frame_index(), 0);
    assert_eq!(b.elapsed(), Duration::ZERO);
    assert!(a.advance(Duration::ZERO).is_none());
    assert_eq!(a.frame_index(), 2);
}

#[test]
fn transforms_require_finite_geometry_and_positive_size() {
    assert!(SpriteTransform::default().is_valid());
    assert!(
        SpriteTransform {
            origin: Vec2::splat(-100.0),
            flip_x: true,
            flip_y: true,
            ..SpriteTransform::default()
        }
        .is_valid()
    );
    for t in [
        SpriteTransform {
            size: Vec2::ZERO,
            ..SpriteTransform::default()
        },
        SpriteTransform {
            size: Vec2::new(-1.0, 1.0),
            ..SpriteTransform::default()
        },
        SpriteTransform {
            size: Vec2::splat(f32::NAN),
            ..SpriteTransform::default()
        },
        SpriteTransform {
            position: Vec2::splat(f32::INFINITY),
            ..SpriteTransform::default()
        },
        SpriteTransform {
            origin: Vec2::splat(f32::NAN),
            ..SpriteTransform::default()
        },
        SpriteTransform {
            rotation: f32::NAN,
            ..SpriteTransform::default()
        },
        SpriteTransform {
            rotation: f32::MAX,
            ..SpriteTransform::default()
        },
    ] {
        assert!(!t.is_valid());
    }
}
