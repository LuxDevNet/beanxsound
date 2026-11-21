//! beanxdsp: the DSP core for beanxsound.
//!
//! Design rules, enforced by the code below:
//!   * no Windows types, so it unit tests anywhere
//!   * no allocation, no locks, no panics in the processing path
//!   * every public measurement has a test that checks a dB target, not a shape
//!
//! Signal order: input gain -> parametric EQ -> stereo width -> limiter.

// std is linked (audiodg is a normal process) but the processing path never allocates.
#![forbid(unsafe_code)]

use core::f32::consts::PI;

pub const MAX_BANDS: usize = 31;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BandKind {
    Peaking,
    LowShelf,
    HighShelf,
    HighPass,
    LowPass,
}

#[derive(Clone, Copy, Debug)]
pub struct Band {
    pub kind: BandKind,
    pub freq: f32,
    pub q: f32,
    pub gain_db: f32,
}

impl Band {
    pub const fn peaking(freq: f32, q: f32, gain_db: f32) -> Self {
        Self { kind: BandKind::Peaking, freq, q, gain_db }
    }
    pub const fn constant_q(freq: f32, q: f32) -> Self {
        Self { kind: BandKind::Peaking, freq, q, gain_db: 0.0 }
    }
}

/// Direct form 1 biquad, transposed so state is two f32.
#[derive(Clone, Copy, Default)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    pub fn identity() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, z1: 0.0, z2: 0.0 }
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    fn normalise(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        let inv = 1.0 / a0;
        Self { b0: b0 * inv, b1: b1 * inv, b2: b2 * inv, a1: a1 * inv, a2: a2 * inv, z1: 0.0, z2: 0.0 }
    }

    pub fn set(&mut self, band: &Band, sample_rate: f32) {
        let fs = if sample_rate > 0.0 { sample_rate } else { 48_000.0 };
        let f0 = band.freq.clamp(10.0, fs * 0.49);
        let q = if band.q > 0.0 { band.q } else { 0.707 };
        let w0 = 2.0 * PI * f0 / fs;
        let cos_w0 = libm_cos(w0);
        let sin_w0 = libm_sin(w0);
        let a = libm_pow10(band.gain_db / 40.0);
        let alpha = sin_w0 / (2.0 * q);

        let next = match band.kind {
            BandKind::Peaking => Self::normalise(
                1.0 + alpha * a,
                -2.0 * cos_w0,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos_w0,
                1.0 - alpha / a,
            ),
            BandKind::LowShelf => {
                let sqrt_a = libm_sqrt(a);
                let two_sqrt_a_alpha = 2.0 * sqrt_a * alpha;
                Self::normalise(
                    a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),
                    a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
                    (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),
                    (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
                )
            }
            BandKind::HighShelf => {
                let sqrt_a = libm_sqrt(a);
                let two_sqrt_a_alpha = 2.0 * sqrt_a * alpha;
                Self::normalise(
                    a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),
                    a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
                    (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),
                    (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
                )
            }
            BandKind::LowPass => Self::normalise(
                (1.0 - cos_w0) * 0.5,
                1.0 - cos_w0,
                (1.0 - cos_w0) * 0.5,
                1.0 + alpha,
                -2.0 * cos_w0,
                1.0 - alpha,
            ),
            BandKind::HighPass => Self::normalise(
                (1.0 + cos_w0) * 0.5,
                -(1.0 + cos_w0),
                (1.0 + cos_w0) * 0.5,
                1.0 + alpha,
                -2.0 * cos_w0,
                1.0 - alpha,
            ),
        };

        // Keep biquad state across coefficient changes: a live EQ edit must not click.
        let z1 = self.z1;
        let z2 = self.z2;
        *self = next;
        self.z1 = z1;
        self.z2 = z2;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Peak limiter with a fixed ceiling and a short lookahead-free attack.
#[derive(Clone, Copy)]
pub struct Limiter {
    ceiling: f32,
    release: f32,
    gain: f32,
    last_reduction_db: f32,
}

impl Default for Limiter {
    fn default() -> Self {
        Self { ceiling: -0.3, release: 0.05, gain: 1.0, last_reduction_db: 0.0 }
    }
}

impl Limiter {
    pub fn set_ceiling_db(&mut self, db: f32) {
        self.ceiling = db;
    }

    pub fn gain_reduction_db(&self) -> f32 {
        self.last_reduction_db
    }

    /// Instant attack, smooth release. No lookahead, so the ceiling is a hard
    /// guarantee: the gain applied to a sample is derived from that sample's own
    /// peak, never from a lagging envelope.
    #[inline]
    pub fn process(&mut self, x: f32, sample_rate: f32) -> f32 {
        let peak = if x < 0.0 { -x } else { x };
        let ceiling_lin = libm_pow10(self.ceiling / 20.0);
        let release_coef = 1.0 - libm_exp(-1.0 / (self.release * sample_rate));

        let desired = if peak > ceiling_lin && peak > 0.0 {
            ceiling_lin / peak
        } else {
            1.0
        };

        if desired < self.gain {
            self.gain = desired;
        } else {
            self.gain += (desired - self.gain) * release_coef;
        }

        self.last_reduction_db = -20.0 * libm_log10(if self.gain > 0.0 { self.gain } else { 1e-6 });
        let y = x * self.gain;
        if y > ceiling_lin { ceiling_lin } else if y < -ceiling_lin { -ceiling_lin } else { y }
    }
}

pub struct Engine {
    pub sample_rate: f32,
    pub channels: usize,
    bands: [Band; MAX_BANDS],
    biquads: [Biquad; MAX_BANDS],
    active: usize,
    pub input_gain_db: f32,
    gain_lin: f32,
    pub width: f32,
    pub limiter_on: bool,
    limiter: Limiter,
    pub bypass: bool,
}

impl Engine {
    pub fn new(sample_rate: f32, channels: usize) -> Self {
        let mut biquads = [Biquad::identity(); MAX_BANDS];
        for b in biquads.iter_mut() {
            *b = Biquad::identity();
        }
        Self {
            sample_rate,
            channels,
            bands: [Band::constant_q(1000.0, 1.0); MAX_BANDS],
            biquads,
            active: 0,
            input_gain_db: 0.0,
            gain_lin: 1.0,
            width: 1.0,
            limiter_on: false,
            limiter: Limiter::default(),
            bypass: false,
        }
    }

    pub fn set_bands(&mut self, bands: &[Band]) {
        let n = bands.len().min(MAX_BANDS);
        for (i, band) in bands.iter().take(n).enumerate() {
            self.bands[i] = *band;
            self.biquads[i].set(band, self.sample_rate);
        }
        self.active = n;
    }

    pub fn set_limiter_ceiling_db(&mut self, db: f32) {
        self.limiter.set_ceiling_db(db);
        self.limiter_on = true;
    }

    pub fn set_input_gain_db(&mut self, db: f32) {
        self.input_gain_db = db;
        self.gain_lin = libm_pow10(db / 20.0);
    }

    pub fn reset(&mut self) {
        for b in self.biquads.iter_mut() {
            b.reset();
        }
        self.limiter = Limiter { ceiling: self.limiter.ceiling, ..Limiter::default() };
    }

    /// In-place, allocation free, no panics. `buf` is interleaved.
    pub fn process_interleaved(&mut self, buf: &mut [f32]) {
        if self.bypass || self.channels == 0 {
            return;
        }
        let channels = self.channels;
        let frames = buf.len() / channels;
        let sr = self.sample_rate;

        for frame in 0..frames {
            let base = frame * channels;

            for ch in 0..channels {
                let mut x = buf[base + ch] * self.gain_lin;
                for b in 0..self.active {
                    x = self.biquads[b].process(x);
                }
                buf[base + ch] = x;
            }

            if channels >= 2 && (self.width - 1.0).abs() > 1e-6 {
                let l = buf[base];
                let r = buf[base + 1];
                let mid = (l + r) * 0.5;
                let side = (l - r) * 0.5 * self.width;
                buf[base] = mid + side;
                buf[base + 1] = mid - side;
            }

            if self.limiter_on {
                for ch in 0..channels {
                    buf[base + ch] = self.limiter.process(buf[base + ch], sr);
                }
            }
        }
    }

    pub fn gain_reduction_db(&self) -> f32 {
        self.limiter.gain_reduction_db()
    }
}

// Small math helpers so the crate can be no_std and dependency free.
#[inline]
fn libm_sin(x: f32) -> f32 {
    x.sin()
}
#[inline]
fn libm_cos(x: f32) -> f32 {
    x.cos()
}
#[inline]
fn libm_sqrt(x: f32) -> f32 {
    x.sqrt()
}
#[inline]
fn libm_exp(x: f32) -> f32 {
    x.exp()
}
#[inline]
fn libm_pow10(x: f32) -> f32 {
    10.0f32.powf(x)
}
#[inline]
fn libm_log10(x: f32) -> f32 {
    x.log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(sr: f32, freq: f32, amp: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|n| amp * libm_sin(2.0 * PI * freq * n as f32 / sr))
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        let sum: f32 = x.iter().map(|v| v * v).sum();
        libm_sqrt(sum / x.len() as f32)
    }

    /// Measured gain of the whole chain at one frequency, in dB.
    fn measured_gain_db(bands: &[Band], freq: f32) -> f32 {
        let sr = 48_000.0;
        let frames = 48_000;
        let mut engine = Engine::new(sr, 1);
        engine.set_bands(bands);
        let input = sine(sr, freq, 0.25, frames);
        let mut buf = input.clone();
        engine.process_interleaved(&mut buf);
        // Skip the settling window so the measurement is steady state.
        let skip = 4_800;
        let out = rms(&buf[skip..]);
        let inp = rms(&input[skip..]);
        20.0 * libm_log10(out / inp)
    }

    #[test]
    fn flat_when_all_bands_are_zero() {
        let bands: Vec<Band> = (0..10).map(|i| Band::peaking(60.0 * 1.9f32.powi(i), 1.0, 0.0)).collect();
        for f in [100.0, 1000.0, 10_000.0] {
            let g = measured_gain_db(&bands, f);
            assert!(g.abs() < 0.05, "expected flat at {f} Hz, measured {g} dB");
        }
    }

    #[test]
    fn peaking_band_hits_its_target() {
        let bands = [Band::peaking(1000.0, 1.4, 6.0)];
        let at_centre = measured_gain_db(&bands, 1000.0);
        let away = measured_gain_db(&bands, 100.0);
        assert!((at_centre - 6.0).abs() < 0.2, "1 kHz measured {at_centre} dB, wanted 6.0");
        assert!(away.abs() < 0.2, "100 Hz measured {away} dB, wanted 0.0");
    }

    #[test]
    fn limiter_keeps_output_under_the_ceiling() {
        let sr = 48_000.0;
        let mut engine = Engine::new(sr, 1);
        engine.limiter_on = true;
        let mut buf = sine(sr, 1000.0, 1.0, 48_000);
        engine.process_interleaved(&mut buf);
        let peak = buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let peak_db = 20.0 * libm_log10(peak);
        assert!(peak_db <= -0.1, "peak {peak_db} dBFS is above the ceiling");
    }

    #[test]
    fn width_zero_collapses_to_mono() {
        let sr = 48_000.0;
        let mut engine = Engine::new(sr, 2);
        engine.width = 0.0;
        let mut buf = Vec::with_capacity(9600);
        for n in 0..4800 {
            let t = n as f32 / sr;
            buf.push(0.4 * libm_sin(2.0 * PI * 440.0 * t));
            buf.push(0.4 * libm_sin(2.0 * PI * 660.0 * t));
        }
        engine.process_interleaved(&mut buf);
        for frame in buf.chunks_exact(2).skip(1000) {
            assert!((frame[0] - frame[1]).abs() < 1e-6, "channels differ with width 0");
        }
    }

    #[test]
    fn bypass_is_bit_exact() {
        let sr = 48_000.0;
        let mut engine = Engine::new(sr, 1);
        engine.set_bands(&[Band::peaking(200.0, 1.0, 12.0)]);
        engine.bypass = true;
        let input = sine(sr, 200.0, 0.5, 4_800);
        let mut buf = input.clone();
        engine.process_interleaved(&mut buf);
        assert_eq!(buf, input);
    }
}
