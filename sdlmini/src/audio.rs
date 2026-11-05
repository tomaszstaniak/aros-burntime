//! Sound through SDL2_mixer, as little of it as the game needs.
//!
//! The mixer decodes: `Mix_LoadWAV_RW` turns an Ogg file in memory into PCM
//! in the device's format (the AROS SDK's `libSDL2_mixer.a` carries
//! `stb_vorbis` for it). Music is then played by our own hook rather than by
//! `Mix_PlayMusic`, because v0.9.1's songs are an intro followed by a loop
//! with no gap between them — `LoopableSong` queues the loop buffer the
//! moment the intro's runs out — and the hook can do exactly that.
//!
//! On a host without SDL2_mixer the build leaves it out (`build.rs`) and
//! [`Audio::open`] says so; the game then runs silent rather than not at all.

#[cfg(any(target_os = "aros", sdlmini_mixer))]
use core::ffi::c_void;
use core::sync::atomic::{AtomicU32, Ordering};

#[cfg(any(target_os = "aros", sdlmini_mixer))]
mod ffi {
    use core::ffi::c_void;

    #[repr(C)]
    pub struct MixChunk {
        pub allocated: i32,
        pub abuf: *mut u8,
        pub alen: u32,
        pub volume: u8,
    }

    pub type Hook = Option<unsafe extern "C" fn(*mut c_void, *mut u8, i32)>;

    // On AROS build.rs links SDL2_mixer, ahead of the SDL2 it calls into.
    #[cfg_attr(not(target_os = "aros"), link(name = "SDL2_mixer"))]
    extern "C" {
        pub fn Mix_OpenAudio(frequency: i32, format: u16, channels: i32, chunksize: i32) -> i32;
        pub fn Mix_QuerySpec(frequency: *mut i32, format: *mut u16, channels: *mut i32) -> i32;
        pub fn Mix_LoadWAV_RW(src: *mut c_void, freesrc: i32) -> *mut MixChunk;
        pub fn Mix_FreeChunk(chunk: *mut MixChunk);
        pub fn Mix_HookMusic(hook: Hook, arg: *mut c_void);
        pub fn Mix_CloseAudio();
    }
    extern "C" {
        pub fn SDL_InitSubSystem(flags: u32) -> i32;
        pub fn SDL_RWFromConstMem(mem: *const c_void, size: i32) -> *mut c_void;
        pub fn SDL_LockAudio();
        pub fn SDL_UnlockAudio();
    }
}

/// Decoded sound, in the device's format.
pub struct Sound {
    #[cfg(any(target_os = "aros", sdlmini_mixer))]
    chunk: *mut ffi::MixChunk,
}

impl Sound {
    fn pcm(&self) -> &[i16] {
        #[cfg(any(target_os = "aros", sdlmini_mixer))]
        unsafe {
            let c = &*self.chunk;
            return core::slice::from_raw_parts(c.abuf as *const i16, c.alen as usize / 2);
        }
        #[allow(unreachable_code)]
        &[]
    }
}

impl Drop for Sound {
    fn drop(&mut self) {
        #[cfg(any(target_os = "aros", sdlmini_mixer))]
        unsafe {
            ffi::Mix_FreeChunk(self.chunk)
        }
    }
}

/// What the hook plays: an intro once, then the loop — for ever while
/// `loop_enabled`, then (v1.2's `LoopableSong`) one more pass faded out over
/// its last fifth of a second when there is one to come, and then nothing.
/// Shared with the audio thread; changed only with the audio locked.
pub struct Playing {
    intro: *const i16,
    intro_len: usize,
    looped: *const i16,
    loop_len: usize,
    /// Position: in the intro while `< intro_len`, then in the loop.
    at: usize,
    loop_enabled: bool,
    /// A faded pass is to follow when the loop is let go (`DisableLoop`).
    fade_pending: bool,
    /// This pass is the faded one: the last.
    fading: bool,
}

impl Playing {
    /// Where a paused track stands, in samples from its start.
    pub fn position(&self) -> usize {
        self.at
    }
}

/// The faded pass's fade (`OggSoundEffect.FadeSeconds`), in samples of
/// 44.1 kHz stereo.
const FADE_SAMPLES: usize = 44_100 / 5 * 2;

static VOLUME: AtomicU32 = AtomicU32::new(0);

static mut PLAYING: Option<Playing> = None;

/// Effect voices (v1.2's `PlaySound`: each on its own, at full volume, over
/// the music and over each other): a sound's samples and how far it is.
const VOICES: usize = 8;
static mut SOUNDS: [Option<(*const i16, usize, usize)>; VOICES] = [None; VOICES];

