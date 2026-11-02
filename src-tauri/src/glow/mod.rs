//! The recording indicator: a coloured glow around the edges of the active
//! monitor, so anyone can see from across the room that Kusema is listening.
//!
//! It follows `AudioRecordingManager`'s state, the one place every recording
//! starts and stops, so it can't disagree with the microphone. The drawing is in
//! `windows_impl`: four thin native layered windows, because a webview over the
//! whole screen would cost memory and GPU all the time. Windows only for now.

use crate::settings::{self, AppSettings};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[cfg(target_os = "windows")]
mod windows_impl;

pub const MIN_THICKNESS: u32 = 8;
pub const MAX_THICKNESS: u32 = 48;
pub const MIN_INTENSITY: u32 = 30;
pub const MAX_INTENSITY: u32 = 100;
const PREVIEW_TIME: Duration = Duration::from_secs(3);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Type)]
#[serde(rename_all = "snake_case")]
pub enum GlowPalette {
    #[default]
    Standard,
    ColorBlind,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum GlowMode {
    Dictation,
    Continuous,
    Ask,
    AskScreen,
    /// Continuous dictation, with its window no longer in front.
    Paused,
}

const ALL_MODES: [GlowMode; 5] = [
    GlowMode::Dictation,
    GlowMode::Continuous,
    GlowMode::Ask,
    GlowMode::AskScreen,
    GlowMode::Paused,
];

impl GlowMode {
    fn for_binding(binding_id: &str) -> Self {
        match binding_id {
            crate::dictation::BINDING => GlowMode::Continuous,
            "assistant" => GlowMode::Ask,
            "assistant_screen" => GlowMode::AskScreen,
            _ => GlowMode::Dictation,
        }
    }

    /// Paused reads differently without relying on colour.
    fn dashed(self) -> bool {
        self == GlowMode::Paused
    }

    /// Red, green, blue. The second palette is Okabe-Ito's, chosen to stay apart
    /// with every common kind of colour blindness.
    fn rgb(self, palette: GlowPalette) -> (u8, u8, u8) {
        match (palette, self) {
            (GlowPalette::Standard, GlowMode::Dictation) => (0x22, 0xC5, 0x5E),
            (GlowPalette::Standard, GlowMode::Continuous) => (0xEF, 0x44, 0x44),
            (GlowPalette::Standard, GlowMode::Ask) => (0x3B, 0x82, 0xF6),
            (GlowPalette::Standard, GlowMode::AskScreen) => (0xD9, 0x46, 0xEF),
            (GlowPalette::Standard, GlowMode::Paused) => (0xF5, 0x9E, 0x0B),
            (GlowPalette::ColorBlind, GlowMode::Dictation) => (0x00, 0x9E, 0x73),
            (GlowPalette::ColorBlind, GlowMode::Continuous) => (0xD5, 0x5E, 0x00),
            (GlowPalette::ColorBlind, GlowMode::Ask) => (0x56, 0xB4, 0xE9),
            (GlowPalette::ColorBlind, GlowMode::AskScreen) => (0xCC, 0x79, 0xA7),
            (GlowPalette::ColorBlind, GlowMode::Paused) => (0xF0, 0xE4, 0x42),
        }
    }
}

/// What the glow should look like right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style {
    pub rgb: (u8, u8, u8),
    pub dashed: bool,
    /// Logical pixels.
    pub thickness: u32,
    /// 0 to 255.
    pub alpha: u8,
    pub pulses: bool,
}

#[derive(Clone, Copy, Debug)]
struct Look {
    enabled: bool,
    thickness: u32,
    intensity: u32,
    palette: GlowPalette,
}

impl Look {
    fn from_settings(settings: &AppSettings) -> Self {
        Self {
            enabled: settings.glow_enabled,
            thickness: settings.glow_thickness,
            intensity: settings.glow_intensity,
            palette: settings.glow_palette,
        }
    }
}

#[derive(Debug)]
struct Desired {
    recording: Option<GlowMode>,
    preview: Option<(GlowMode, Instant)>,
    look: Look,
}

impl Desired {
    /// A real recording always wins over a preview.
    fn style(&self, now: Instant) -> Option<Style> {
        let recording = self.recording.filter(|_| self.look.enabled);
        let preview = self
            .preview
            .filter(|(_, until)| now < *until)
            .map(|(mode, _)| mode);
        let mode = recording.or(preview)?;
        Some(Style {
            rgb: mode.rgb(self.look.palette),
            dashed: mode.dashed(),
            thickness: self.look.thickness.clamp(MIN_THICKNESS, MAX_THICKNESS),
            alpha: alpha_for(self.look.intensity),
            pulses: !mode.dashed(),
        })
    }
}

