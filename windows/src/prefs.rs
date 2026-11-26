use std::ffi::c_void;
use std::time::{SystemTime, UNIX_EPOCH};

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_DWORD, REG_SZ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::Win32::UI::WindowsAndMessaging::{SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW};
use windows::core::{BOOL, PCWSTR, w};

use snowhush_core::christmas::HolidaySettings;
use snowhush_core::halloween::Choice;
use snowhush_core::simulation::Mode;

const KEY: PCWSTR = w!("Software\\ChristmasTreeUltimate");
const MODES: [Mode; 4] = [Mode::Snow, Mode::Autumn, Mode::Rain, Mode::Blossom];
const RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = w!("ChristmasTreeUltimate");
const LEGACY_RUN_VALUE: PCWSTR = w!("GovnoWithSmoke");

#[derive(Clone, Copy)]
pub struct Settings {
    pub holiday: HolidaySettings,
    /// How much falls, in percent: 0 to 100 in steps of 5.
    pub intensity: u32,
    /// Snow and leaves settle along the bottom of the screen.
    pub ground: bool,
    pub melt: bool,
    pub snow_on_windows: bool,
    pub behind_windows: bool,
    /// The snow stays over full-screen apps instead of fading out of their way.
    pub over_full_screen: bool,
    /// The snow settles at the bottom of the screen, under the taskbar, for a taskbar made see-through, as by TranslucentTB.
    pub behind_taskbar: bool,
    pub show_in_screenshots: bool,
    /// The snow also settles on what the desktop draws without a window of its own: a selection dragged across it.
    pub deep_interaction: bool,
    pub welcomed: bool,
    /// Unix seconds of the first run, or 0 for an install older than the stamp.
    pub installed_at: u32,
    pub halloween: Choice,
    /// What falls: snow, leaves or rain; the kind picked last.
    pub mode: Mode,
    /// Two kinds fall together: `mix`, the one picked last after the one picked before it.
    pub mixed: bool,
    pub mix: [Mode; 2],
    /// Drawn at the display's highest rate all the time, instead of only while something is pushed around.
    pub max_performance: bool,
    /// How opaque what falls is drawn, in percent: 10 to 100 in steps of 5.
    pub opacity: u32,
}

impl Settings {
    /// Whether snow settles anywhere.
    pub fn accumulates(&self) -> bool {
        self.ground || self.snow_on_windows
    }

    pub fn load() -> Settings {
        let calm = !animations_enabled();
        // Before the slider, the intensity was one of Off, Light, Steady, Heavy and Variable.
        let stop = read(w!("snowfall")).map(|i| i.min(4) as usize);
        let welcomed = read(w!("welcomed")).is_some_and(|v| v != 0);
        let installed_at = read(w!("installedAt")).unwrap_or_else(|| {
            let stamp = if welcomed { 0 } else { now() };
            write(w!("installedAt"), stamp);
            stamp
        });
        let mode = read(w!("mode")).and_then(|i| MODES.get(i as usize).copied()).unwrap_or(if read(w!("autumn")).is_some_and(|v| v != 0) {
            Mode::Autumn
        } else {
            Mode::Snow
        });
        // Before Accumulate picked where snow settles, it turned settling off everywhere at once.
        let settled = read(w!("accumulate")).is_none_or(|v| v != 0) || read(w!("settleGround")).is_some();
        Settings {
            holiday: load_holiday(),
            intensity: read(w!("intensity")).map(|v| v.clamp(0, 100)).or(stop.map(|i| [0, 25, 50, 75, 50][i])).unwrap_or(if calm { 25 } else { 50 }),
            ground: read(w!("settleGround")).map_or(settled, |v| v != 0),
            melt: read(w!("meltSnow")).is_none_or(|v| v != 0),
            snow_on_windows: settled && read(w!("snowOnWindows")).is_none_or(|v| v != 0),
            behind_windows: read(w!("behindWindows")).is_some_and(|v| v != 0),
            over_full_screen: read(w!("showOverFullScreen")).is_some_and(|v| v != 0),
            behind_taskbar: read(w!("behindTaskbar")).is_some_and(|v| v != 0),
            show_in_screenshots: read(w!("showInScreenshots")).is_some_and(|v| v != 0),
            deep_interaction: read(w!("deepInteraction")).is_some_and(|v| v != 0),
            welcomed,
            installed_at,
            halloween: Choice { on: read(w!("halloweenOn")).is_some_and(|v| v != 0), skipped: read(w!("halloweenSkipped")).unwrap_or(0) },
            // Before rain there was only a switch for leaves.
            max_performance: read(w!("performance")).is_some_and(|v| v == 1),
            opacity: read(w!("fallingOpacity")).map_or(100, |v| v.clamp(10, 100)),
            mode,
            mixed: read(w!("mixed")).is_some_and(|v| v != 0),
            mix: mix_of(read(w!("mix0")), read(w!("mix1")), mode),
        }
    }

