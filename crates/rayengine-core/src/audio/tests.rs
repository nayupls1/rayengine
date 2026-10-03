use super::*;

#[test]
fn buses_compose_master_mute_and_transient_duck_once() {
    let mut buses = AudioBuses::default();
    let extra = buses.add("dialogue").unwrap();
    assert_eq!(extra, buses.add("dialogue").unwrap());
    assert_eq!(buses.find("music"), Some(BusId::MUSIC));
    buses.set_volume(BusId::MASTER, 0.5).unwrap();
    buses.set_volume(BusId::MUSIC, 0.8).unwrap();
    buses
        .duck(BusId::MUSIC, 0.25, Duration::from_secs(1))
        .unwrap();
    buses.advance(Duration::from_millis(500));
    assert_eq!(buses.gain(BusId::MUSIC).unwrap(), 0.25);
    assert_eq!(buses.gain(BusId::MASTER).unwrap(), 0.5);
    assert_eq!(buses.gain(extra).unwrap(), 0.5);
    buses.set_muted(BusId::MASTER, true).unwrap();
    assert_eq!(buses.gain(extra).unwrap(), 0.0);
    buses.advance(Duration::from_secs(10));
    buses.set_muted(BusId::MASTER, false).unwrap();
    assert_eq!(buses.gain(BusId::MUSIC).unwrap(), 0.1);
    assert_eq!(buses.settings_for(BusId::MUSIC).unwrap().volume, 0.8);
    buses.set_muted(BusId::MUSIC, true).unwrap();
    assert_eq!(buses.gain(BusId::MUSIC).unwrap(), 0.0);
}

#[test]
fn fades_crossfade_retarget_complete_and_partition_independently() {
    let second = Duration::from_secs(1);
    let mut incoming = GainFade::new(0.0, 1.0, second).unwrap();
    let mut outgoing = GainFade::new(1.0, 0.0, second).unwrap();
    for _ in 0..4 {
        incoming.advance(Duration::from_millis(125));
        outgoing.advance(Duration::from_millis(125));
        assert_eq!(incoming.gain() + outgoing.gain(), 1.0);
    }
    assert_eq!(incoming.gain(), 0.5);
    let mut whole = incoming.clone();
    whole.advance(Duration::from_millis(500));
    for _ in 0..5 {
        incoming.advance(Duration::from_millis(100));
    }
    assert_eq!(whole.gain(), incoming.gain());
    assert!(incoming.finished());
    outgoing.retarget(1.0, second).unwrap();
    assert_eq!(outgoing.gain(), 0.5);
    outgoing.advance(Duration::from_millis(500));
    assert_eq!(outgoing.gain(), 0.75);
    outgoing.advance(Duration::MAX);
    assert_eq!(outgoing.gain(), 1.0);
    assert!(outgoing.finished());
    outgoing.retarget(0.0, Duration::ZERO).unwrap();
    assert_eq!(outgoing.gain(), 0.0);
    assert!(outgoing.finished());
}

#[test]
fn invalid_controls_and_restore_are_atomic() {
    let mut buses = AudioBuses::default();
    let before = buses.settings();
    for invalid in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        assert!(buses.set_volume(BusId::MUSIC, invalid).is_err());
        assert!(buses.duck(BusId::MUSIC, invalid, Duration::ZERO).is_err());
        assert!(GainFade::new(invalid, 0.0, Duration::ZERO).is_err());
        let mut settings = before.clone();
        settings
            .buses
            .insert("aaa-new".into(), BusSettings::default());
        settings.buses.get_mut("sfx").unwrap().volume = invalid;
        assert!(buses.apply_settings(&settings).is_err());
        assert_eq!(before, buses.settings());
        assert!(buses.find("aaa-new").is_none());
    }
    assert!(buses.add("  ").is_err());
    assert!(buses.gain(BusId(999)).is_err());
}

#[test]
fn settings_round_trip_through_versioned_save_without_duck() {
    // The engine save owns bytes/schema; this lightweight codec is game-owned.
    let mut buses = AudioBuses::default();
    let extra = buses.add("dialogue").unwrap();
    buses.set_volume(extra, 0.3).unwrap();
    buses.set_muted(BusId::SFX, true).unwrap();
    buses.duck(BusId::MUSIC, 0.1, Duration::ZERO).unwrap();
    let settings = buses.settings();
    let text = toml::to_string(&settings).unwrap();
    let container =
        crate::save::encode(7, text.as_bytes(), crate::save::SaveLimits::default()).unwrap();
    let payload = crate::save::decode(&container, crate::save::SaveLimits::default()).unwrap();
    payload.require_schema(7).unwrap();
    let decoded: AudioSettings =
        toml::from_str(std::str::from_utf8(payload.payload).unwrap()).unwrap();
    let mut restored = AudioBuses::default();
    restored.apply_settings(&decoded).unwrap();
    assert_eq!(restored.settings(), settings);
    assert_eq!(restored.gain(BusId::MUSIC).unwrap(), 1.0);
    assert_eq!(
        restored.gain(restored.find("dialogue").unwrap()).unwrap(),
        0.3
    );
}
