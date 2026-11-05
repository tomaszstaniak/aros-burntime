//! Scene music.
//!
//! **v1.2's playback** (`Burntime.MonoGame/Music/MusicPlayback.cs`), stepped
//! from the game's frame instead of a thread of its own: a song is asked for,
//! what plays fades out over 200 ms (ten steps of 20), the new one fades in
//! over as long; a song is an intro played once and a loop played for ever,
//! or once, or once and faded at its end; a track can be put aside paused
//! and taken up again where it was — one slot for the song Options
//! interrupted (Keep), one for the playlist's track (List). The maps have no
//! song of their own: what plays there is the map music mode's affair
//! (`burngfx::music_rules`). Effects play on voices of their own, at full
//! volume, over the music and over each other.
//!
//! v0.9.1 (the legacy game) used the same calls with no map music; a scene
//! stopped what played and started its own song.
//!
//! **The files are not in the program.** The sets are the remaster's — DOS
//! (`songs_dos.txt`, the AdLib renders) and Amiga (`songs_amiga.txt`) — read
//! from disk at run time, from `music/dos` and `music/amiga`. Which one plays
//! follows `SetMusicMode`: the one asked for if it is there, otherwise DOS,
//! otherwise Amiga.

use alloc::string::String;
use alloc::vec::Vec;
use burngfx::music_rules::{transition, MapMusic, SceneKind};
use sdlmini::audio::{Audio, Playing, Sound};

/// `TransitionStep`, `TransitionInterval`.
const STEP: f32 = 0.1;
const STEP_MS: u32 = 20;

/// A song loaded: what plays, kept alive while the mixer reads it, and its
/// place when it is put aside.
struct Track {
    name: String,
    _intro: Option<Sound>,
    _looped: Sound,
    repeat: bool,
    map_playlist: bool,
    paused: Option<Playing>,
}

#[derive(Clone, Debug, PartialEq)]
enum Request {
    Play { song: String, repeat: bool, map_playlist: bool },
    Stop,
    ResumePlaylist,
}

pub struct Music {
    audio: Option<Audio>,
    /// The chosen set's directory, with a trailing separator.
    dir: String,
    /// `name=file` or `name=intro:loop`, as the list gives them.
    songs: Vec<(String, String)>,
    /// `playlist_*.txt`: the map playlist.
    map_playlist: Vec<String>,
    /// "DOS" or "Amiga", when a set was found and the device opened.
    set: Option<&'static str>,
    mode: MapMusic,
    /// What the music thread's queue (`_playlist`) holds.
    queue: Vec<String>,
    current: Option<Track>,
    repeat: bool,
    map_playback: bool,
    remembered: Option<Track>,
    remembered_song: Option<(String, bool, bool)>,
    remembered_playlist: Option<Track>,
    remembered_playlist_song: Option<String>,
    last_playlist_song: Option<String>,
    continue_map_playlist: bool,
    remember_playlist_on_transition: bool,
    pending: Option<Request>,
    transition_volume: f32,
    /// `music_volume`, 0 to 1.
    volume: f32,
    /// The scene blend (`Music.SceneVolume`).
    scene_volume: f32,
    /// `MusicSilenced`, while the handover's wait is shown.
    silenced: bool,
    /// `IsMuted`: the intro's first two seconds.
    muted: bool,
    /// v0.9.1's fade of the scene blend: from, to, over how long, from when.
    fade: Option<(f32, f32, u32, u32)>,
    next_step: Option<u32>,
    rng: u64,
    /// Effects on their voices, kept alive while they play.
    sounds: Vec<(Sound, usize)>,
    read: fn(&str) -> Option<Vec<u8>>,
}

/// How a sound was played, for the console.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Played {
    /// v0.9.1's `PlayOnce`: the music stopped and the sound played once in
    /// its place.
    InsteadOfMusic,
    /// `PlaySound`: over the music, which goes on.
    OverMusic,
    /// No device, or the file would not decode.
    Silent,
}

/// `ConfigFile`'s reading of a song list: `key=value`, `'` starts a comment.
pub fn parse_songs(text: &str) -> Vec<(String, String)> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .filter_map(|l| {
            let l = l.split('\'').next()?.trim();
            let (k, v) = l.split_once('=')?;
            let (k, v) = (k.trim(), v.trim());
            (!k.is_empty() && !v.is_empty()).then(|| (String::from(k), String::from(v)))
        })
        .collect()
}

