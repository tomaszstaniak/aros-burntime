//! `SceneManager.ConfigureMusicTransition`, case by case as v1.2 has it.

use burngfx::music_rules::{transition, MapMusic, SceneKind as K, Transition};

#[test]
fn none_leaves_the_maps_silent() {
    // Into INFO: the continuation stops; nothing is remembered.
    let t = transition(K::Map, K::Plain, MapMusic::None, false);
    assert!(t.stop_continuation && t.discard_remembered_playlist);
    assert!(!t.keep_music && !t.remember_song && !t.remember_playlist);
    // Back to the map: nothing kept, no playlist — the map has no song.
    let t = transition(K::Plain, K::Map, MapMusic::None, false);
    assert_eq!(t, Transition { target_is_map: true, discard_remembered_playlist: true, ..Transition::default() });
}

#[test]
fn keep_carries_a_scene_song_onto_the_map_and_pauses_for_options() {
    // INFO's song goes on into the map, which lets its loop go (the
    // continuation) so that the playlist follows its faded last pass.
    let t = transition(K::Plain, K::Map, MapMusic::Keep, false);
    assert!(t.keep_music && t.continue_playlist && !t.play_playlist);
    // Options remembers the map's song and gives it back.
    let t = transition(K::Map, K::Interruption, MapMusic::Keep, false);
    assert!(t.remember_song && !t.discard_remembered_song && t.stop_continuation);
    let t = transition(K::Interruption, K::Map, MapMusic::Keep, false);
    assert!(t.resume_remembered_song && !t.keep_music && !t.discard_remembered_song);
    // Options from INFO is not the map's: nothing to remember.
    let t = transition(K::Plain, K::Interruption, MapMusic::Keep, false);
    assert!(!t.remember_song && t.discard_remembered_song);
}

#[test]
fn list_pauses_the_playlist_away_from_the_map() {
    let t = transition(K::Map, K::Plain, MapMusic::List, true);
    assert!(t.remember_playlist && !t.discard_remembered_playlist);
    // Back: not from a playlist track (INFO's song), so the playlist plays
    // — the remembered track first.
    let t = transition(K::Plain, K::Map, MapMusic::List, false);
    assert!(t.play_playlist && !t.keep_music && t.continue_playlist);
    // World map and place: the track goes on.
    let t = transition(K::Map, K::Map, MapMusic::List, true);
    assert!(t.keep_music && !t.play_playlist && !t.remember_playlist);
    let t = transition(K::Map, K::Map, MapMusic::List, false);
    assert!(t.keep_music, "between the maps even a queued track stays");
    // A handover's wait keeps a playlist track going.
    let t = transition(K::Map, K::Continuation, MapMusic::List, true);
    assert!(t.keep_music && !t.play_playlist);
    let t = transition(K::Map, K::Continuation, MapMusic::List, false);
    assert!(!t.keep_music && t.play_playlist);
    // Options under List: the playlist is remembered, a song is not.
    let t = transition(K::Map, K::Interruption, MapMusic::List, true);
    assert!(t.remember_playlist && !t.remember_song && t.discard_remembered_song);
}

#[test]
fn the_mode_reads_and_cycles_as_the_setting_does() {
    assert_eq!(MapMusic::parse("2"), MapMusic::List);
    assert_eq!(MapMusic::parse("keep"), MapMusic::None);
    assert_eq!(MapMusic::None.next().next().next(), MapMusic::None);
}