    pub fn save(&self) {
        save_holiday(self.holiday);
        write(w!("intensity"), self.intensity);
        write(w!("settleGround"), self.ground as u32);
        write(w!("meltSnow"), self.melt as u32);
        write(w!("snowOnWindows"), self.snow_on_windows as u32);
        write(w!("behindWindows"), self.behind_windows as u32);
        write(w!("showOverFullScreen"), self.over_full_screen as u32);
        write(w!("behindTaskbar"), self.behind_taskbar as u32);
        write(w!("showInScreenshots"), self.show_in_screenshots as u32);
        write(w!("deepInteraction"), self.deep_interaction as u32);
        write(w!("welcomed"), self.welcomed as u32);
        write(w!("installedAt"), self.installed_at);
        write(w!("halloweenOn"), self.halloween.on as u32);
        write(w!("halloweenSkipped"), self.halloween.skipped);
        write(w!("mode"), MODES.iter().position(|&m| m == self.mode).unwrap_or(0) as u32);
        write(w!("mixed"), self.mixed as u32);
        write(w!("mix0"), MODES.iter().position(|&m| m == self.mix[0]).unwrap_or(0) as u32);
        write(w!("mix1"), MODES.iter().position(|&m| m == self.mix[1]).unwrap_or(0) as u32);
        write(w!("autumn"), (self.mode == Mode::Autumn) as u32);
        write(w!("performance"), self.max_performance as u32);
        write(w!("fallingOpacity"), self.opacity);
    }

    /// A drag can report many positions per second; it only needs to persist the two coordinates.
    pub fn save_tree_position(&self) {
        write(w!("holidayTreeX"), self.holiday.tree_x.min(1000));
        write(w!("holidayTreeY"), self.holiday.tree_y.min(1000));
    }

    /// The kinds falling: the two mixed, or the one picked.
    pub fn modes(&self) -> Vec<Mode> {
        if self.mixed { self.mix.to_vec() } else { vec![self.mode] }
    }

    /// Picks a kind; mixed, a third kind picked takes the place of the one picked before the other. Whether anything
    /// changed.
    pub fn pick(&mut self, mode: Mode) -> bool {
        if self.mixed {
            if self.mix.contains(&mode) {
                return false;
            }
            self.mix = [self.mix[1], mode];
        } else if mode == self.mode {
            return false;
        }
        self.mode = mode;
        true
    }

    /// Mixed, the kind picked last falls along with the one picked before it; unmixed, it falls alone.
    pub fn toggle_mixed(&mut self) {
        self.mixed = !self.mixed;
        if self.mixed {
            let other = if self.mix.contains(&self.mode) { self.mix.iter().copied().find(|&m| m != self.mode) } else { Some(self.mix[1]) };
            self.mix = [other.unwrap_or(other_than(self.mode)), self.mode];
        }
    }
}