/// `LoadPlaylist`: every `songs=` line, once each, in order.
pub fn parse_playlist(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (k, v) in parse_songs(text) {
        if k.eq_ignore_ascii_case("songs") && !out.iter().any(|s| s.eq_ignore_ascii_case(&v)) {
            out.push(v);
        }
    }
    out
}

fn say(line: &str) {
    crate::println_(line);
}

impl Music {
    /// `mode` is the `music` setting: "amiga", "off", or anything else for
    /// the remaster's default. `read` fetches a file's bytes. `seed` starts
    /// the playlist's generator — upstream's is `Random.Shared`, different
    /// every run, and so is the caller's — nought for a fixed one.
    pub fn open(root: &str, mode: &str, read: fn(&str) -> Option<Vec<u8>>, seed: u64) -> (Music, String) {
        let mut m = Music {
            audio: None,
            dir: String::new(),
            songs: Vec::new(),
            map_playlist: Vec::new(),
            set: None,
            mode: MapMusic::None,
            queue: Vec::new(),
            current: None,
            repeat: false,
            map_playback: false,
            remembered: None,
            remembered_song: None,
            remembered_playlist: None,
            remembered_playlist_song: None,
            last_playlist_song: None,
            continue_map_playlist: false,
            remember_playlist_on_transition: false,
            pending: None,
            transition_volume: 1.0,
            volume: 1.0,
            scene_volume: 1.0,
            silenced: false,
            muted: false,
            fade: None,
            next_step: None,
            // xorshift stays at nought for ever: nought is the fixed seed.
            rng: if seed == 0 { 0x9e37_79b9_7f4a_7c15 } else { seed },
            sounds: Vec::new(),
            read,
        };
        if mode == "off" {
            return (m, String::from("music: off"));
        }
        let set = |sub: &str, list: &str, playlist: &str| -> Option<(String, Vec<(String, String)>, Vec<String>)> {
            let mut dir = String::from(root);
            dir.push_str(sub);
            let mut path = dir.clone();
            path.push_str(list);
            let text = read(&path)?;
            let mut pl = dir.clone();
            pl.push_str(playlist);
            let playlist = read(&pl).and_then(|t| String::from_utf8(t).ok()).map(|t| parse_playlist(&t)).unwrap_or_default();
            Some((dir, parse_songs(core::str::from_utf8(&text).ok()?), playlist))
        };
        let dos = || set("dos/", "songs_dos.txt", "playlist_dos.txt");
        let amiga = || set("amiga/", "songs_amiga.txt", "playlist_amiga.txt");
        let chosen = if mode == "amiga" { amiga().map(|s| (s, "Amiga")).or_else(|| dos().map(|s| (s, "DOS"))) } else { None }
            .or_else(|| dos().map(|s| (s, "DOS")))
            .or_else(|| amiga().map(|s| (s, "Amiga")));
        let Some(((dir, songs, playlist), which)) = chosen else {
            let mut line = String::from("music: no set under ");
            line.push_str(root);
            return (m, line);
        };
        match Audio::open() {
            Ok(a) => {
                m.audio = Some(a);
                m.dir = dir;
                m.songs = songs;
                m.map_playlist = playlist;
                m.set = Some(which);
                m.apply_volume();
                let mut line = String::from("music: the ");
                line.push_str(which);
                line.push_str(" set");
                (m, line)
            }
            Err(why) => {
                let mut line = String::from("music: none, ");
                line.push_str(why);
                (m, line)
            }
        }
    }

