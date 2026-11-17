//! Our own playable sound: a clip's bytes decoded as usual, always out in stereo, each side scaled by gains the
//! game updates every frame (`Gains`). This is what gives a sound its volume and its equal-power balance, instead
//! of Bevy's spatial sink (whose `1/d²` has no cut-off and whose balance never goes fully to one side).

use bevy::audio::{AudioSource, ChannelCount, Decodable, Sample, SampleRate, Source};
use bevy::prelude::*;
use std::num::NonZero;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Left and right gains, shared between the game and the audio thread.
#[derive(Debug, Default)]
pub struct Gains {
    left: AtomicU32,
    right: AtomicU32,
}

impl Gains {
    pub fn new(left: f32, right: f32) -> Arc<Self> {
        let g = Gains::default();
        g.set(left, right);
        Arc::new(g)
    }

    pub fn set(&self, left: f32, right: f32) {
        self.left.store(left.to_bits(), Ordering::Relaxed);
        self.right.store(right.to_bits(), Ordering::Relaxed);
    }

    pub fn get(&self) -> (f32, f32) {
        (f32::from_bits(self.left.load(Ordering::Relaxed)), f32::from_bits(self.right.load(Ordering::Relaxed)))
    }
}

/// One playing instance: the clip and its own gains.
#[derive(Asset, TypePath)]
pub struct PannedSound {
    pub source: AudioSource,
    pub gains: Arc<Gains>,
}

impl Decodable for PannedSound {
    type Decoder = Panned<<AudioSource as Decodable>::Decoder>;

    fn decoder(&self) -> Self::Decoder {
        Panned::new(self.source.decoder(), self.gains.clone())
    }
}

/// A source played in stereo with live gains: a mono one is doubled, a stereo one scaled per side.
pub struct Panned<S> {
    inner: S,
    mono: bool,
    gains: Arc<Gains>,
    /// Mono: the right sample of the current frame, still to give.
    pending: Option<Sample>,
    /// Stereo: the right gain for the current frame's second sample; None at a frame start.
    right: Option<f32>,
}

impl<S: Source> Panned<S> {
    pub fn new(inner: S, gains: Arc<Gains>) -> Self {
        let mono = inner.channels().get() == 1;
        Panned { inner, mono, gains, pending: None, right: None }
    }
}

impl<S: Source> Iterator for Panned<S> {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if self.mono {
            if let Some(r) = self.pending.take() {
                return Some(r);
            }
            let s = self.inner.next()?;
            let (l, r) = self.gains.get();
            self.pending = Some(s * r as Sample);
            return Some(s * l as Sample);
        }
        let s = self.inner.next()?;
        match self.right.take() {
            Some(r) => Some(s * r as Sample),
            None => {
                let (l, r) = self.gains.get();
                self.right = Some(r);
                Some(s * l as Sample)
            }
        }
    }
}

impl<S: Source> Source for Panned<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len().map(|n| if self.mono { n * 2 } else { n })
    }

    fn channels(&self) -> ChannelCount {
        if self.mono { NonZero::new(2).unwrap() } else { self.inner.channels() }
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Samples {
        data: std::vec::IntoIter<Sample>,
        channels: u16,
    }

    impl Iterator for Samples {
        type Item = Sample;
        fn next(&mut self) -> Option<Sample> {
            self.data.next()
        }
    }

    impl Source for Samples {
        fn current_span_len(&self) -> Option<usize> {
            None
        }
        fn channels(&self) -> ChannelCount {
            NonZero::new(self.channels).unwrap()
        }
        fn sample_rate(&self) -> SampleRate {
            NonZero::new(22050).unwrap()
        }
        fn total_duration(&self) -> Option<Duration> {
            None
        }
    }

    fn panned(data: Vec<Sample>, channels: u16, gains: &Arc<Gains>) -> Panned<Samples> {
        Panned::new(Samples { data: data.into_iter(), channels }, gains.clone())
    }

    #[test]
    fn mono_comes_out_as_stereo_scaled_per_side() {
        let g = Gains::new(1.0, 0.5);
        let p = panned(vec![1.0, -0.5], 1, &g);
        assert_eq!(p.channels().get(), 2);
        assert_eq!(p.collect::<Vec<_>>(), vec![1.0, 0.5, -0.5, -0.25]);
    }

    #[test]
    fn stereo_is_scaled_per_side() {
        let g = Gains::new(0.0, 2.0);
        assert_eq!(panned(vec![1.0, 1.0, 0.5, 0.5], 2, &g).collect::<Vec<_>>(), vec![0.0, 2.0, 0.0, 1.0]);
    }

    #[test]
    fn gains_changed_while_playing_apply_from_the_next_frame() {
        let g = Gains::new(1.0, 1.0);
        let mut p = panned(vec![1.0, 1.0, 1.0], 1, &g);
        assert_eq!(p.next(), Some(1.0));
        g.set(0.25, 0.75);
        assert_eq!(p.next(), Some(1.0), "the rest of the frame keeps its gains");
        assert_eq!((p.next(), p.next()), (Some(0.25), Some(0.75)));
        assert_eq!(g.get(), (0.25, 0.75));
    }
}
