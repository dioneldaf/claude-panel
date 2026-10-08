//! Notification sounds: short, original chime cues synthesized in code. Pure: this
//! module builds PCM samples and in-memory WAV files; playing them is the
//! application's job.

use crate::notify::{Notice, NoticeKind};

/// Output sample rate (mono, 16-bit PCM).
pub const SAMPLE_RATE: u32 = 44_100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cue {
    /// Urgent: a session waits for a permission or an answer.
    NeedsYou,
    /// Friendly: a session finished its turn.
    Done,
}

impl Cue {
    pub const ALL: [Cue; 2] = [Cue::NeedsYou, Cue::Done];

    pub fn for_kind(kind: NoticeKind) -> Self {
        match kind {
            NoticeKind::NeedsYou => Cue::NeedsYou,
            NoticeKind::Done => Cue::Done,
        }
    }

    /// One cue per batch of notices, the most urgent one, so sounds never stack.
    pub fn for_notices(notices: &[Notice]) -> Option<Self> {
        let mut cues = notices.iter().map(|n| Cue::for_kind(n.kind));
        let first = cues.next()?;
        Some(if first == Cue::NeedsYou || cues.any(|c| c == Cue::NeedsYou) { Cue::NeedsYou } else { first })
    }

    /// Name used by the command line and logs.
    pub fn name(self) -> &'static str {
        match self {
            Cue::NeedsYou => "needs_you",
            Cue::Done => "done",
        }
    }
}

/// One mallet strike: a pitch, when it starts, and its relative loudness.
struct Strike {
    freq: f64,
    at_ms: f64,
    gain: f64,
}

const fn strike(freq: f64, at_ms: f64, gain: f64) -> Strike {
    Strike { freq, at_ms, gain }
}

/// A cue: strikes that ring and overlap, rendered for `length_ms`.
struct Chime {
    strikes: &'static [Strike],
    /// Time constant of the fundamental's exponential decay.
    decay_ms: f64,
    length_ms: f64,
}

// Equal-tempered pitches (A4 = 440 Hz).
const A4: f64 = 440.00;
const CS5: f64 = 554.37;
const E5: f64 = 659.25;

/// Raised-cosine attack: soft mallet, no step.
const ATTACK_MS: f64 = 10.0;
/// Raised-cosine fade at the end of the buffer, so the ring stops at zero.
const TAIL_FADE_MS: f64 = 80.0;
/// Second harmonic at -18 dB; decays faster than the fundamental.
const SECOND_HARMONIC: f64 = 0.126;
/// Inharmonic bar partial (2.76x) for a marimba-like knock; very quiet and short.
const BAR_RATIO: f64 = 2.76;
const BAR_PARTIAL: f64 = 0.04;
/// One-pole low-pass that rounds off the attack.
const LOW_PASS_HZ: f64 = 2500.0;
/// Final peak level: -11 dBFS.
const PEAK: f64 = 0.2818;

/// Urgent: a rising A major chime, A4, C#5, E5, struck 120 ms apart; 900 ms in all.
const NEEDS_YOU: Chime = Chime {
    strikes: &[strike(A4, 0.0, 0.85), strike(CS5, 120.0, 0.9), strike(E5, 240.0, 1.0)],
    decay_ms: 170.0,
    length_ms: 900.0,
};

/// Friendly: a descending "ding-dong", E5 then A4 160 ms later; 650 ms in all.
const DONE: Chime = Chime { strikes: &[strike(E5, 0.0, 1.0), strike(A4, 160.0, 0.9)], decay_ms: 180.0, length_ms: 650.0 };