fn alpha_for(intensity: u32) -> u8 {
    (intensity.clamp(MIN_INTENSITY, MAX_INTENSITY) * 255 / 100) as u8
}

static DESIRED: Mutex<Option<Desired>> = Mutex::new(None);

fn update(change: impl FnOnce(&mut Desired), look: Look) {
    {
        let mut desired = DESIRED.lock().unwrap_or_else(|e| e.into_inner());
        let desired = desired.get_or_insert(Desired {
            recording: None,
            preview: None,
            look,
        });
        desired.look = look;
        change(desired);
    }
    #[cfg(target_os = "windows")]
    windows_impl::wake();
}

/// The style to draw now, for the drawing thread.
#[cfg(target_os = "windows")]
fn current_style() -> Option<Style> {
    DESIRED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|desired| desired.style(Instant::now()))
}

/// A recording began for this shortcut.
pub fn recording_started(app: &AppHandle, binding_id: &str) {
    let look = Look::from_settings(&settings::get_settings(app));
    let mode = GlowMode::for_binding(binding_id);
    update(|desired| desired.recording = Some(mode), look);
}

/// The microphone stopped listening.
pub fn recording_stopped(app: &AppHandle) {
    let look = Look::from_settings(&settings::get_settings(app));
    update(|desired| desired.recording = None, look);
}

/// Continuous dictation's window left the front, or came back.
pub fn set_paused(app: &AppHandle, paused: bool) {
    let look = Look::from_settings(&settings::get_settings(app));
    update(
        |desired| {
            if matches!(
                desired.recording,
                Some(GlowMode::Continuous | GlowMode::Paused)
            ) {
                desired.recording = Some(if paused {
                    GlowMode::Paused
                } else {
                    GlowMode::Continuous
                });
            }
        },
        look,
    );
}

fn settings_changed(app: &AppHandle, name: &str, new_settings: AppSettings) {
    let look = Look::from_settings(&new_settings);
    settings::write_settings(app, new_settings);
    let _ = app.emit("settings-changed", serde_json::json!({ "setting": name }));
    update(|_| {}, look);
}