    /// `CycleMusicMode`: the remaster's set, the Amiga's, off — skipping a
    /// set that is not on disk. Opens the device the first time it is
    /// wanted. Returns the new `music` setting.
    pub fn cycle_set(&mut self, root: &str) -> &'static str {
        let next = match self.mode_name() {
            "off" => "remaster",
            "remaster" => "amiga",
            _ => "off",
        };
        let was = self.playing().map(String::from);
        self.stop_now();
        if next == "off" {
            self.audio = None;
            self.set = None;
            say("music: off");
            return "off";
        }
        self.reopen(root, next, was)
    }

    /// `ToggleMusicMode`, F9 in `Application.OnProcess` (v1.2, every scene):
    /// while music is not off, the remaster's set for the Amiga's and back.
    /// Never to off, and nothing when the other set is not on disk. Returns
    /// the new `music` setting when it changed.
    pub fn toggle_set(&mut self, root: &str) -> Option<&'static str> {
        let (next, list) = match self.mode_name() {
            "remaster" => ("amiga", "amiga/songs_amiga.txt"),
            "amiga" => ("remaster", "dos/songs_dos.txt"),
            _ => return None,
        };
        let mut path = String::from(root);
        path.push_str(list);
        (self.read)(&path)?;
        let was = self.playing().map(String::from);
        self.stop_now();
        Some(self.reopen(root, next, was))
    }

    /// The set `next` opened in place of this one, the song that played (or
    /// the radio) taken up again.
    fn reopen(&mut self, root: &str, next: &str, was: Option<String>) -> &'static str {
        // One device at a time: the old one closes before the new opens.
        self.audio = None;
        let (mut fresh, line) = Music::open(root, next, self.read, self.rng);
        say(&line);
        fresh.mode = self.mode;
        fresh.volume = self.volume;
        *self = fresh;
        self.apply_volume();
        // `ResumeSongOrRadio`: what played, else the radio.
        let song = was.filter(|s| self.can_play(s)).unwrap_or_else(|| String::from("radio"));
        self.play(&song);
        // Amiga asked for and not there: what opened says so.
        self.mode_name()
    }

    /// The `music` setting this playback stands for.
    pub fn mode_name(&self) -> &'static str {
        match self.set {
            None => "off",
            Some("Amiga") => "amiga",
            _ => "remaster",
        }
    }

    fn stop_now(&mut self) {
        if let Some(a) = &self.audio {
            a.stop();
            a.stop_sound();
        }
        self.current = None;
        self.pending = None;
        self.queue.clear();
        self.remembered = None;
        self.remembered_playlist = None;
        self.sounds.clear();
    }

    /// The settings: the map music mode and `music_volume` (0–100).
    pub fn configure(&mut self, mode: MapMusic, volume_percent: i32) {
        self.mode = mode;
        self.volume = volume_percent.clamp(0, 100) as f32 / 100.0;
        self.apply_volume();
    }

    pub fn map_mode(&self) -> MapMusic {
        self.mode
    }

    /// The Options button: the next mode (`CycleMapMusicMode`).
    pub fn cycle_map_mode(&mut self) -> MapMusic {
        self.mode = self.mode.next();
        self.mode
    }

    fn enabled(&self) -> bool {
        self.audio.is_some()
    }

    fn apply_volume(&mut self) {
        let v = if self.silenced || self.muted { 0.0 } else { self.volume * self.scene_volume * self.transition_volume };
        if let Some(a) = &self.audio {
            a.set_volume(v);
        }
    }

    /// `IsMuted`: nothing heard, the song going on underneath
    /// (`IntroScene` mutes its first two seconds).
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.apply_volume();
    }

    fn position(&self) -> usize {
        self.audio.as_ref().and_then(|a| a.position()).unwrap_or(0)
    }

    fn log(&self, what: &str, song: &str, at: Option<usize>) {
        let mut line = String::from("music: ");
        line.push_str(what);
        if !song.is_empty() {
            line.push(' ');
            line.push_str(song);
        }
        if let Some(at) = at {
            line.push_str(" @");
            push_num(&mut line, at);
        }
        say(&line);
    }

    // -- the scenes (`SceneManager.SetScene`, `Scene.ActivateScene`) --------

    /// A scene change: `ConfigureMusicTransition`, the target's activation
    /// (it stops what plays unless it keeps it, and plays its song), then
    /// what the transition asks of the playlist. `wait` is the handover's
    /// `WaitScene` (`ISceneTransitionBridge`): the music is silenced through
    /// it and not stopped.
    pub fn change_scene(&mut self, from: SceneKind, to: SceneKind, song: Option<&str>, looped: bool, from_wait: bool, to_wait: bool) {
        let t = transition(from, to, self.mode, self.is_playing_from_playlist());
        if t.stop_continuation {
            self.set_playlist_continuation(false);
        }
        if t.discard_remembered_playlist {
            self.discard_remembered_playlist_song();
        }
        if t.discard_remembered_song {
            self.discard_remembered_song();
        }
        if t.remember_song {
            self.remember_current_song();
        }
        if t.remember_playlist {
            self.remember_playlist_song();
        }
        let keep = t.target_is_map && t.keep_music;
        if !keep {
            self.stop();
        }
        if let Some(s) = song {
            if looped {
                self.play(s);
            } else {
                self.play_once(s);
            }
        }
        if t.resume_remembered_song {
            self.resume_remembered_song();
        }
        if t.play_playlist {
            self.play_playlist();
        } else {
            self.set_playlist_continuation(t.continue_playlist);
        }
        if to_wait {
            self.silenced = true;
        } else if from_wait {
            self.silenced = false;
        }
        self.apply_volume();
    }

    /// `DialogWindow.OnShow` with `PlayMusic`: "talking", the playlist's
    /// track remembered under List.
    pub fn talk_begins(&mut self) -> bool {
        let resume = self.mode == MapMusic::List && self.is_playing_from_playlist();
        if resume {
            self.remember_playlist_song();
        }
        self.play("talking");
        resume
    }

    /// `DialogWindow.OnHide`: the playlist back under List, silence under
    /// None; under Keep the talking goes on.
    pub fn talk_ends(&mut self, resume: bool) {
        match self.mode {
            MapMusic::List if resume => self.play_playlist(),
            MapMusic::None => self.stop(),
            _ => {}
        }
    }

    // -- `MusicPlayback` ----------------------------------------------------

    pub fn is_playing_from_playlist(&self) -> bool {
        self.map_playback || matches!(self.pending, Some(Request::Play { map_playlist: true, .. }))
    }

    /// `Play(name)`: the song, looping.
    pub fn play(&mut self, name: &str) {
        self.play_with(name, true, false);
    }

    /// `PlayOnce(name)`: the song once.
    pub fn play_once_song(&mut self, name: &str) {
        self.play_with(name, false, false);
    }

    fn play_once(&mut self, name: &str) {
        self.play_once_song(name);
    }

    fn play_with(&mut self, name: &str, repeat: bool, map_playlist: bool) {
        if !self.enabled() {
            return;
        }
        self.request(Request::Play { song: String::from(name), repeat, map_playlist });
    }

    /// `RequestPlayback`: identical playback goes on undisturbed.
    fn request(&mut self, r: Request) {
        self.queue.clear();
        if let (Request::Play { song, repeat, map_playlist }, Some(c)) = (&r, &self.current) {
            if c.name == *song && self.repeat == *repeat && self.map_playback == *map_playlist {
                self.remember_playlist_on_transition = false;
                self.pending = None;
                return;
            }
        }
        self.pending = Some(r);
    }

    /// `Stop`: fade out, then nothing.
    pub fn stop(&mut self) {
        self.queue.clear();
        self.pending = Some(Request::Stop);
    }

    fn remember_current_song(&mut self) {
        self.remembered = None;
        self.remembered_song = None;
        if let Some(mut c) = self.current.take() {
            c.paused = self.audio.as_ref().and_then(|a| a.pause());
            self.log("remember", &c.name, Some(self.position_of(&c)));
            self.remembered_song = Some((c.name.clone(), self.repeat, self.map_playback));
            self.remembered = Some(c);
        } else if let Some(q) = self.queue.first() {
            self.remembered_song = Some((q.clone(), self.repeat, self.map_playback));
        }
        self.queue.clear();
        self.map_playback = false;
    }

    fn position_of(&self, t: &Track) -> usize {
        t.paused.as_ref().map_or(0, |p| p.position())
    }

    fn resume_remembered_song(&mut self) {
        if !self.enabled() {
            self.remembered = None;
            self.remembered_song = None;
            return;
        }
        if let (Some(mut t), Some((name, repeat, was_map))) = (self.remembered.take(), self.remembered_song.take()) {
            self.queue.clear();
            self.pending = None;
            if let (Some(a), Some(p)) = (&self.audio, t.paused.take()) {
                self.log("resume", &name, Some(p.position()));
                a.resume(p);
            }
            self.current = Some(t);
            self.repeat = repeat;
            self.map_playback = was_map;
            self.apply_volume();
            return;
        }
        if let Some((name, repeat, was_map)) = self.remembered_song.take() {
            if self.can_play(&name) {
                self.queue.clear();
                self.queue.push(name);
                self.repeat = repeat;
                self.map_playback = was_map;
            }
        }
    }

    fn discard_remembered_song(&mut self) {
        self.remembered_song = None;
        self.remembered = None;
    }

    fn remember_playlist_song(&mut self) {
        if self.map_playback {
            if let Some(c) = &self.current {
                self.remembered_playlist = None;
                self.remembered_playlist_song = Some(c.name.clone());
                self.queue.clear();
                self.remember_playlist_on_transition = true;
                return;
            }
        }
        if let Some(Request::Play { song, map_playlist: true, .. }) = &self.pending {
            self.remembered_playlist_song = Some(song.clone());
            self.pending = None;
            return;
        }
        if self.map_playback && !self.queue.is_empty() {
            self.remembered_playlist_song = Some(self.queue[0].clone());
            self.queue.clear();
            self.map_playback = false;
            return;
        }
        if self.remembered_playlist.is_none() && self.remembered_playlist_song.is_none() {
            self.remembered_playlist_song = self.last_playlist_song.clone();
        }
    }

    fn discard_remembered_playlist_song(&mut self) {
        self.remember_playlist_on_transition = false;
        self.remembered_playlist_song = None;
        self.remembered_playlist = None;
    }

    fn set_playlist_continuation(&mut self, enabled: bool) {
        self.continue_map_playlist = enabled;
        if enabled {
            self.repeat = false;
            if self.current.is_some() {
                if let Some(a) = &self.audio {
                    a.disable_loop();
                }
            }
            if let Some(Request::Play { repeat, map_playlist, .. }) = &mut self.pending {
                *repeat = false;
                *map_playlist = true;
            }
            if self.current.is_none() && self.queue.is_empty() && self.remembered_playlist_song.is_none() {
                self.queue_random_playlist_song();
            }
        }
    }

    fn play_playlist(&mut self) {
        if !self.enabled() || self.map_playlist.is_empty() {
            self.discard_remembered_playlist_song();
            self.stop();
            return;
        }
        self.continue_map_playlist = true;
        if self.remember_playlist_on_transition && self.current.is_some() && self.map_playback {
            self.remember_playlist_on_transition = false;
            self.pending = None;
            return;
        }
        if self.remembered_playlist.is_some() && self.remembered_playlist_song.as_deref().is_some_and(|s| self.can_play(s)) {
            self.queue.clear();
            self.pending = Some(Request::ResumePlaylist);
            return;
        }
        self.remembered_playlist = None;
        if let Some(s) = self.remembered_playlist_song.take().filter(|s| self.can_play(s)) {
            self.request(Request::Play { song: s, repeat: false, map_playlist: true });
            return;
        }
        let s = self.pick_playlist_song();
        self.request(Request::Play { song: s, repeat: false, map_playlist: true });
    }

    /// `PickPlaylistSong`: at random, never the last one twice.
    fn pick_playlist_song(&mut self) -> String {
        loop {
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 7;
            self.rng ^= self.rng << 17;
            let s = self.map_playlist[(self.rng % self.map_playlist.len() as u64) as usize].clone();
            if self.map_playlist.len() == 1 || self.last_playlist_song.as_deref() != Some(&s) {
                self.last_playlist_song = Some(s.clone());
                return s;
            }
        }
    }

    fn queue_random_playlist_song(&mut self) {
        if !self.enabled() || self.map_playlist.is_empty() {
            return;
        }
        let s = self.pick_playlist_song();
        self.queue.clear();
        self.repeat = false;
        self.queue.push(s);
        self.map_playback = true;
    }

    fn can_play(&self, name: &str) -> bool {
        name.starts_with("sounds/") || self.songs.iter().any(|(k, _)| k == name)
    }

    /// The song's files, decoded.
    fn load(&self, name: &str) -> Option<(Option<Sound>, Sound)> {
        let audio = self.audio.as_ref()?;
        // A scene's `Music` can be a file of the sounds (`sounds/trader.ogg`
        // before two traders), not a song of the list.
        if let Some(file) = name.strip_prefix("sounds/") {
            let mut path = crate::sounds_root();
            path.push_str(file);
            return Some((None, (self.read)(&path).and_then(|bytes| audio.decode(&bytes))?));
        }
        let (_, value) = self.songs.iter().find(|(k, _)| k == name)?;
        let (intro, looped) = match value.split_once(':') {
            Some((i, l)) => (Some(i), l),
            None => (None, value.as_str()),
        };
        let load = |file: &str| {
            let mut path = self.dir.clone();
            path.push_str(file);
            (self.read)(&path).and_then(|bytes| audio.decode(&bytes))
        };
        let looped = load(looped)?;
        Some((intro.and_then(load), looped))
    }

    fn start(&mut self, name: &str, repeat: bool, map_playlist: bool) {
        let fade = map_playlist && self.continue_map_playlist;
        let Some((intro, looped)) = self.load(name) else {
            self.log("missing", name, None);
            return;
        };
        if let Some(a) = &self.audio {
            a.play_track(intro.as_ref(), &looped, repeat, fade);
        }
        self.repeat = repeat;
        self.map_playback = map_playlist;
        self.current = Some(Track { name: String::from(name), _intro: intro, _looped: looped, repeat, map_playlist, paused: None });
        self.log(if map_playlist { "playlist" } else { "play" }, name, None);
    }

    /// `FinishCurrentTrack`: gone, or — a playlist track leaving the map
    /// under List — put aside paused.
    fn finish_current(&mut self) {
        if let Some(mut c) = self.current.take() {
            if self.remember_playlist_on_transition && self.map_playback {
                c.paused = self.audio.as_ref().and_then(|a| a.pause());
                self.log("remember", &c.name, Some(self.position_of(&c)));
                self.remembered_playlist_song = Some(c.name.clone());
                self.remembered_playlist = Some(c);
            } else if let Some(a) = &self.audio {
                a.stop();
                self.log("stop", &c.name, None);
            }
        }
        self.map_playback = false;
        self.remember_playlist_on_transition = false;
        self.transition_volume = 1.0;
    }

    fn start_request(&mut self, r: Request) {
        match r {
            Request::Stop => {}
            Request::ResumePlaylist => {
                if let (Some(mut t), Some(name)) = (self.remembered_playlist.take(), self.remembered_playlist_song.take()) {
                    if let (Some(a), Some(p)) = (&self.audio, t.paused.take()) {
                        self.log("resume", &name, Some(p.position()));
                        a.resume(p);
                    }
                    t.repeat = false;
                    t.map_playlist = true;
                    self.current = Some(t);
                    self.repeat = false;
                    self.map_playback = true;
                    self.transition_volume = 0.0;
                    self.apply_volume();
                }
            }
            Request::Play { song, repeat, map_playlist } => {
                self.start(&song, repeat, map_playlist);
                self.transition_volume = 0.0;
                self.apply_volume();
            }
        }
    }

    /// `UpdateTransition`: a request fades out what plays and then starts;
    /// otherwise what plays fades in.
    fn update_transition(&mut self) -> bool {
        if let Some(r) = self.pending.clone() {
            if self.current.is_some() && self.transition_volume > 0.0 {
                self.transition_volume = (self.transition_volume - STEP).max(0.0);
                self.apply_volume();
                return true;
            }
            self.finish_current();
            self.pending = None;
            self.start_request(r);
            return true;
        }
        if self.current.is_some() && self.transition_volume < 1.0 {
            self.transition_volume = (self.transition_volume + STEP).min(1.0);
            self.apply_volume();
        }
        false
    }

    /// One `MusicThread` step.
    fn step(&mut self) {
        if let Some(a) = &self.audio {
            self.sounds.retain(|(_, voice)| a.sound_playing(*voice));
        }
        if self.update_transition() {
            return;
        }
        if self.current.is_none() {
            if self.queue.is_empty() {
                return;
            }
            let next = self.queue.remove(0);
            if self.repeat {
                self.queue.push(next.clone());
            }
            let map = self.map_playback;
            self.start(&next, self.repeat, map);
            self.apply_volume();
        } else if !self.audio.as_ref().is_some_and(|a| a.is_playing()) {
            if let Some(c) = self.current.take() {
                self.log("ended", &c.name, None);
            }
            self.map_playback = false;
            if self.continue_map_playlist {
                self.queue_random_playlist_song();
            }
        }
    }

    /// Called every frame: the thread's 20 ms steps that came due, and the
    /// scene blend's fade.
    pub fn tick(&mut self, now: u32) {
        if let Some((from, to, ms, start)) = self.fade {
            let t = (now.wrapping_sub(start) as f32 / ms as f32).min(1.0);
            self.scene_volume = from + (to - from) * t;
            self.apply_volume();
            if t >= 1.0 {
                self.fade = None;
            }
        }
        let due = *self.next_step.get_or_insert(now);
        let mut steps = 0;
        let mut at = due;
        while now.wrapping_sub(at) as i32 >= 0 && steps < 50 {
            self.step();
            at = at.wrapping_add(STEP_MS);
            steps += 1;
        }
        self.next_step = Some(if steps == 50 { now.wrapping_add(STEP_MS) } else { at });
    }

    /// Fade the scene blend to `to` over `ms` (`Music.Volume = 1 -
    /// BlendState`).
    pub fn fade_to(&mut self, to: f32, ms: u32, now: u32) {
        self.fade = Some((self.scene_volume, to, ms.max(1), now));
    }

    // -- effects ------------------------------------------------------------

    /// v0.9.1's `PlayOnce(file)`: the music stopped and the file played once
    /// in its place. `path` is a file, not a song name.
    pub fn play_file_once(&mut self, path: &str) -> Played {
        let Some(audio) = &self.audio else { return Played::Silent };
        let Some(sound) = (self.read)(path).and_then(|b| audio.decode(&b)) else { return Played::Silent };
        audio.stop();
        audio.play_track(None, &sound, false, false);
        self.current = Some(Track { name: String::from("\u{0}once"), _intro: None, _looped: sound, repeat: false, map_playlist: false, paused: None });
        self.queue.clear();
        self.pending = None;
        self.transition_volume = 1.0;
        self.apply_volume();
        Played::InsteadOfMusic
    }

    /// `PlaySound(file)`: a voice of its own, at full volume, over whatever
    /// the music is doing and over the other sounds.
    pub fn sound(&mut self, path: &str) -> Played {
        let Some(audio) = &self.audio else { return Played::Silent };
        let Some(sound) = (self.read)(path).and_then(|b| audio.decode(&b)) else { return Played::Silent };
        let voice = audio.play_sound(&sound);
        // A voice taken from an older sound frees that sound.
        self.sounds.retain(|(_, v)| *v != voice);
        self.sounds.push((sound, voice));
        let mut line = String::from("music: sound ");
        line.push_str(path.rsplit('/').next().unwrap_or(path));
        say(&line);
        Played::OverMusic
    }

    // -- for the Options page -------------------------------------------------

    /// `Music.Songlist`, in the list's order.
    pub fn song_names(&self) -> Vec<String> {
        self.songs.iter().map(|(k, _)| k.clone()).collect()
    }

    /// `!DisableMusic`: a set's song list is on disk under `root`, whether
    /// or not one is playing.
    pub fn on_disk(&self, root: &str) -> bool {
        ["dos/songs_dos.txt", "amiga/songs_amiga.txt"].iter().any(|list| {
            let mut path = String::from(root);
            path.push_str(list);
            (self.read)(&path).is_some()
        })
    }

    pub fn set_name(&self) -> Option<&'static str> {
        self.set
    }

    pub fn playing(&self) -> Option<&str> {
        self.current.as_ref().map(|c| c.name.as_str())
    }
}

