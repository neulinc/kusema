//! What a microphone check measured, from its two parts: a few seconds of
//! quiet, then a sentence read aloud. Pure, so every rule is tested.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Levels are judged over windows this long.
const WINDOW_MS: usize = 20;
/// The start of the quiet part is skipped: the click of starting, a breath.
const SETTLE_MS: usize = 300;
/// The room is measured at this percentile of its windows.
const NOISE_PERCENTILE: f32 = 0.8;
/// Quieter than this is "no signal" (dBFS).
const FLOOR_DB: f32 = -100.0;
/// A sample this close to full scale is clipped.
const CLIP_LEVEL: f32 = 0.99;
/// More clipped samples than this (percent of the speech) distorts.
const CLIPPING_PERCENT: f32 = 0.05;
/// Room noise louder than this is "a bit noisy" (dBFS); a quiet room with
/// a laptop microphone measures about -60 to -50.
const NOISY_DB: f32 = -48.0;
/// Speech must stand this far above the room to be heard well (dB).
const GOOD_SNR_DB: f32 = 20.0;
/// Under this, nobody was heard reading.
const HEARD_SNR_DB: f32 = 8.0;
/// The loud parts of speech quieter than this are too quiet (dBFS).
const QUIET_SPEECH_DB: f32 = -36.0;
/// The VAD gate sits this far over the room noise...
const GATE_OVER_NOISE_DB: f32 = 8.0;
/// ...but at least this far under the loud parts of speech, so soft words pass.
const GATE_UNDER_SPEECH_DB: f32 = 15.0;
/// A gate less than this over the noise wouldn't change anything.
const GATE_MIN_OVER_NOISE_DB: f32 = 3.0;

/// What a microphone measured, kept per microphone for the VAD gate.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct MicProfile {
    /// The room with nobody speaking (dBFS).
    pub noise_db: f32,
    /// The loud parts of speech (dBFS).
    pub speech_db: f32,
}

/// One thing to tell the user, most important first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MicTip {
    /// Nobody was heard reading: the wrong microphone, or muted.
    NotHeard,
    /// Loud enough to distort.
    Clipping,
    /// Too quiet.
    Quiet,
    /// The room's noise is close to the speech.
    Noisy,
    /// A Bluetooth headset's mic in hands-free mode.
    BluetoothHandsFree,
    /// Nothing to change.
    Good,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct MicReport {
    pub noise_db: f32,
    pub speech_db: f32,
    pub snr_db: f32,
    pub clipped_percent: f32,
    pub bluetooth_hands_free: bool,
    pub tips: Vec<MicTip>,
}

/// The level of each window, in dBFS.
pub fn window_levels(samples: &[f32], rate: u32) -> Vec<f32> {
    let window = (rate as usize * WINDOW_MS / 1000).max(1);
    samples.chunks_exact(window).map(|w| level_db(w)).collect()
}

/// The RMS level of `samples`, in dBFS.
pub fn level_db(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return FLOOR_DB;
    }
    let power = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    if power <= 0.0 {
        return FLOOR_DB;
    }
    (10.0 * power.log10()).max(FLOOR_DB)
}

