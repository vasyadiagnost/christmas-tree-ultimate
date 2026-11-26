//! Ground traffic has its own clock: it continues while snowfall is switched off, and only platforms that ask for
//! it run it. Clearing removes actual ground snow; platforms draw the powder separately without adding any mass.

use std::f32::consts::TAU;

use super::{Mode, SnowSimulation};
use crate::christmas::{HolidaySettings, TransportKind, TransportSprite};
use crate::geometry::V2;

const TRACTOR_COOLDOWN: f64 = 45.0;
const TRACTOR_SPEED: f32 = 130.0;
const SANTA_SPEED: f32 = 175.0;
const EDGE_MARGIN: f32 = 24.0;

#[derive(Default)]
pub(super) struct Christmas {
    vehicle: Option<TransportSprite>,
    requested: Option<TransportKind>,
    santa_elapsed: f64,
    tractor_cooldown: f64,
    reverse: bool,
}

impl SnowSimulation {
    /// Advances Christmas traffic using real elapsed time, independently of the snowfall simulation's clock.
    /// Call after `step`, and include the return value when deciding whether to draw this screen.
    pub fn step_christmas(&mut self, dt: f32, settings: &HolidaySettings, selected: bool) -> bool {
        let settings = settings.sanitized();
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let was_active = self.christmas_active();
        let allowed = selected
            && self.accumulates
            && self.ground_settles
            && self.air.modes.contains(&Mode::Snow)
            && self.next_mode.is_none()
            && self.air.gust.is_none();
        if !allowed {
            self.christmas = Christmas::default();
            return was_active;
        }

        self.christmas.tractor_cooldown = (self.christmas.tractor_cooldown - dt as f64).max(0.0);
        if settings.santa {
            self.christmas.santa_elapsed += dt as f64;
        } else {
            self.christmas.santa_elapsed = 0.0;
        }
        let enabled = |kind| match kind {
            TransportKind::Tractor => settings.tractor,
            TransportKind::Santa => settings.santa,
        };
        if self.christmas.vehicle.is_some_and(|vehicle| !enabled(vehicle.kind)) {
            self.christmas.vehicle = None;
        }
        if self.christmas.requested.is_some_and(|kind| !enabled(kind)) {
            self.christmas.requested = None;
        }

        if self.christmas.vehicle.is_none() {
            let depth = self.christmas_snow_depth();
            // Both events need real ground snow, rather than leaves or just snow resting on a window.
            let next = if depth < 0.5 {
                self.christmas.requested = None;
                None
            } else if let Some(requested) = self.christmas.requested.take() {
                Some(requested)
            } else if settings.santa && self.christmas.santa_elapsed >= settings.santa_interval as f64 {
                Some(TransportKind::Santa)
            } else if settings.tractor && self.christmas.tractor_cooldown <= 0.0 && depth >= settings.tractor_threshold as f32 {
                Some(TransportKind::Tractor)
            } else {
                None
            };
            if let Some(kind) = next {
                self.start_christmas_vehicle(kind);
            }
        }

        let Some(mut vehicle) = self.christmas.vehicle else { return was_active };
        let previous_x = vehicle.anchor.x;
        let speed = if vehicle.kind == TransportKind::Tractor { TRACTOR_SPEED } else { SANTA_SPEED };
        let end = if vehicle.direction > 0.0 { self.width + vehicle.width * 0.5 + EDGE_MARGIN } else { -vehicle.width * 0.5 - EDGE_MARGIN };
        // Clipping the motion to its remaining path also handles a long frame without skipping any snow columns.
        let travel = (speed * dt).min((end - previous_x).abs());
        vehicle.anchor.x += vehicle.direction * travel;
        vehicle.phase = (vehicle.phase + travel * 0.045).rem_euclid(TAU);
        if vehicle.kind == TransportKind::Tractor {
            self.clear_christmas_path(previous_x, vehicle.anchor.x, vehicle.direction, vehicle.width);
        }
        let (height, angle) = self.christmas_support(vehicle.anchor.x, vehicle.width, vehicle.kind);
        let ease = 1.0 - (-dt * 7.0).exp();
        vehicle.anchor.y += (height - vehicle.anchor.y) * ease;
        vehicle.angle += (angle - vehicle.angle) * ease;

        if (vehicle.anchor.x - end).abs() <= 0.01 {
            self.christmas.vehicle = None;
            if vehicle.kind == TransportKind::Tractor {
                self.christmas.tractor_cooldown = TRACTOR_COOLDOWN;
            }
        } else {
            self.christmas.vehicle = Some(vehicle);
        }
        true
    }

