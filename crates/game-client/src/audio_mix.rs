//! The mixer: a volume per channel (Music, Fx) under a master volume, and a "mute all" switch.
//! Pure state: `audio.rs` applies `Mixer::gain` to what plays, `user_settings.rs` saves it.

use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Channel {
    Music,
    Fx,
}

/// One of the mixer's volumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    Master,
    Channel(Channel),
}

/// A volume in percent, 0..=100.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Level(u8);

impl Level {
    /// What one menu step changes, in percent.
    pub const STEP: i32 = 10;

    pub fn new(percent: u32) -> Self {
        Level(percent.min(100) as u8)
    }

    pub fn percent(self) -> u8 {
        self.0
    }

    pub fn fraction(self) -> f32 {
        self.0 as f32 / 100.0
    }

    pub fn step(self, delta: i32) -> Self {
        Level((self.0 as i32 + delta).clamp(0, 100) as u8)
    }

    /// Up one step; past 100 back to 0 (a menu entry picked again and again goes round).
    pub fn cycle_up(self) -> Self {
        if self.0 >= 100 { Level(0) } else { self.step(Self::STEP) }
    }
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mixer {
    pub master: Level,
    pub music: Level,
    pub fx: Level,
    pub mute_all: bool,
}

impl Default for Mixer {
    fn default() -> Self {
        Mixer { master: Level(80), music: Level(60), fx: Level(80), mute_all: false }
    }
}

impl Mixer {
    pub fn level(&self, slot: Slot) -> Level {
        match slot {
            Slot::Master => self.master,
            Slot::Channel(Channel::Music) => self.music,
            Slot::Channel(Channel::Fx) => self.fx,
        }
    }

    pub fn level_mut(&mut self, slot: Slot) -> &mut Level {
        match slot {
            Slot::Master => &mut self.master,
            Slot::Channel(Channel::Music) => &mut self.music,
            Slot::Channel(Channel::Fx) => &mut self.fx,
        }
    }

    /// Linear amplitude for a channel: master × channel, squared so that the percentages feel even
    /// (half the slider sounds about half as loud); 0 while muted.
    pub fn gain(&self, channel: Channel) -> f32 {
        if self.mute_all {
            return 0.0;
        }
        let v = self.master.fraction() * self.level(Slot::Channel(channel)).fraction();
        v * v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mixer(master: u32, fx: u32) -> Mixer {
        Mixer { master: Level::new(master), fx: Level::new(fx), ..Mixer::default() }
    }

    #[test]
    fn gain_is_silent_at_zero_or_muted_and_full_at_the_top() {
        assert_eq!(mixer(0, 100).gain(Channel::Fx), 0.0);
        assert_eq!(mixer(100, 0).gain(Channel::Fx), 0.0);
        assert_eq!(mixer(100, 100).gain(Channel::Fx), 1.0);
        let muted = Mixer { mute_all: true, ..mixer(100, 100) };
        assert_eq!(muted.gain(Channel::Fx), 0.0);
        assert_eq!(muted.gain(Channel::Music), 0.0);
    }

    #[test]
    fn gain_rises_with_each_volume_and_channels_are_apart() {
        let gains: Vec<f32> = (0..=10).map(|i| mixer(100, i * 10).gain(Channel::Fx)).collect();
        assert!(gains.windows(2).all(|w| w[0] < w[1]));
        assert!(mixer(50, 100).gain(Channel::Fx) < mixer(60, 100).gain(Channel::Fx));
        let m = Mixer { master: Level::new(100), music: Level::new(0), fx: Level::new(100), mute_all: false };
        assert_eq!((m.gain(Channel::Music), m.gain(Channel::Fx)), (0.0, 1.0));
    }

    #[test]
    fn levels_clamp_when_stepped_and_wrap_when_cycled() {
        assert_eq!(Level::new(250), Level::new(100));
        assert_eq!(Level::new(95).step(10), Level::new(100));
        assert_eq!(Level::new(5).step(-10), Level::new(0));
        assert_eq!(Level::new(90).cycle_up(), Level::new(100));
        assert_eq!(Level::new(100).cycle_up(), Level::new(0));
        assert_eq!(Level::new(95).cycle_up(), Level::new(100));
    }

    #[test]
    fn slots_reach_their_own_volume() {
        let mut m = Mixer::default();
        *m.level_mut(Slot::Channel(Channel::Music)) = Level::new(30);
        *m.level_mut(Slot::Master) = Level::new(40);
        assert_eq!((m.music, m.master, m.fx), (Level::new(30), Level::new(40), Mixer::default().fx));
        assert_eq!(m.level(Slot::Channel(Channel::Music)), Level::new(30));
    }
}
