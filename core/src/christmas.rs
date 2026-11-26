//! Settings and draw data for the optional Christmas decorations. Platforms opt into their animation explicitly.

use crate::geometry::V2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HolidaySettings {
    pub garlands: bool,
    pub tree: bool,
    pub tractor: bool,
    pub santa: bool,
    pub sound_enabled: bool,
    pub santa_sound: bool,
    pub tractor_sound: bool,
    /// Independent application voice gains, in percent; zero is silence.
    pub santa_volume: u32,
    pub tractor_volume: u32,
    pub over_full_screen: bool,
    /// Zero selects the primary screen; other values are a stable FNV-1a hash of the Windows display name.
    pub tree_monitor: u32,
    /// Position of the tree's foot within the available space, in thousandths, from the left and bottom.
    pub tree_x: u32,
    pub tree_y: u32,
    pub tree_height: u32,
    pub brightness: u32,
    /// Zero is steady light, one is a gentle twinkle, two is a running light.
    pub blink_mode: u32,
    /// Mean depth of actual ground snow, in points, which starts a clearing pass.
    pub tractor_threshold: u32,
    pub santa_interval: u32,
}

impl Default for HolidaySettings {
    fn default() -> Self {
        Self {
            garlands: true,
            tree: true,
            tractor: true,
            santa: true,
            sound_enabled: true,
            santa_sound: true,
            tractor_sound: true,
            santa_volume: 20,
            tractor_volume: 12,
            over_full_screen: true,
            tree_monitor: 0,
            tree_x: 850,
            tree_y: 0,
            tree_height: 320,
            brightness: 80,
            blink_mode: 1,
            tractor_threshold: 24,
            santa_interval: 180,
        }
    }
}

impl HolidaySettings {
    pub fn sanitized(mut self) -> Self {
        self.tree_x = self.tree_x.min(1000);
        self.tree_y = self.tree_y.min(1000);
        self.tree_height = self.tree_height.clamp(120, 640);
        self.brightness = self.brightness.clamp(10, 100);
        self.blink_mode = self.blink_mode.min(2);
        self.tractor_threshold = self.tractor_threshold.clamp(8, 80);
        self.santa_interval = self.santa_interval.clamp(30, 1800);
        self.santa_volume = self.santa_volume.min(100);
        self.tractor_volume = self.tractor_volume.min(100);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportKind {
    Tractor,
    Santa,
}

/// One vehicle's middle bottom, size in points, heading (one or minus one), animation phase and world-space angle.
#[derive(Clone, Copy, Debug)]
pub struct TransportSprite {
    pub kind: TransportKind,
    pub anchor: V2,
    pub width: f32,
    pub height: f32,
    pub direction: f32,
    pub phase: f32,
    pub angle: f32,
}

#[cfg(test)]
mod tests {
    use super::HolidaySettings;

    #[test]
    fn settings_bound_values_loaded_from_outside_the_app() {
        let settings = HolidaySettings {
            tree_x: u32::MAX,
            tree_y: u32::MAX,
            tree_height: 0,
            brightness: 0,
            blink_mode: u32::MAX,
            tractor_threshold: u32::MAX,
            santa_interval: 0,
            ..HolidaySettings::default()
        }
        .sanitized();
        assert_eq!((settings.tree_x, settings.tree_y), (1000, 1000));
        assert_eq!((settings.tree_height, settings.brightness, settings.blink_mode), (120, 10, 2));
        assert_eq!((settings.tractor_threshold, settings.santa_interval), (80, 30));
        assert_eq!(HolidaySettings::default().sanitized(), HolidaySettings::default());
    }

    #[test]
    fn sound_defaults_are_quiet_and_volume_bounds_preserve_silence() {
        let defaults = HolidaySettings::default();
        assert!(defaults.sound_enabled && defaults.santa_sound && defaults.tractor_sound);
        assert_eq!((defaults.santa_volume, defaults.tractor_volume), (20, 12));
        let muted = HolidaySettings { sound_enabled: false, santa_volume: 0, tractor_volume: 0, ..defaults }.sanitized();
        assert!(!muted.sound_enabled);
        assert_eq!((muted.santa_volume, muted.tractor_volume), (0, 0));
        let maximum = HolidaySettings { santa_volume: u32::MAX, tractor_volume: 101, ..defaults }.sanitized();
        assert_eq!((maximum.santa_volume, maximum.tractor_volume), (100, 100));
    }
}