#[tauri::command]
#[specta::specta]
pub fn change_glow_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut new_settings = settings::get_settings(&app);
    new_settings.glow_enabled = enabled;
    settings_changed(&app, "glow_enabled", new_settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_glow_thickness_setting(app: AppHandle, thickness: u32) -> Result<(), String> {
    let mut new_settings = settings::get_settings(&app);
    new_settings.glow_thickness = thickness.clamp(MIN_THICKNESS, MAX_THICKNESS);
    settings_changed(&app, "glow_thickness", new_settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_glow_intensity_setting(app: AppHandle, intensity: u32) -> Result<(), String> {
    let mut new_settings = settings::get_settings(&app);
    new_settings.glow_intensity = intensity.clamp(MIN_INTENSITY, MAX_INTENSITY);
    settings_changed(&app, "glow_intensity", new_settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_glow_palette_setting(app: AppHandle, palette: GlowPalette) -> Result<(), String> {
    let mut new_settings = settings::get_settings(&app);
    new_settings.glow_palette = palette;
    settings_changed(&app, "glow_palette", new_settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_dictation_sounds_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut new_settings = settings::get_settings(&app);
    new_settings.dictation_sounds = enabled;
    settings::write_settings(&app, new_settings);
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "dictation_sounds" }),
    );
    Ok(())
}

/// One mode's colour, for the key on the settings page.
#[derive(Serialize, Clone, Debug, PartialEq, Eq, Type)]
pub struct GlowColor {
    pub mode: GlowMode,
    /// `#rrggbb`
    pub color: String,
    pub dashed: bool,
}

fn colors_for(palette: GlowPalette) -> Vec<GlowColor> {
    ALL_MODES
        .iter()
        .map(|&mode| {
            let (r, g, b) = mode.rgb(palette);
            GlowColor {
                mode,
                color: format!("#{r:02x}{g:02x}{b:02x}"),
                dashed: mode.dashed(),
            }
        })
        .collect()
}

#[tauri::command]
#[specta::specta]
pub fn get_glow_colors(app: AppHandle) -> Vec<GlowColor> {
    colors_for(settings::get_settings(&app).glow_palette)
}

/// Shows a mode's glow for a few seconds, so it can be seen without recording.
#[tauri::command]
#[specta::specta]
pub fn preview_recording_glow(app: AppHandle, mode: GlowMode) {
    let look = Look::from_settings(&settings::get_settings(&app));
    let until = Instant::now() + PREVIEW_TIME;
    update(|desired| desired.preview = Some((mode, until)), look);
}

/// Where the four strips go, for a monitor at `(left, top)` of this size, as
/// (x, y, width, height) in physical pixels. Top and bottom own the corners.
pub fn strips(left: i32, top: i32, width: i32, height: i32, thickness: i32) -> [[i32; 4]; 4] {
    let side_height = (height - 2 * thickness).max(0);
    [
        [left, top, width, thickness],
        [left, top + height - thickness, width, thickness],
        [left, top + thickness, thickness, side_height],
        [
            left + width - thickness,
            top + thickness,
            thickness,
            side_height,
        ],
    ]
}

/// How strong the glow is (0 to 255) at this pixel of a `width` by `height`
/// monitor: solid at the very edge, then fading inwards, following the corners.
pub fn pixel_alpha(x: i32, y: i32, width: i32, height: i32, thickness: i32, dash: i32) -> u8 {
    let to_side = x.min(width - 1 - x);
    let to_end = y.min(height - 1 - y);
    let distance = to_side.min(to_end);
    if distance >= thickness || thickness <= 0 {
        return 0;
    }
    if dash > 0 {
        // Along whichever edge this pixel belongs to.
        let along = if to_end <= to_side { x } else { y };
        if (along / dash) % 2 == 1 {
            return 0;
        }
    }
    let solid = thickness as f32 * 0.3;
    let fade = ((distance as f32 - solid) / (thickness as f32 - solid)).clamp(0.0, 1.0);
    ((1.0 - fade) * (1.0 - fade) * 255.0) as u8
}

/// How strong the pulse leaves the glow, `seconds` in: never below 70%, so it's
/// always plain that it's on.
pub fn pulse_alpha(alpha: u8, seconds: f32) -> u8 {
    const PERIOD: f32 = 2.4;
    let wave = (seconds / PERIOD * std::f32::consts::TAU).cos() * 0.5 + 0.5;
    (alpha as f32 * (0.7 + 0.3 * wave)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look() -> Look {
        Look {
            enabled: true,
            thickness: 24,
            intensity: 80,
            palette: GlowPalette::Standard,
        }
    }

    fn desired(recording: Option<GlowMode>) -> Desired {
        Desired {
            recording,
            preview: None,
            look: look(),
        }
    }

    #[test]
    fn each_shortcut_has_its_own_mode() {
        assert_eq!(GlowMode::for_binding("transcribe"), GlowMode::Dictation);
        assert_eq!(
            GlowMode::for_binding("transcribe_with_post_process"),
            GlowMode::Dictation
        );
        assert_eq!(
            GlowMode::for_binding(crate::dictation::BINDING),
            GlowMode::Continuous
        );
        assert_eq!(GlowMode::for_binding("assistant"), GlowMode::Ask);
        assert_eq!(
            GlowMode::for_binding("assistant_screen"),
            GlowMode::AskScreen
        );
    }

    #[test]
    fn every_mode_has_a_different_colour_in_both_palettes() {
        for palette in [GlowPalette::Standard, GlowPalette::ColorBlind] {
            let colors: Vec<_> = ALL_MODES.iter().map(|m| m.rgb(palette)).collect();
            for (i, a) in colors.iter().enumerate() {
                for b in &colors[i + 1..] {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn the_palettes_differ() {
        assert_ne!(
            colors_for(GlowPalette::Standard),
            colors_for(GlowPalette::ColorBlind)
        );
        assert_eq!(colors_for(GlowPalette::Standard)[0].color, "#22c55e");
    }

    #[test]
    fn only_paused_is_dashed_and_it_never_pulses() {
        for mode in ALL_MODES {
            let style = desired(Some(mode)).style(Instant::now()).unwrap();
            assert_eq!(style.dashed, mode == GlowMode::Paused);
            assert_eq!(style.pulses, mode != GlowMode::Paused);
        }
    }

    #[test]
    fn nothing_shows_when_not_recording_or_when_turned_off() {
        assert_eq!(desired(None).style(Instant::now()), None);
        let mut off = desired(Some(GlowMode::Dictation));
        off.look.enabled = false;
        assert_eq!(off.style(Instant::now()), None);
    }

    #[test]
    fn a_preview_ends_by_itself_and_a_recording_wins_over_it() {
        let now = Instant::now();
        let mut state = desired(None);
        state.preview = Some((GlowMode::Ask, now + PREVIEW_TIME));
        let colour = |m: GlowMode| Some(m.rgb(GlowPalette::Standard));
        assert_eq!(state.style(now).map(|s| s.rgb), colour(GlowMode::Ask));
        assert_eq!(state.style(now + PREVIEW_TIME), None);
        state.recording = Some(GlowMode::Dictation);
        assert_eq!(state.style(now).map(|s| s.rgb), colour(GlowMode::Dictation));
    }

    #[test]
    fn a_preview_shows_even_with_the_glow_off() {
        let now = Instant::now();
        let mut state = desired(None);
        state.look.enabled = false;
        state.preview = Some((GlowMode::Ask, now + PREVIEW_TIME));
        assert!(state.style(now).is_some());
    }

    #[test]
    fn out_of_range_values_are_held_to_the_limits() {
        let mut state = desired(Some(GlowMode::Dictation));
        state.look.thickness = 500;
        state.look.intensity = 1;
        let style = state.style(Instant::now()).unwrap();
        assert_eq!(style.thickness, MAX_THICKNESS);
        assert_eq!(style.alpha, alpha_for(MIN_INTENSITY));
        assert_eq!(alpha_for(100), 255);
    }

    #[test]
    fn strips_cover_the_edges_without_overlapping() {
        let [top, bottom, left, right] = strips(0, 0, 1920, 1080, 24);
        assert_eq!(top, [0, 0, 1920, 24]);
        assert_eq!(bottom, [0, 1056, 1920, 24]);
        assert_eq!(left, [0, 24, 24, 1032]);
        assert_eq!(right, [1896, 24, 24, 1032]);
    }

    #[test]
    fn strips_follow_a_monitor_anywhere_in_the_desktop() {
        // A second monitor to the left and above, at 200% scaling.
        let [top, bottom, left, right] = strips(-3840, -400, 3840, 2160, 48);
        assert_eq!(top, [-3840, -400, 3840, 48]);
        assert_eq!(bottom, [-3840, 1712, 3840, 48]);
        assert_eq!(left, [-3840, -352, 48, 2064]);
        assert_eq!(right, [-48, -352, 48, 2064]);
    }

    #[test]
    fn strips_never_have_a_negative_size() {
        assert_eq!(strips(0, 0, 100, 40, 24)[2][3], 0);
    }

    #[test]
    fn the_glow_is_solid_at_the_edge_and_gone_past_its_thickness() {
        assert_eq!(pixel_alpha(500, 0, 1000, 1000, 24, 0), 255);
        assert_eq!(pixel_alpha(0, 500, 1000, 1000, 24, 0), 255);
        assert_eq!(pixel_alpha(999, 500, 1000, 1000, 24, 0), 255);
        assert_eq!(pixel_alpha(500, 999, 1000, 1000, 24, 0), 255);
        assert_eq!(pixel_alpha(500, 24, 1000, 1000, 24, 0), 0);
        assert_eq!(pixel_alpha(500, 500, 1000, 1000, 24, 0), 0);
        let middle = pixel_alpha(500, 14, 1000, 1000, 24, 0);
        assert!(0 < middle && middle < 255, "{middle}");
    }

    #[test]
    fn the_glow_meets_itself_at_the_corners() {
        // Where the top strip hands over to the side strip.
        let top = pixel_alpha(10, 23, 1000, 1000, 24, 0);
        let side = pixel_alpha(10, 24, 1000, 1000, 24, 0);
        assert_eq!(top, side);
        assert!(top > 0);
    }

    #[test]
    fn dashes_leave_gaps_along_every_edge() {
        let dash = 64;
        assert_eq!(pixel_alpha(10, 0, 1000, 1000, 24, dash), 255);
        assert_eq!(pixel_alpha(70, 0, 1000, 1000, 24, dash), 0);
        assert_eq!(pixel_alpha(70, 999, 1000, 1000, 24, dash), 0);
        assert_eq!(pixel_alpha(0, 10 + 128, 1000, 1000, 24, dash), 255);
        assert_eq!(pixel_alpha(0, 70, 1000, 1000, 24, dash), 0);
    }

    #[test]
    fn the_pulse_never_goes_dark() {
        for tenth in 0..100 {
            let alpha = pulse_alpha(200, tenth as f32 / 10.0);
            assert!((139..=200).contains(&alpha), "{alpha}");
        }
        assert_eq!(pulse_alpha(200, 0.0), 200);
    }
}