    pub fn christmas_sprites(&self) -> Vec<TransportSprite> {
        self.christmas.vehicle.into_iter().collect()
    }

    pub fn christmas_active(&self) -> bool {
        self.christmas.vehicle.is_some()
    }

    /// Requests one pass. An ongoing pass finishes first; the next step still checks the setting and ground snow.
    pub fn request_christmas(&mut self, kind: TransportKind) {
        self.christmas.requested = Some(kind);
    }

    /// Used for a mode change, Clear Snow, a new monitor layout, or disabling the scene.
    pub fn reset_christmas(&mut self) {
        self.restyled |= self.christmas_active();
        self.christmas = Christmas::default();
    }

    fn christmas_snow_depth(&self) -> f32 {
        let ground = &self.layers[0];
        let mut snow = 0.0;
        let mut width = 0.0;
        for i in 0..ground.count() {
            let left = ground.center_x(i) - ground.column_width * 0.5;
            let span = (self.width - left).clamp(0.0, ground.column_width);
            snow += (ground.heights[i] - ground.leaves[i]).max(0.0) * span;
            width += span;
        }
        if width > 0.0 { snow / width } else { 0.0 }
    }

    fn start_christmas_vehicle(&mut self, kind: TransportKind) {
        let (width, height) = match kind {
            TransportKind::Tractor => (180.0, 90.0),
            TransportKind::Santa => (440.0, 176.0),
        };
        let direction = if self.christmas.reverse { -1.0 } else { 1.0 };
        self.christmas.reverse = !self.christmas.reverse;
        let x = if direction > 0.0 { -width * 0.5 - EDGE_MARGIN } else { self.width + width * 0.5 + EDGE_MARGIN };
        let (y, angle) = self.christmas_support(x, width, kind);
        self.christmas.vehicle = Some(TransportSprite { kind, anchor: V2::new(x, y), width, height, direction, phase: 0.0, angle });
        if kind == TransportKind::Santa {
            self.christmas.santa_elapsed = 0.0;
        }
    }

    fn christmas_support(&self, x: f32, width: f32, kind: TransportKind) -> (f32, f32) {
        let ground = &self.layers[0];
        let reach = width * 0.22;
        let sample = |center: f32| {
            let from = ground.clamped_index(center - 12.0);
            let to = ground.clamped_index(center + 12.0);
            (from..=to).map(|i| ground.floor(i) + if kind == TransportKind::Tractor { ground.leaves[i] } else { ground.heights[i] }).sum::<f32>()
                / (to - from + 1) as f32
        };
        let left = sample(x - reach);
        let right = sample(x + reach);
        ((left + right) * 0.5, ((right - left) / (2.0 * reach)).atan().clamp(-0.18, 0.18))
    }

