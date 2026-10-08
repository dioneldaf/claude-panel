//! Plays the synthesized notification cues. The toasts themselves are silent (see
//! `engine::show_notice`), so this is the only sound the panel makes.

use cpanel_core::sound::{self, Cue};
use std::sync::OnceLock;

/// WAV files built once and kept for the whole process: with `SND_ASYNC` the
/// system keeps reading the buffer after `PlaySoundW` has returned.
fn buffer(cue: Cue) -> &'static [u8] {
    static NEEDS_YOU: OnceLock<Vec<u8>> = OnceLock::new();
    static DONE: OnceLock<Vec<u8>> = OnceLock::new();
    let cell = match cue {
        Cue::NeedsYou => &NEEDS_YOU,
        Cue::Done => &DONE,
    };
    cell.get_or_init(|| sound::wav(cue))
}

/// Starts the cue and returns at once. A new cue replaces one still playing, so
/// cues never overlap. Returns whether the system accepted the sound; failure is
/// never fatal.
#[cfg(windows)]
pub fn play(cue: Cue) -> bool {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    let wav = buffer(cue);
    // SAFETY: with SND_MEMORY the first argument points to a complete WAV image,
    // which lives in a process-wide static and is never freed or mutated.
    unsafe { PlaySoundW(wav.as_ptr().cast(), std::ptr::null_mut(), SND_MEMORY | SND_ASYNC | SND_NODEFAULT) != 0 }
}

#[cfg(not(windows))]
pub fn play(cue: Cue) -> bool {
    let _ = buffer(cue);
    false
}

/// Hidden `--play-sounds`: plays every cue once with a pause in between. Returns
/// the process exit code: 0 when the system accepted every cue, 1 otherwise.
pub fn preview() -> i32 {
    let mut ok = true;
    for cue in Cue::ALL {
        let accepted = play(cue);
        println!("{}: {}", cue.name(), if accepted { "PlaySoundW accepted the sound" } else { "PlaySoundW failed" });
        ok &= accepted;
        // Asynchronous playback: stay alive until the cue has finished, plus a pause.
        let ms = sound::samples(cue).len() as u64 * 1000 / sound::SAMPLE_RATE as u64;
        std::thread::sleep(std::time::Duration::from_millis(ms + 700));
    }
    if ok {
        0
    } else {
        1
    }
}