impl Drop for Music {
    fn drop(&mut self) {
        if let Some(a) = &self.audio {
            a.stop();
            a.stop_sound();
        }
    }
}

fn push_num(s: &mut String, mut v: usize) {
    let mut digits = [0u8; 20];
    let mut n = 0;
    loop {
        digits[n] = b'0' + (v % 10) as u8;
        n += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    for d in digits[..n].iter().rev() {
        s.push(*d as char);
    }
}

/// `PickPlaylistSong`'s order: upstream's `Random.Shared` differs every run
/// (`MusicPlayback.cs:349-359`), and so does this with another seed; the
/// fixed seed (nought, a scripted run's) is the order every run had before.
#[cfg(test)]
mod playlist {
    use super::Music;
    use alloc::string::String;
    use alloc::vec::Vec;

    fn order(seed: u64) -> Vec<String> {
        let (mut m, _) = Music::open("", "off", |_| None, seed);
        m.map_playlist = (0..6).map(|i| alloc::format!("song{i}")).collect();
        (0..30).map(|_| m.pick_playlist_song()).collect()
    }

    #[test]
    fn the_order_follows_the_seed_and_never_repeats_a_song() {
        assert_ne!(order(0x1234_5678), order(0x8765_4321));
        assert_eq!(order(0), order(0x9e37_79b9_7f4a_7c15));
        let o = order(42);
        assert!(o.windows(2).all(|w| w[0] != w[1]));
    }
}
