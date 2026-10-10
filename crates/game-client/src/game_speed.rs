//! Game speed: the simulation paused or run 1, 2, 4 or 8 ticks per tick time (`SimClock::steps_due`). The
//! camera, the HUD and purely visual motion keep their pace. P pauses, Shift + `+` / `-` (and the numpad's)
//! change the speed, as in the original; the pause menu cycles it; `GAME_SPEED=0|1|2|4|8` sets the start.
//! A label at the top of the view shows it when not normal.

use crate::hud::PANEL_WIDTH;
use crate::keymap::{Shortcut, Shortcuts};
use bevy::prelude::*;

pub const SPEEDS: [u8; 4] = [1, 2, 4, 8];

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameSpeed {
    pub paused: bool,
    /// Ticks per tick time, one of `SPEEDS`.
    pub times: u8,
}

impl Default for GameSpeed {
    fn default() -> Self {
        GameSpeed { paused: false, times: 1 }
    }
}

impl GameSpeed {
    pub fn from_env(value: Option<&str>) -> Self {
        match value.and_then(|v| v.parse::<u8>().ok()) {
            Some(0) => GameSpeed { paused: true, times: 1 },
            Some(n) if SPEEDS.contains(&n) => GameSpeed { paused: false, times: n },
            _ => GameSpeed::default(),
        }
    }

    /// Ticks run per tick time: 0 while paused.
    pub fn ticks_per_step(self) -> u32 {
        if self.paused { 0 } else { self.times as u32 }
    }

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    /// One step faster (it also resumes), up to the fastest.
    pub fn faster(&mut self) {
        self.paused = false;
        self.times = SPEEDS.iter().copied().find(|&s| s > self.times).unwrap_or(self.times);
    }

    /// One step slower (it also resumes), down to normal.
    pub fn slower(&mut self) {
        self.paused = false;
        self.times = SPEEDS.iter().rev().copied().find(|&s| s < self.times).unwrap_or(self.times);
    }

    /// Next speed round the list, for the pause menu.
    pub fn cycle(&mut self) {
        let i = SPEEDS.iter().position(|&s| s == self.times).unwrap_or(0);
        self.times = SPEEDS[(i + 1) % SPEEDS.len()];
    }

    /// What the top of the view shows; None at normal speed.
    pub fn label(self) -> Option<String> {
        match (self.paused, self.times) {
            (true, _) => Some("Paused".into()),
            (false, 1) => None,
            (false, n) => Some(format!("Speed x{n}")),
        }
    }
}

#[derive(Component)]
struct SpeedLabel;

pub struct GameSpeedPlugin;

impl Plugin for GameSpeedPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GameSpeed::from_env(std::env::var("GAME_SPEED").ok().as_deref()))
            .add_systems(Startup, spawn_label)
            .add_systems(Update, (speed_keys.in_set(crate::menu::Gameplay), update_label).chain());
    }
}

fn speed_keys(keys: Shortcuts, mut speed: ResMut<GameSpeed>) {
    if keys.just_pressed(Shortcut::Pause) {
        speed.toggle_pause();
    }
    if keys.just_pressed(Shortcut::Faster) {
        speed.faster();
    }
    if keys.just_pressed(Shortcut::Slower) {
        speed.slower();
    }
}

fn spawn_label(mut commands: Commands) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: px(40),
            left: px(PANEL_WIDTH),
            right: px(0),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_child((SpeedLabel, Text::new(""), TextFont { font_size: FontSize::Px(22.0), ..default() }, TextShadow::default(), Visibility::Hidden));
}

fn update_label(speed: Res<GameSpeed>, mut q: Query<(&mut Text, &mut Visibility), With<SpeedLabel>>) {
    if !speed.is_changed() {
        return;
    }
    let label = speed.label();
    for (mut text, mut vis) in &mut q {
        *vis = if label.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        text.0 = label.clone().unwrap_or_default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faster_and_slower_step_through_the_speeds_and_resume() {
        let mut s = GameSpeed::default();
        s.faster();
        s.faster();
        assert_eq!(s.ticks_per_step(), 4);
        s.faster();
        s.faster();
        assert_eq!(s.times, 8, "capped");
        s.toggle_pause();
        assert_eq!(s.ticks_per_step(), 0);
        s.slower();
        assert_eq!((s.paused, s.times), (false, 4), "slower resumes");
        for _ in 0..5 {
            s.slower();
        }
        assert_eq!(s.times, 1);
    }

    #[test]
    fn cycles_for_the_menu_and_reads_the_env() {
        let mut s = GameSpeed { paused: false, times: 8 };
        s.cycle();
        assert_eq!(s.times, 1);
        assert_eq!(GameSpeed::from_env(Some("4")), GameSpeed { paused: false, times: 4 });
        assert_eq!(GameSpeed::from_env(Some("0")).ticks_per_step(), 0);
        assert_eq!(GameSpeed::from_env(Some("3")), GameSpeed::default(), "not a speed");
        assert_eq!(GameSpeed::from_env(None), GameSpeed::default());
    }

    #[test]
    fn labels_only_when_not_normal() {
        assert_eq!(GameSpeed::default().label(), None);
        assert_eq!(GameSpeed { paused: false, times: 2 }.label().as_deref(), Some("Speed x2"));
        assert_eq!(GameSpeed { paused: true, times: 2 }.label().as_deref(), Some("Paused"));
    }
}