/// The `p`th percentile (0 to 1) of `values`.
fn percentile(values: &[f32], p: f32) -> f32 {
    if values.is_empty() {
        return FLOOR_DB;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let index = ((sorted.len() - 1) as f32 * p).round() as usize;
    sorted[index]
}

/// Whether a sample is at full scale.
pub fn is_clipped(sample: f32) -> bool {
    sample.abs() >= CLIP_LEVEL
}

/// The share of samples at full scale, in percent.
#[cfg(test)]
fn clipped_percent(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let clipped = samples.iter().filter(|s| is_clipped(**s)).count();
    100.0 * clipped as f32 / samples.len() as f32
}

/// Whether Windows names this a Bluetooth headset in hands-free mode, whose
/// microphone sends narrowband audio (8 or 16 kHz, heavily compressed).
pub fn is_bluetooth_hands_free(device_name: &str, device_rate: Option<u32>) -> bool {
    let name = device_name.to_lowercase();
    if name.contains("hands-free") || name.contains("hands free") || name.contains("handsfree") {
        return true;
    }
    let low_rate = device_rate.is_some_and(|rate| rate <= 16_000);
    low_rate && (name.contains("bluetooth") || name.contains("headset"))
}

/// Measures a check: `quiet` and `speech` are the two parts, mono, at `rate`.
/// `clipped` is the share of the speech part at full scale, counted on the
/// samples as they came, before channels were mixed.
pub fn analyze(
    quiet: &[f32],
    speech: &[f32],
    clipped: f32,
    rate: u32,
    device_name: &str,
    device_rate: Option<u32>,
) -> MicReport {
    let settle = (rate as usize * SETTLE_MS / 1000).min(quiet.len());
    // The room's louder moments: voices nearby or a television come and go,
    // and a steady fan reads the same either way.
    let noise_db = percentile(&window_levels(&quiet[settle..], rate), NOISE_PERCENTILE);
    let speech_db = percentile(&window_levels(speech, rate), 0.9);
    let snr_db = speech_db - noise_db;
    let bluetooth = is_bluetooth_hands_free(device_name, device_rate);

    let mut tips = Vec::new();
    if snr_db < HEARD_SNR_DB {
        tips.push(MicTip::NotHeard);
    } else {
        if clipped >= CLIPPING_PERCENT {
            tips.push(MicTip::Clipping);
        } else if speech_db < QUIET_SPEECH_DB {
            tips.push(MicTip::Quiet);
        }
        if noise_db > NOISY_DB || snr_db < GOOD_SNR_DB {
            tips.push(MicTip::Noisy);
        }
    }
    if bluetooth {
        tips.push(MicTip::BluetoothHandsFree);
    }
    if tips.is_empty() {
        tips.push(MicTip::Good);
    }
    MicReport {
        noise_db: round1(noise_db),
        speech_db: round1(speech_db),
        snr_db: round1(snr_db),
        clipped_percent: (clipped * 1000.0).round() / 1000.0,
        bluetooth_hands_free: bluetooth,
        tips,
    }
}

fn round1(value: f32) -> f32 {
    (value * 10.0).round() / 10.0
}

impl MicReport {
    /// What to keep for the VAD gate, when someone was heard.
    pub fn profile(&self) -> Option<MicProfile> {
        (!self.tips.contains(&MicTip::NotHeard)).then_some(MicProfile {
            noise_db: self.noise_db,
            speech_db: self.speech_db,
        })
    }
}

/// The level under which the voice detector ignores sound, for a measured
/// microphone: over the room's noise, under soft speech. `None` when the two
/// are too close for a gate to help without losing words.
pub fn gate_db(profile: &MicProfile) -> Option<f32> {
    let gate =
        (profile.noise_db + GATE_OVER_NOISE_DB).min(profile.speech_db - GATE_UNDER_SPEECH_DB);
    (gate >= profile.noise_db + GATE_MIN_OVER_NOISE_DB).then_some(gate)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 16_000;

    /// A sine at `db` dBFS RMS, `ms` long.
    fn tone(db: f32, ms: usize) -> Vec<f32> {
        let amplitude = 10f32.powf(db / 20.0) * std::f32::consts::SQRT_2;
        (0..RATE as usize * ms / 1000)
            .map(|i| amplitude * (i as f32 * 0.07).sin())
            .collect()
    }

    /// Speech-like: loud bursts with softer gaps.
    fn speech(loud_db: f32, ms: usize) -> Vec<f32> {
        let mut out = Vec::new();
        while out.len() < RATE as usize * ms / 1000 {
            out.extend(tone(loud_db, 200));
            out.extend(tone(loud_db - 20.0, 100));
        }
        out
    }

    fn report(quiet: &[f32], spoken: &[f32]) -> MicReport {
        analyze(
            quiet,
            spoken,
            clipped_percent(spoken),
            RATE,
            "Microphone Array",
            Some(48_000),
        )
    }

    #[test]
    fn levels_are_measured_in_dbfs() {
        assert!((level_db(&tone(-20.0, 100)) + 20.0).abs() < 0.2);
        assert!((level_db(&tone(-55.0, 100)) + 55.0).abs() < 0.2);
        assert_eq!(level_db(&[0.0; 320]), FLOOR_DB);
        assert_eq!(level_db(&[]), FLOOR_DB);
    }

    #[test]
    fn a_quiet_room_and_clear_speech_sound_good() {
        let r = report(&tone(-60.0, 3000), &speech(-22.0, 3000));
        assert!((r.noise_db + 60.0).abs() < 0.5, "{r:?}");
        assert!((r.speech_db + 22.0).abs() < 0.5, "{r:?}");
        assert!((r.snr_db - 38.0).abs() < 1.0, "{r:?}");
        assert_eq!(r.tips, vec![MicTip::Good]);
    }

    #[test]
    fn the_start_of_the_quiet_part_is_skipped() {
        // A click at the start doesn't count as the room.
        let mut quiet = tone(-10.0, 250);
        quiet.extend(tone(-60.0, 2750));
        let r = report(&quiet, &speech(-22.0, 3000));
        assert!((r.noise_db + 60.0).abs() < 0.5, "{r:?}");
    }

    #[test]
    fn voices_that_come_and_go_count_as_the_room() {
        // A fan, and someone talking nearby half the time.
        let mut quiet = Vec::new();
        for _ in 0..6 {
            quiet.extend(tone(-58.0, 250));
            quiet.extend(tone(-40.0, 250));
        }
        let r = report(&quiet, &speech(-20.0, 3000));
        assert!((r.noise_db + 40.0).abs() < 0.5, "{r:?}");
        assert!(r.tips.contains(&MicTip::Noisy), "{r:?}");
    }

    #[test]
    fn a_noisy_room_suggests_a_headset() {
        let r = report(&tone(-42.0, 3000), &speech(-22.0, 3000));
        assert_eq!(r.tips, vec![MicTip::Noisy]);
        // Quieter noise but speech only a little louder is noisy too.
        let r = report(&tone(-55.0, 3000), &speech(-38.0, 3000));
        assert!(r.tips.contains(&MicTip::Noisy), "{r:?}");
    }

    #[test]
    fn quiet_speech_says_move_closer() {
        let r = report(&tone(-70.0, 3000), &speech(-40.0, 3000));
        assert_eq!(r.tips, vec![MicTip::Quiet]);
    }

    #[test]
    fn clipping_is_found_and_outranks_quiet() {
        let mut loud = speech(-6.0, 3000);
        for s in loud.iter_mut().step_by(50) {
            *s = 1.0;
        }
        assert!(clipped_percent(&loud) > 1.0);
        let r = report(&tone(-60.0, 3000), &loud);
        assert_eq!(r.tips, vec![MicTip::Clipping]);
        // A few full-scale samples in a long read aren't distortion.
        let mut fine = speech(-20.0, 3000);
        fine[100] = 1.0;
        assert!(clipped_percent(&fine) < CLIPPING_PERCENT);
    }

    #[test]
    fn nobody_heard_is_said_plainly_and_keeps_no_profile() {
        let r = report(&tone(-50.0, 3000), &tone(-49.0, 3000));
        assert_eq!(r.tips, vec![MicTip::NotHeard]);
        assert_eq!(r.profile(), None);
        assert!(report(&tone(-60.0, 3000), &speech(-22.0, 3000))
            .profile()
            .is_some());
    }

    #[test]
    fn bluetooth_hands_free_is_recognized_by_name_or_rate() {
        assert!(is_bluetooth_hands_free(
            "Headset (WH-1000XM4 Hands-Free AG Audio)",
            Some(16_000)
        ));
        assert!(is_bluetooth_hands_free("Headset (Jabra Hands-Free)", None));
        assert!(is_bluetooth_hands_free(
            "Headset Microphone (Bluetooth)",
            Some(8_000)
        ));
        assert!(!is_bluetooth_hands_free(
            "Headset Microphone (Jabra Evolve 40)",
            Some(48_000)
        ));
        assert!(!is_bluetooth_hands_free(
            "Microphone Array (Realtek)",
            Some(16_000)
        ));
        let r = analyze(
            &tone(-60.0, 3000),
            &speech(-22.0, 3000),
            0.0,
            RATE,
            "Headset (AirPods Hands-Free)",
            Some(16_000),
        );
        assert_eq!(r.tips, vec![MicTip::BluetoothHandsFree]);
    }

    #[test]
    fn the_gate_sits_between_room_and_soft_speech() {
        // A quiet room: 8 dB over it.
        let quiet = MicProfile {
            noise_db: -60.0,
            speech_db: -22.0,
        };
        assert_eq!(gate_db(&quiet), Some(-52.0));
        // A noisy room with a close microphone: still 8 over.
        let noisy = MicProfile {
            noise_db: -45.0,
            speech_db: -18.0,
        };
        assert_eq!(gate_db(&noisy), Some(-37.0));
        // Speech close to the noise: under soft speech, if that's still worth it.
        let close = MicProfile {
            noise_db: -45.0,
            speech_db: -26.0,
        };
        assert_eq!(gate_db(&close), Some(-41.0));
        // Too close for a gate to help without cutting words.
        let too_close = MicProfile {
            noise_db: -45.0,
            speech_db: -30.0,
        };
        assert_eq!(gate_db(&too_close), None);
    }
}