    fn clear_christmas_path(&mut self, from: f32, to: f32, direction: f32, width: f32) {
        let blade = direction * width * 0.44;
        let left = (from + blade).min(to + blade) - 5.0;
        let right = (from + blade).max(to + blade) + 5.0;
        let ground = &mut self.layers[0];
        let mut cleared = false;
        for i in 0..ground.count() {
            let x = ground.center_x(i);
            if x < left || x > right {
                continue;
            }
            let snow = (ground.heights[i] - ground.leaves[i]).max(0.0);
            if snow <= 0.0 {
                continue;
            }
            ground.remove_snow(snow, i);
            cleared = true;
        }
        if !cleared {
            return;
        }
        ground.refresh_top();
        self.air.heights_changed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::SnowLayer;

    fn snowy_screen(snow: f32, leaves: f32) -> SnowSimulation {
        let mut simulation = SnowSimulation::new(800.0, 600.0, 0.0);
        for i in 0..simulation.layers[0].count() {
            simulation.layers[0].set(i, snow + leaves);
            simulation.layers[0].leaves[i] = leaves;
            simulation.layers[0].petals[i] = leaves * 0.5;
        }
        simulation.layers[0].refresh_top();
        simulation
    }

    fn manual_settings() -> HolidaySettings {
        HolidaySettings { tractor_threshold: 80, santa_interval: 1800, ..HolidaySettings::default() }
    }

    #[test]
    fn tractor_clears_the_ground_at_different_frame_rates_and_preserves_windows_and_leaves() {
        for dt in [1.0_f32 / 120.0, 1.0 / 30.0, 0.75, 30.0] {
            let mut simulation = snowy_screen(40.0, 5.0);
            let mut window = SnowLayer::new(V2::new(100.0, 350.0), 120, 2.0, 40.0, true, (0.0, 0.0));
            for i in 0..window.count() {
                window.set(i, 18.0);
            }
            simulation.layers.push(window);
            simulation.ids.push(9);
            simulation.request_christmas(TransportKind::Tractor);
            for _ in 0..(12.0 / dt).ceil() as usize {
                simulation.step_christmas(dt, &manual_settings(), true);
            }
            assert!(!simulation.christmas_active(), "pass did not finish for dt={dt}");
            for i in 0..simulation.layers[0].count() {
                assert_eq!(simulation.layers[0].heights[i], 5.0, "column {i}, dt={dt}");
                assert_eq!(simulation.layers[0].leaves[i], 5.0);
                assert_eq!(simulation.layers[0].petals[i], 2.5);
            }
            assert!(simulation.layers[1].heights.iter().all(|&height| height == 18.0));
            assert!(simulation.air.grains.is_empty(), "cleared snow must not be deposited again");
            assert!(simulation.air.motes.is_empty(), "the independent decoration renderer draws the powder");
        }
    }

    #[test]
    fn a_partial_pass_changes_only_the_ground_the_blade_has_reached() {
        let mut simulation = snowy_screen(40.0, 0.0);
        simulation.request_christmas(TransportKind::Tractor);
        simulation.step_christmas(2.0, &manual_settings(), true);
        let vehicle = simulation.christmas_sprites()[0];
        let blade = vehicle.anchor.x + vehicle.width * 0.44;
        for i in 0..simulation.layers[0].count() {
            let x = simulation.layers[0].center_x(i);
            assert_eq!(simulation.layers[0].heights[i], if x <= blade + 5.0 { 0.0 } else { 40.0 });
        }
    }

    #[test]
    fn leaves_and_window_snow_alone_cannot_start_winter_traffic() {
        let mut simulation = snowy_screen(0.0, 40.0);
        assert!(simulation.has_snow(), "the generic predicate includes leaves");
        for kind in [TransportKind::Tractor, TransportKind::Santa] {
            simulation.request_christmas(kind);
            simulation.step_christmas(40.0, &HolidaySettings::default(), true);
            assert!(!simulation.christmas_active());
        }
        assert!(simulation.layers[0].heights.iter().all(|&height| height == 40.0));
        let mut window = SnowLayer::new(V2::new(100.0, 350.0), 120, 2.0, 40.0, true, (0.0, 0.0));
        for i in 0..window.count() {
            window.set(i, 30.0);
        }
        simulation.layers.push(window);
        simulation.ids.push(9);
        simulation.request_christmas(TransportKind::Tractor);
        simulation.step_christmas(0.1, &HolidaySettings::default(), true);
        assert!(!simulation.christmas_active());
    }

    #[test]
    fn santa_uses_real_elapsed_time_and_follows_the_snow_without_removing_it() {
        let mut simulation = snowy_screen(20.0, 0.0);
        let settings = HolidaySettings { tractor: false, santa_interval: 30, ..HolidaySettings::default() };
        simulation.step_christmas(29.0, &settings, true);
        assert!(!simulation.christmas_active());
        simulation.step_christmas(1.0, &settings, true);
        assert_eq!(simulation.air.clock, 0.0, "traffic must not rely on snowfall steps");
        let vehicle = simulation.christmas_sprites()[0];
        assert_eq!(vehicle.kind, TransportKind::Santa);
        assert!((vehicle.anchor.y - 20.0).abs() < 0.01);
        assert!(vehicle.angle.is_finite() && vehicle.phase.is_finite());
        assert!(simulation.layers[0].heights.iter().all(|&height| height == 20.0));
    }

    #[test]
    fn changing_modes_clearing_and_deselecting_cancel_the_pass() {
        let settings = manual_settings();
        let mut simulation = snowy_screen(40.0, 0.0);
        simulation.request_christmas(TransportKind::Santa);
        simulation.step_christmas(0.1, &settings, true);
        assert!(simulation.christmas_active());
        simulation.set_mode(Mode::Autumn);
        assert!(!simulation.christmas_active());
        simulation.request_christmas(TransportKind::Tractor);
        simulation.step_christmas(0.1, &settings, true);
        assert!(!simulation.christmas_active());
        simulation.set_mode(Mode::Snow);
        simulation.request_christmas(TransportKind::Santa);
        simulation.step_christmas(0.1, &settings, true);
        assert!(simulation.christmas_active());
        simulation.blow_away(1.0, false);
        assert!(!simulation.christmas_active());
        simulation.air.gust = None;
        simulation.request_christmas(TransportKind::Santa);
        simulation.step_christmas(0.1, &settings, true);
        assert!(simulation.christmas_active());
        assert!(simulation.step_christmas(0.1, &settings, false));
        assert!(!simulation.christmas_active());
    }

    #[test]
    fn a_disabled_ground_or_feature_cancels_the_active_vehicle() {
        for ground_off in [false, true] {
            let mut simulation = snowy_screen(40.0, 0.0);
            let mut settings = manual_settings();
            simulation.request_christmas(TransportKind::Santa);
            simulation.step_christmas(0.1, &settings, true);
            if ground_off {
                simulation.ground_settles = false;
            } else {
                settings.santa = false;
            }
            assert!(simulation.step_christmas(0.1, &settings, true));
            assert!(!simulation.christmas_active());
        }
    }

    #[test]
    fn a_monitor_resize_and_switching_off_accumulation_cancel_traffic() {
        let mut simulation = snowy_screen(40.0, 0.0);
        let settings = manual_settings();
        simulation.request_christmas(TransportKind::Santa);
        simulation.step_christmas(0.1, &settings, true);
        simulation.request_christmas(TransportKind::Tractor);
        simulation.resize(640.0, 480.0);
        assert!(!simulation.christmas_active());
        simulation.step_christmas(0.1, &settings, true);
        assert!(!simulation.christmas_active(), "the old monitor's queued request must go too");
        simulation.request_christmas(TransportKind::Santa);
        simulation.step_christmas(0.1, &settings, true);
        assert!(simulation.christmas_active());
        simulation.accumulates = false;
        assert!(simulation.step_christmas(0.1, &settings, true));
        assert!(!simulation.christmas_active());
    }

    #[test]
    fn pure_rain_and_petals_do_not_start_ground_traffic() {
        for mode in [Mode::Rain, Mode::Autumn, Mode::Blossom] {
            let mut simulation = snowy_screen(40.0, 0.0);
            simulation.set_mode(mode);
            simulation.request_christmas(TransportKind::Tractor);
            simulation.step_christmas(0.1, &HolidaySettings::default(), true);
            assert!(!simulation.christmas_active(), "unexpected vehicle in {mode:?}");
            assert!(simulation.layers[0].heights.iter().all(|&height| height == 40.0));
        }
    }

    #[test]
    fn automatic_tractor_requires_the_threshold_and_waits_after_a_pass() {
        let mut simulation = snowy_screen(10.0, 0.0);
        let settings = HolidaySettings { santa: false, tractor_threshold: 24, ..HolidaySettings::default() };
        simulation.step_christmas(1.0, &settings, true);
        assert!(!simulation.christmas_active());
        for i in 0..simulation.layers[0].count() {
            simulation.layers[0].set(i, 30.0);
        }
        simulation.step_christmas(0.1, &settings, true);
        assert!(simulation.christmas_active());
        simulation.step_christmas(20.0, &settings, true);
        assert!(!simulation.christmas_active());
        for i in 0..simulation.layers[0].count() {
            simulation.layers[0].set(i, 30.0);
        }
        simulation.step_christmas(1.0, &settings, true);
        assert!(!simulation.christmas_active(), "a new pile must not bypass the cooldown");
        simulation.step_christmas(44.0, &settings, true);
        assert!(!simulation.christmas_active(), "a long step should finish the next pass too");
        assert!(simulation.layers[0].heights.iter().all(|&height| height == 0.0));
    }

    #[test]
    fn santa_and_the_tractor_never_overlap_and_bad_elapsed_time_is_safe() {
        let mut simulation = snowy_screen(40.0, 0.0);
        let settings = HolidaySettings::default();
        simulation.request_christmas(TransportKind::Santa);
        simulation.step_christmas(0.1, &settings, true);
        simulation.request_christmas(TransportKind::Tractor);
        for dt in [f32::NAN, -1.0, f32::INFINITY, 0.1, 0.5] {
            simulation.step_christmas(dt, &settings, true);
            let sprites = simulation.christmas_sprites();
            assert_eq!(sprites.len(), 1);
            assert_eq!(sprites[0].kind, TransportKind::Santa);
            assert!(sprites[0].anchor.x.is_finite() && sprites[0].anchor.y.is_finite());
        }
    }
}