/// The cue's PCM samples: sine voices with two quiet partials, low-passed and
/// normalised to [`PEAK`].
pub fn samples(cue: Cue) -> Vec<i16> {
    use std::f64::consts::PI;
    let chime = match cue {
        Cue::NeedsYou => &NEEDS_YOU,
        Cue::Done => &DONE,
    };
    let rate = SAMPLE_RATE as f64;
    let len = (chime.length_ms * rate / 1000.0) as usize;
    let mut mix = vec![0.0f64; len];
    for s in chime.strikes {
        let start = (s.at_ms * rate / 1000.0) as usize;
        for (i, out) in mix.iter_mut().enumerate().skip(start) {
            let t = (i - start) as f64 / rate;
            let t_ms = t * 1000.0;
            let attack = if t_ms < ATTACK_MS { 0.5 - 0.5 * (PI * t_ms / ATTACK_MS).cos() } else { 1.0 };
            let ring = (-t_ms / chime.decay_ms).exp();
            let phase = 2.0 * PI * s.freq * t;
            let voice = phase.sin()
                + SECOND_HARMONIC * (2.0 * phase).sin() * (-t_ms / (chime.decay_ms * 0.5)).exp()
                + BAR_PARTIAL * (BAR_RATIO * phase).sin() * (-t_ms / (chime.decay_ms * 0.2)).exp();
            *out += s.gain * attack * ring * voice;
        }
    }
    let alpha = 1.0 - (-2.0 * PI * LOW_PASS_HZ / rate).exp();
    let mut state = 0.0;
    let fade = TAIL_FADE_MS * rate / 1000.0;
    for (i, x) in mix.iter_mut().enumerate() {
        state += alpha * (*x - state);
        let left = (len - 1 - i) as f64;
        let tail = if left < fade { 0.5 - 0.5 * (PI * left / fade).cos() } else { 1.0 };
        *x = state * tail;
    }
    let peak = mix.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    let scale = if peak > 0.0 { PEAK / peak } else { 0.0 };
    mix.iter().map(|x| (x * scale * 32767.0).round() as i16).collect()
}

/// The cue as a complete RIFF/WAVE file.
pub fn wav(cue: Cue) -> Vec<u8> {
    encode_wav(&samples(cue), SAMPLE_RATE)
}