#[cfg(any(target_os = "aros", sdlmini_mixer))]
unsafe extern "C" fn hook(_arg: *mut c_void, stream: *mut u8, len: i32) {
    let out = core::slice::from_raw_parts_mut(stream as *mut i16, len as usize / 2);
    let volume = VOLUME.load(Ordering::Relaxed) as i32; // 0..=256
    let playing = &mut *core::ptr::addr_of_mut!(PLAYING);
    for s in out.iter_mut() {
        let v = match playing {
            None => 0,
            Some(p) => {
                let sample = if p.at < p.intro_len {
                    *p.intro.add(p.at) as i32
                } else if p.loop_len > 0 {
                    let mut i = p.at - p.intro_len;
                    if i >= p.loop_len {
                        // The end of a pass: round again, the faded last
                        // pass, or the end.
                        if p.loop_enabled {
                            p.at = p.intro_len;
                        } else if p.fade_pending && !p.fading {
                            p.at = p.intro_len;
                            p.fading = true;
                        } else {
                            *playing = None;
                            *s = 0;
                            continue;
                        }
                        i = 0;
                    }
                    let raw = *p.looped.add(i) as i32;
                    let left = p.loop_len - i;
                    if p.fading && left <= FADE_SAMPLES {
                        raw * left as i32 / FADE_SAMPLES as i32
                    } else {
                        raw
                    }
                } else {
                    *playing = None;
                    *s = 0;
                    continue;
                };
                p.at += 1;
                sample
            }
        };
        *s = ((v * volume) >> 8) as i16;
    }
    let sounds = &mut *core::ptr::addr_of_mut!(SOUNDS);
    for voice in sounds.iter_mut() {
        let Some((pcm, len, at)) = voice else { continue };
        for s in out.iter_mut() {
            if *at >= *len {
                break;
            }
            *s = (*s as i32 + *pcm.add(*at) as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            *at += 1;
        }
        if *at >= *len {
            *voice = None;
        }
    }
}

pub struct Audio {
    _private: (),
}

/// `SDL_INIT_AUDIO`.
const INIT_AUDIO: u32 = 0x10;
/// `AUDIO_S16LSB`: every target here is little-endian.
const S16: u16 = 0x8010;

impl Audio {
    /// Open the device at 44.1 kHz, 16-bit stereo, which is what the Ogg
    /// sets are. `Err` says why there is no sound.
    pub fn open() -> Result<Audio, &'static str> {
        #[cfg(any(target_os = "aros", sdlmini_mixer))]
        unsafe {
            if ffi::SDL_InitSubSystem(INIT_AUDIO) != 0 {
                return Err("SDL audio did not start");
            }
            if ffi::Mix_OpenAudio(44100, S16, 2, 2048) != 0 {
                return Err("Mix_OpenAudio failed");
            }
            let (mut f, mut fmt, mut ch) = (0, 0u16, 0);
            ffi::Mix_QuerySpec(&mut f, &mut fmt, &mut ch);
            if fmt != S16 || ch != 2 {
                ffi::Mix_CloseAudio();
                return Err("the audio device is not 16-bit stereo");
            }
            ffi::Mix_HookMusic(Some(hook), core::ptr::null_mut());
            return Ok(Audio { _private: () });
        }
        #[allow(unreachable_code)]
        Err("built without SDL2_mixer")
    }

    /// Decode an Ogg (or WAV) file held in memory.
    pub fn decode(&self, bytes: &[u8]) -> Option<Sound> {
        #[cfg(any(target_os = "aros", sdlmini_mixer))]
        unsafe {
            let rw = ffi::SDL_RWFromConstMem(bytes.as_ptr() as *const c_void, bytes.len() as i32);
            if rw.is_null() {
                return None;
            }
            let chunk = ffi::Mix_LoadWAV_RW(rw, 1);
            return (!chunk.is_null()).then_some(Sound { chunk });
        }
        #[allow(unreachable_code)]
        {
            let _ = bytes;
            None
        }
    }

    /// Play `intro` once and then `looped`: for ever while `repeat` (and,
    /// when the loop is let go, one faded pass more); with `fade` and no
    /// `repeat`, the loop once and faded; else the loop once. The sounds
    /// must outlive the playing: keep them until [`Audio::stop`] or until
    /// the track is paused and dropped.
    pub fn play_track(&self, intro: Option<&Sound>, looped: &Sound, repeat: bool, fade: bool) {
        let (ip, il) = intro.map_or((core::ptr::null(), 0), |s| (s.pcm().as_ptr(), s.pcm().len()));
        let l = looped.pcm();
        self.set(Some(Playing {
            intro: ip,
            intro_len: il,
            looped: l.as_ptr(),
            loop_len: l.len(),
            at: 0,
            loop_enabled: repeat,
            fade_pending: repeat,
            fading: !repeat && fade,
        }));
    }

    /// v0.9.1's `LoopableSong`: an intro, then the loop for ever (or once).
    pub fn play(&self, intro: Option<&Sound>, looped: &Sound, repeat: bool) {
        self.play_track(intro, looped, repeat, false);
    }

    pub fn stop(&self) {
        self.set(None);
    }

    /// `DisableLoop`: the pass under way is the last full one; a looping
    /// track then plays its faded pass and ends.
    pub fn disable_loop(&self) {
        self.with(|p| {
            if let Some(p) = p {
                p.loop_enabled = false;
            }
        });
    }

    /// `Pause`: the track leaves the hook with its place kept. The sounds
    /// it plays must be kept alive as long as the paused track is.
    pub fn pause(&self) -> Option<Playing> {
        let mut out = None;
        self.with(|p| out = p.take());
        out
    }

    /// `Resume`: a paused track carries on where it was.
    pub fn resume(&self, track: Playing) {
        self.set(Some(track));
    }

    /// Where the track is, in samples from its start; `None` when nothing
    /// plays.
    pub fn position(&self) -> Option<usize> {
        let mut out = None;
        self.with(|p| out = p.as_ref().map(|p| p.at));
        out
    }

    /// Lay `sound` over whatever is playing, once, on a free voice (the
    /// oldest when all are busy). It must outlive the playing: the voice is
    /// returned so that its owner can tell when it is done.
    pub fn play_sound(&self, sound: &Sound) -> usize {
        let pcm = sound.pcm();
        let mut chosen = 0;
        #[cfg(any(target_os = "aros", sdlmini_mixer))]
        unsafe {
            ffi::SDL_LockAudio();
            let sounds = &mut *core::ptr::addr_of_mut!(SOUNDS);
            chosen = sounds.iter().position(|v| v.is_none()).unwrap_or_else(|| {
                (0..VOICES).max_by_key(|&i| sounds[i].map_or(0, |(_, _, at)| at)).unwrap_or(0)
            });
            sounds[chosen] = Some((pcm.as_ptr(), pcm.len(), 0));
            ffi::SDL_UnlockAudio();
        }
        #[cfg(not(any(target_os = "aros", sdlmini_mixer)))]
        let _ = pcm;
        chosen
    }

    /// Whether a voice still plays.
    pub fn sound_playing(&self, voice: usize) -> bool {
        #[cfg(any(target_os = "aros", sdlmini_mixer))]
        unsafe {
            ffi::SDL_LockAudio();
            let on = (*core::ptr::addr_of!(SOUNDS)).get(voice).is_some_and(|v| v.is_some());
            ffi::SDL_UnlockAudio();
            return on;
        }
        #[allow(unreachable_code)]
        {
            let _ = voice;
            false
        }
    }

    /// Stop every effect voice, before the sounds they play are dropped.
    pub fn stop_sound(&self) {
        #[cfg(any(target_os = "aros", sdlmini_mixer))]
        unsafe {
            ffi::SDL_LockAudio();
            *core::ptr::addr_of_mut!(SOUNDS) = [None; VOICES];
            ffi::SDL_UnlockAudio();
        }
    }

    /// 0.0 to 1.0.
    pub fn set_volume(&self, v: f32) {
        VOLUME.store((v.clamp(0.0, 1.0) * 256.0) as u32, Ordering::Relaxed);
    }

    /// Whether the hook has anything left to play.
    pub fn is_playing(&self) -> bool {
        #[cfg(any(target_os = "aros", sdlmini_mixer))]
        unsafe {
            ffi::SDL_LockAudio();
            let on = (*core::ptr::addr_of!(PLAYING)).is_some();
            ffi::SDL_UnlockAudio();
            return on;
        }
        #[allow(unreachable_code)]
        false
    }

    fn with(&self, f: impl FnOnce(&mut Option<Playing>)) {
        #[cfg(any(target_os = "aros", sdlmini_mixer))]
        unsafe {
            ffi::SDL_LockAudio();
            f(&mut *core::ptr::addr_of_mut!(PLAYING));
            ffi::SDL_UnlockAudio();
        }
        #[cfg(not(any(target_os = "aros", sdlmini_mixer)))]
        {
            let mut none = None;
            f(&mut none);
        }
    }

    fn set(&self, p: Option<Playing>) {
        #[cfg(any(target_os = "aros", sdlmini_mixer))]
        unsafe {
            ffi::SDL_LockAudio();
            *core::ptr::addr_of_mut!(PLAYING) = p;
            ffi::SDL_UnlockAudio();
        }
        #[cfg(not(any(target_os = "aros", sdlmini_mixer)))]
        let _ = p;
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        self.stop();
        self.stop_sound();
        #[cfg(any(target_os = "aros", sdlmini_mixer))]
        unsafe {
            ffi::Mix_HookMusic(None, core::ptr::null_mut());
            ffi::Mix_CloseAudio();
        }
    }
}
