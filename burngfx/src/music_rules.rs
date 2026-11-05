//! v1.2's music between scenes (`SceneManager.ConfigureMusicTransition`):
//! what a scene change does to the song playing, given the map music mode.
//!
//! The scenes come in kinds by the interfaces they wear: the two map
//! scenes are *navigation* (and *continuation*); the wait between turns and
//! the last rival's question are *continuation* alone (the wait also a
//! *bridge*); Options is an *interruption*; everything else is plain.

/// `MapMusicMode`: what plays on the maps, which have no song of their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapMusic {
    /// Silent maps.
    None = 0,
    /// The last scene's song carries on, ends with a faded pass, and the
    /// playlist follows; Options pauses it and gives it back.
    Keep = 1,
    /// The map playlist, paused while away and resumed on return.
    List = 2,
}

impl MapMusic {
    /// `SetMapMusicMode`: the setting's number, None for anything else.
    pub fn parse(value: &str) -> MapMusic {
        match value.trim() {
            "1" => MapMusic::Keep,
            "2" => MapMusic::List,
            _ => MapMusic::None,
        }
    }

    /// `CycleMapMusicMode`.
    pub fn next(self) -> MapMusic {
        match self {
            MapMusic::None => MapMusic::Keep,
            MapMusic::Keep => MapMusic::List,
            MapMusic::List => MapMusic::None,
        }
    }
}

/// What a scene is to the music.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneKind {
    /// The world map or a place (`IMapNavigationScene`).
    Map,
    /// `WaitScene`, `LastRivalScene` (`IMapMusicContinuationScene`).
    Continuation,
    /// `OptionsScene` (`IMapMusicInterruptionScene`).
    Interruption,
    /// Anything else.
    Plain,
}

impl SceneKind {
    fn is_map(self) -> bool {
        matches!(self, SceneKind::Map | SceneKind::Continuation)
    }
}

/// The transition's decisions, in `ConfigureMusicTransition`'s names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Transition {
    pub target_is_map: bool,
    /// The target scene does not stop what plays (`KeepMusic`).
    pub keep_music: bool,
    pub play_playlist: bool,
    pub remember_playlist: bool,
    pub continue_playlist: bool,
    pub remember_song: bool,
    pub resume_remembered_song: bool,
    pub discard_remembered_song: bool,
    /// `SetPlaylistContinuation(false)` before anything else: the target is
    /// no map.
    pub stop_continuation: bool,
    /// `DiscardRememberedPlaylistSong` before anything else: not List.
    pub discard_remembered_playlist: bool,
}

/// `ConfigureMusicTransition`. `from_playlist` is `IsPlayingFromPlaylist`.
pub fn transition(source: SceneKind, target: SceneKind, mode: MapMusic, from_playlist: bool) -> Transition {
    let source_is_map = source.is_map();
    let target_is_map = target.is_map();
    let nav = |k: SceneKind| k == SceneKind::Map;
    let interruption = |k: SceneKind| k == SceneKind::Interruption;
    let mut t = Transition {
        target_is_map,
        continue_playlist: target_is_map && mode != MapMusic::None,
        remember_song: mode == MapMusic::Keep && source_is_map && interruption(target),
        resume_remembered_song: mode == MapMusic::Keep && interruption(source) && target_is_map,
        stop_continuation: !target_is_map,
        discard_remembered_playlist: mode != MapMusic::List,
        ..Transition::default()
    };
    t.discard_remembered_song = (interruption(target) && !t.remember_song) || (interruption(source) && !t.resume_remembered_song);
    if mode == MapMusic::List && source_is_map && !target_is_map {
        t.remember_playlist = true;
    }
    if target_is_map {
        match mode {
            MapMusic::Keep => t.keep_music = !interruption(source),
            MapMusic::List => {
                t.keep_music = nav(source) && nav(target) || source_is_map && from_playlist;
                t.play_playlist = !t.keep_music;
            }
            MapMusic::None => {}
        }
    }
    t
}