fn load_holiday() -> HolidaySettings {
    let defaults = HolidaySettings::default();
    HolidaySettings {
        garlands: read(w!("holidayGarlands")).map_or(defaults.garlands, |v| v != 0),
        tree: read(w!("holidayTree")).map_or(defaults.tree, |v| v != 0),
        tractor: read(w!("holidayTractor")).map_or(defaults.tractor, |v| v != 0),
        santa: read(w!("holidaySanta")).map_or(defaults.santa, |v| v != 0),
        sound_enabled: read(w!("holidaySoundEnabled")).map_or(defaults.sound_enabled, |v| v != 0),
        santa_sound: read(w!("holidaySantaSound")).map_or(defaults.santa_sound, |v| v != 0),
        tractor_sound: read(w!("holidayTractorSound")).map_or(defaults.tractor_sound, |v| v != 0),
        santa_volume: read(w!("holidaySantaVolume")).unwrap_or(defaults.santa_volume).min(100),
        tractor_volume: read(w!("holidayTractorVolume")).unwrap_or(defaults.tractor_volume).min(100),
        over_full_screen: read(w!("holidayOverFullScreen")).map_or(defaults.over_full_screen, |v| v != 0),
        tree_monitor: read(w!("holidayTreeMonitor")).unwrap_or(defaults.tree_monitor),
        tree_x: read(w!("holidayTreeX")).unwrap_or(defaults.tree_x).min(1000),
        tree_y: read(w!("holidayTreeY")).unwrap_or(defaults.tree_y).min(1000),
        tree_height: read(w!("holidayTreeHeight")).unwrap_or(defaults.tree_height).clamp(120, 640),
        brightness: read(w!("holidayBrightness")).unwrap_or(defaults.brightness).clamp(10, 100),
        blink_mode: read(w!("holidayBlinkMode")).unwrap_or(defaults.blink_mode).min(2),
        tractor_threshold: read(w!("holidayTractorThreshold")).unwrap_or(defaults.tractor_threshold).clamp(8, 80),
        santa_interval: read(w!("holidaySantaInterval")).unwrap_or(defaults.santa_interval).clamp(30, 1800),
    }
}

fn save_holiday(settings: HolidaySettings) {
    write(w!("holidayGarlands"), settings.garlands as u32);
    write(w!("holidayTree"), settings.tree as u32);
    write(w!("holidayTractor"), settings.tractor as u32);
    write(w!("holidaySanta"), settings.santa as u32);
    write(w!("holidaySoundEnabled"), settings.sound_enabled as u32);
    write(w!("holidaySantaSound"), settings.santa_sound as u32);
    write(w!("holidayTractorSound"), settings.tractor_sound as u32);
    write(w!("holidaySantaVolume"), settings.santa_volume.min(100));
    write(w!("holidayTractorVolume"), settings.tractor_volume.min(100));
    write(w!("holidayOverFullScreen"), settings.over_full_screen as u32);
    write(w!("holidayTreeMonitor"), settings.tree_monitor);
    write(w!("holidayTreeX"), settings.tree_x.min(1000));
    write(w!("holidayTreeY"), settings.tree_y.min(1000));
    write(w!("holidayTreeHeight"), settings.tree_height.clamp(120, 640));
    write(w!("holidayBrightness"), settings.brightness.clamp(10, 100));
    write(w!("holidayBlinkMode"), settings.blink_mode.min(2));
    write(w!("holidayTractorThreshold"), settings.tractor_threshold.clamp(8, 80));
    write(w!("holidaySantaInterval"), settings.santa_interval.clamp(30, 1800));
}

/// Two different kinds read from the settings, or `mode` and another when they don't make a pair.
fn mix_of(first: Option<u32>, second: Option<u32>, mode: Mode) -> [Mode; 2] {
    let kind = |i: Option<u32>| i.and_then(|i| MODES.get(i as usize).copied());
    match (kind(first), kind(second)) {
        (Some(a), Some(b)) if a != b => [a, b],
        _ => [other_than(mode), mode],
    }
}