/// Encodes mono 16-bit PCM samples as a RIFF/WAVE file.
pub fn encode_wav(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

/// Global rate limit for sounds across all sessions: a cue is dropped when another
/// one started less than `min_gap_ms` earlier.
pub struct SoundGate {
    min_gap_ms: u64,
    last: Option<u64>,
}

impl SoundGate {
    pub fn new(min_gap_ms: u64) -> Self {
        Self { min_gap_ms, last: None }
    }

    /// True when a cue may start now; records it as started.
    pub fn allow(&mut self, now: u64) -> bool {
        if self.last.is_some_and(|at| now.saturating_sub(at) < self.min_gap_ms) {
            return false;
        }
        self.last = Some(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16_at(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([bytes[at], bytes[at + 1]])
    }

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    }

    fn duration_ms(cue: Cue) -> u64 {
        samples(cue).len() as u64 * 1000 / SAMPLE_RATE as u64
    }

    fn peak_dbfs(cue: Cue) -> f64 {
        let peak = samples(cue).iter().map(|s| (*s as i32).unsigned_abs()).max().unwrap_or(0);
        20.0 * (peak as f64 / 32767.0).log10()
    }

    /// Magnitude of one frequency component (Goertzel), normalised by length.
    fn magnitude(samples: &[i16], freq: f64) -> f64 {
        let w = 2.0 * std::f64::consts::PI * freq / SAMPLE_RATE as f64;
        let coeff = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        for x in samples {
            let s0 = *x as f64 + coeff * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0).sqrt() / samples.len() as f64
    }

    /// Strongest component on a 10 Hz grid in `[from, to)`: (frequency, magnitude).
    fn strongest(samples: &[i16], from: u32, to: u32) -> (f64, f64) {
        (from..to)
            .step_by(10)
            .map(|f| (f as f64, magnitude(samples, f as f64)))
            .fold((0.0, 0.0), |best, cur| if cur.1 > best.1 { cur } else { best })
    }

    fn notice(kind: NoticeKind) -> Notice {
        Notice { kind, session_id: "s".into(), name: "n".into(), folder: "f".into() }
    }

    #[test]
    fn wav_header_describes_mono_16_bit_pcm() {
        for cue in Cue::ALL {
            let bytes = wav(cue);
            assert!(bytes.len() > 44, "{cue:?}");
            assert_eq!(&bytes[0..4], b"RIFF");
            assert_eq!(u32_at(&bytes, 4) as usize, bytes.len() - 8);
            assert_eq!(&bytes[8..12], b"WAVE");
            assert_eq!(&bytes[12..16], b"fmt ");
            assert_eq!(u32_at(&bytes, 16), 16);
            assert_eq!(u16_at(&bytes, 20), 1, "PCM");
            assert_eq!(u16_at(&bytes, 22), 1, "mono");
            assert_eq!(u32_at(&bytes, 24), SAMPLE_RATE);
            assert_eq!(u32_at(&bytes, 28), SAMPLE_RATE * 2, "byte rate");
            assert_eq!(u16_at(&bytes, 32), 2, "block align");
            assert_eq!(u16_at(&bytes, 34), 16, "bits per sample");
            assert_eq!(&bytes[36..40], b"data");
        }
    }

    #[test]
    fn wav_data_chunk_holds_the_samples_little_endian() {
        let bytes = encode_wav(&[0, 1, -1, i16::MAX, i16::MIN], 8000);
        assert_eq!(u32_at(&bytes, 24), 8000);
        assert_eq!(u32_at(&bytes, 40), 10);
        assert_eq!(bytes.len(), 44 + 10);
        assert_eq!(&bytes[44..], &[0, 0, 1, 0, 0xff, 0xff, 0xff, 0x7f, 0x00, 0x80]);
        for cue in Cue::ALL {
            let bytes = wav(cue);
            assert_eq!(u32_at(&bytes, 40) as usize, samples(cue).len() * 2);
        }
    }

    #[test]
    fn needs_you_lasts_700_to_1000_ms_including_the_ring() {
        let ms = duration_ms(Cue::NeedsYou);
        assert!((700..=1000).contains(&ms), "{ms} ms");
    }

    #[test]
    fn done_lasts_500_to_800_ms_including_the_ring() {
        let ms = duration_ms(Cue::Done);
        assert!((500..=800).contains(&ms), "{ms} ms");
    }

    #[test]
    fn peak_level_is_soft_between_minus_12_and_minus_10_dbfs() {
        for cue in Cue::ALL {
            let db = peak_dbfs(cue);
            assert!((-12.0..=-10.0).contains(&db), "{cue:?}: {db:.2} dBFS");
        }
    }

    #[test]
    fn strongest_component_is_a_fundamental_between_400_and_1100_hz() {
        for cue in Cue::ALL {
            let s = samples(cue);
            let (freq, _) = strongest(&s, 100, 11_000);
            assert!((400.0..=1100.0).contains(&freq), "{cue:?}: strongest at {freq} Hz");
        }
    }

    #[test]
    fn nothing_above_3_khz_comes_within_30_db_of_the_fundamental() {
        for cue in Cue::ALL {
            let s = samples(cue);
            let (_, fundamental) = strongest(&s, 400, 1100);
            let (freq, high) = strongest(&s, 3_000, 11_000);
            let db = 20.0 * (high / fundamental).log10();
            assert!(db < -30.0, "{cue:?}: {freq} Hz at {db:.1} dB relative to the fundamental");
        }
    }

    #[test]
    fn cues_fade_in_and_out_without_clicks() {
        let quiet = (32767.0 * 0.02) as i32;
        for cue in Cue::ALL {
            let s = samples(cue);
            let peak = s.iter().map(|v| (*v as i32).abs()).max().unwrap();
            // Endpoints are silent and the first and last half millisecond stay well
            // below the peak, so the buffer neither starts nor stops on a step.
            assert!((s[0] as i32).abs() <= quiet && (s[s.len() - 1] as i32).abs() <= quiet, "{cue:?}");
            let edge = SAMPLE_RATE as usize / 2000;
            let head = s[..edge].iter().map(|v| (*v as i32).abs()).max().unwrap();
            let tail = s[s.len() - edge..].iter().map(|v| (*v as i32).abs()).max().unwrap();
            assert!(head < peak / 4 && tail < peak / 4, "{cue:?}: head {head}, tail {tail}, peak {peak}");
        }
    }

    #[test]
    fn the_two_cues_are_distinct() {
        assert_ne!(samples(Cue::NeedsYou), samples(Cue::Done));
        assert_ne!(wav(Cue::NeedsYou), wav(Cue::Done));
    }

    #[test]
    fn synthesis_is_deterministic() {
        for cue in Cue::ALL {
            assert_eq!(samples(cue), samples(cue));
        }
    }

    #[test]
    fn cue_follows_the_notice_kind_and_the_most_urgent_wins() {
        assert_eq!(Cue::for_kind(NoticeKind::NeedsYou), Cue::NeedsYou);
        assert_eq!(Cue::for_kind(NoticeKind::Done), Cue::Done);
        assert_eq!(Cue::for_notices(&[]), None);
        assert_eq!(Cue::for_notices(&[notice(NoticeKind::Done)]), Some(Cue::Done));
        assert_eq!(
            Cue::for_notices(&[notice(NoticeKind::Done), notice(NoticeKind::NeedsYou), notice(NoticeKind::Done)]),
            Some(Cue::NeedsYou)
        );
    }

    #[test]
    fn sound_gate_drops_cues_inside_the_gap() {
        let mut gate = SoundGate::new(1000);
        assert!(gate.allow(5_000));
        assert!(!gate.allow(5_500));
        assert!(!gate.allow(5_999));
        assert!(gate.allow(6_000));
        assert!(!gate.allow(6_100));
    }
}