/// The kind to mix with `mode` until another is picked.
fn other_than(mode: Mode) -> Mode {
    if mode == Mode::Snow { Mode::Autumn } else { Mode::Snow }
}

pub fn opens_at_login() -> bool {
    let mut buffer = [0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    let status =
        unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN, RUN_VALUE, RRF_RT_REG_SZ, None, Some(buffer.as_mut_ptr().cast::<c_void>()), Some(&mut size)) };
    if status != ERROR_SUCCESS {
        return false;
    }
    let length = buffer.iter().position(|&c| c == 0).unwrap_or(0);
    command().is_some_and(|command| command[..command.len() - 1] == buffer[..length])
}

pub fn set_opens_at_login(on: bool) -> WIN32_ERROR {
    unsafe {
        if !on {
            let current = remove_login_value(RUN_VALUE);
            return if current == ERROR_SUCCESS { remove_login_value(LEGACY_RUN_VALUE) } else { current };
        }
        let previous = login_command(RUN_VALUE);
        let Some(command) = command() else { return WIN32_ERROR(1) };
        let status = RegSetKeyValueW(HKEY_CURRENT_USER, RUN, RUN_VALUE, REG_SZ.0, Some(command.as_ptr().cast()), (command.len() * 2) as u32);
        if status != ERROR_SUCCESS {
            return status;
        }
        let removed = remove_login_value(LEGACY_RUN_VALUE);
        if removed != ERROR_SUCCESS {
            if let Some(previous) = previous {
                let _ = RegSetKeyValueW(HKEY_CURRENT_USER, RUN, RUN_VALUE, REG_SZ.0, Some(previous.as_ptr().cast()), (previous.len() * 2) as u32);
            } else {
                let _ = remove_login_value(RUN_VALUE);
            }
        }
        removed
    }
}

fn remove_login_value(name: PCWSTR) -> WIN32_ERROR {
    let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN, name) };
    if status == ERROR_FILE_NOT_FOUND || status == ERROR_PATH_NOT_FOUND { ERROR_SUCCESS } else { status }
}

fn login_command(name: PCWSTR) -> Option<Vec<u16>> {
    let mut buffer = vec![0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    let status = unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN, name, RRF_RT_REG_SZ, None, Some(buffer.as_mut_ptr().cast()), Some(&mut size)) };
    if status != ERROR_SUCCESS {
        return None;
    }
    let length = buffer.iter().position(|&unit| unit == 0)?;
    buffer.truncate(length + 1);
    Some(buffer)
}

/// The quoted path of this executable, NUL-terminated.
fn command() -> Option<Vec<u16>> {
    let path = std::env::current_exe().ok()?;
    Some(format!("\"{}\"", path.display()).encode_utf16().chain([0]).collect())
}

fn read(name: PCWSTR) -> Option<u32> {
    let mut value = 0u32;
    let mut size = 4u32;
    let status =
        unsafe { RegGetValueW(HKEY_CURRENT_USER, KEY, name, RRF_RT_REG_DWORD, None, Some((&raw mut value).cast::<c_void>()), Some(&mut size)) };
    (status == ERROR_SUCCESS).then_some(value)
}

fn now() -> u32 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as u32)
}

fn write(name: PCWSTR, value: u32) {
    unsafe {
        let _ = RegSetKeyValueW(HKEY_CURRENT_USER, KEY, name, REG_DWORD.0, Some((&raw const value).cast::<c_void>()), 4);
    }
}

/// The "Animation effects" setting: off means the person prefers less motion.
pub fn animations_enabled() -> bool {
    let mut enabled = BOOL(1);
    let result = unsafe {
        SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, Some((&raw mut enabled).cast::<c_void>()), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0))
    };
    result.is_err() || enabled.as_bool()
}
