//! On-device personalisation: Thompson sampling over Beta posteriors, one per message style and
//! one per send-time slot, with the network's output as the prior.
//!
//! A success is a reminder the user opened (or followed by a dictation) within 24 hours; an ignored
//! or dismissed reminder is a failure.

use serde::{Deserialize, Serialize};

use crate::messages::K_STYLES;
use crate::rng::Rng;

/// Send-time slots, local hours: 09-12, 12-15, 15-18, 18-21 (the allowed daytime window).
pub const N_SLOTS: usize = 4;
pub const SLOT_START_HOUR: f32 = 9.0;
pub const SLOT_HOURS: f32 = 3.0;

/// How many pseudo observations the network's style preference is worth.
pub const STYLE_PRIOR_STRENGTH: f64 = 4.0;
/// The slot prior: a 30% open rate worth 10 pseudo observations.
pub const SLOT_PRIOR_MEAN: f64 = 0.3;
pub const SLOT_PRIOR_STRENGTH: f64 = 10.0;
/// How strongly the slot evidence moves the network's send log-odds.
pub const SLOT_WEIGHT: f64 = 0.5;
/// Outcomes a slot needs before its evidence counts.
pub const SLOT_MIN_TRIALS: f32 = 3.0;

pub fn slot_of(hour: f32) -> Option<usize> {
    let s = ((hour - SLOT_START_HOUR) / SLOT_HOURS).floor();
    (s >= 0.0 && (s as usize) < N_SLOTS).then_some(s as usize)
}

/// Successes and failures per arm.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bandit {
    pub style: [[f32; 2]; K_STYLES],
    pub slot: [[f32; 2]; N_SLOTS],
}

fn logit(p: f64) -> f64 {
    let p = p.clamp(1e-4, 1.0 - 1e-4);
    (p / (1.0 - p)).ln()
}

/// The digamma function (recurrence up to 6, then the asymptotic series), for x > 0.
pub fn digamma(mut x: f64) -> f64 {
    let mut r = 0.0;
    while x < 6.0 {
        r -= 1.0 / x;
        x += 1.0;
    }
    let f = 1.0 / (x * x);
    r + x.ln() - 0.5 / x - f * (1.0 / 12.0 - f * (1.0 / 120.0 - f * (1.0 / 252.0 - f / 240.0)))
}

/// Prior mean open rate for a style from the network's softmax share.
pub fn style_prior_mean(share: f32) -> f64 {
    (0.2 + 0.15 * share as f64).clamp(0.02, 0.98)
}

impl Bandit {
    pub fn update(&mut self, style: usize, slot: Option<usize>, success: bool) {
        let k = if success { 0 } else { 1 };
        if let Some(s) = self.style.get_mut(style) {
            s[k] += 1.0;
        }
        if let Some(s) = slot.and_then(|s| self.slot.get_mut(s)) {
            s[k] += 1.0;
        }
    }

    pub fn trials(&self) -> f32 {
        self.style.iter().map(|s| s[0] + s[1]).sum()
    }

    fn style_params(&self, s: usize, share: f32) -> (f64, f64) {
        let m = style_prior_mean(share);
        let [win, loss] = self.style[s];
        (STYLE_PRIOR_STRENGTH * m + win as f64, STYLE_PRIOR_STRENGTH * (1.0 - m) + loss as f64)
    }

    /// Posterior mean open rate of a style.
    pub fn style_mean(&self, s: usize, share: f32) -> f64 {
        let (a, b) = self.style_params(s, share);
        a / (a + b)
    }

    /// Thompson sampling: draw an open rate for each allowed style from its posterior and take the
    /// best draw.
    pub fn pick_style(&self, shares: &[f32; K_STYLES], allowed: &[bool; K_STYLES], rng: &mut Rng) -> usize {
        let mut best = (f64::MIN, 0);
        for s in 0..K_STYLES {
            if !allowed[s] {
                continue;
            }
            let (a, b) = self.style_params(s, shares[s]);
            let draw = rng.beta(a, b);
            if draw > best.0 {
                best = (draw, s);
            }
        }
        best.1
    }

    /// How much this slot's evidence moves the send log-odds: a sample of the slot's open rate
    /// (prior centred on the user's pooled open rate over all slots) against that pooled rate.
    /// Zero until the slot has outcomes of its own: the network's timing is trusted until there is
    /// evidence against it.
    pub fn slot_shift(&self, slot: usize, rng: &mut Rng) -> f64 {
        if self.slot[slot][0] + self.slot[slot][1] < SLOT_MIN_TRIALS {
            return 0.0;
        }
        let (win, loss): (f32, f32) = self.slot.iter().fold((0.0, 0.0), |(w, l), s| (w + s[0], l + s[1]));
        let pooled = (SLOT_PRIOR_STRENGTH * SLOT_PRIOR_MEAN + win as f64) / (SLOT_PRIOR_STRENGTH + (win + loss) as f64);
        let (a0, b0) = (SLOT_PRIOR_STRENGTH * pooled, SLOT_PRIOR_STRENGTH * (1.0 - pooled));
        let [w, l] = self.slot[slot];
        let draw = rng.beta(a0 + w as f64, b0 + l as f64);
        // Centre: E[logit(x)] (digamma(a) - digamma(b) for x ~ Beta(a, b)) for a slot with the
        // same number of outcomes at exactly the pooled rate, so the shift is zero on average
        // unless this slot really differs.
        let n = (w + l) as f64;
        let centre = digamma(a0 + n * pooled) - digamma(b0 + n * (1.0 - pooled));
        SLOT_WEIGHT * (logit(draw) - centre)
    }
}

/// The network's send probability adjusted by the slot evidence.
pub fn send_probability(p_send: f32, shift: f64) -> f64 {
    1.0 / (1.0 + (-(logit(p_send as f64) + shift)).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digamma_matches_known_values() {
        assert!((digamma(1.0) + 0.577_215_664_9).abs() < 1e-9);
        assert!((digamma(0.5) + 1.963_510_026).abs() < 1e-8);
        assert!((digamma(7.0) - 1.872_784_335).abs() < 1e-8);
    }

    #[test]
    fn slots_cover_the_daytime_window() {
        assert_eq!(slot_of(8.99), None);
        assert_eq!(slot_of(9.0), Some(0));
        assert_eq!(slot_of(14.5), Some(1));
        assert_eq!(slot_of(20.99), Some(3));
        assert_eq!(slot_of(21.0), None);
    }

    #[test]
    fn evidence_moves_the_style_posterior() {
        let mut b = Bandit::default();
        let shares = [0.7, 0.1, 0.1, 0.1];
        assert!(b.style_mean(0, shares[0]) > b.style_mean(2, shares[2]), "prior follows the network");
        for _ in 0..10 {
            b.update(0, Some(1), false);
            b.update(2, Some(1), true);
        }
        assert!(b.style_mean(2, shares[2]) > b.style_mean(0, shares[0]), "data overrides the prior");
        let mut rng = Rng::new(3);
        let picks = (0..200).filter(|_| b.pick_style(&shares, &[true; K_STYLES], &mut rng) == 2).count();
        assert!(picks > 150, "picked the winning style {picks} of 200 times");
        assert_eq!(b.trials(), 20.0);
    }

    #[test]
    fn disallowed_styles_are_never_picked() {
        let b = Bandit::default();
        let mut rng = Rng::new(4);
        for _ in 0..100 {
            assert_ne!(b.pick_style(&[0.1, 0.9, 0.0, 0.0], &[true, false, true, true], &mut rng), 1);
        }
    }

    #[test]
    fn slot_evidence_shifts_the_send_decision() {
        let mut b = Bandit::default();
        let mut rng = Rng::new(5);
        let mean = |b: &Bandit, s: usize, rng: &mut Rng| (0..4000).map(|_| b.slot_shift(s, rng)).sum::<f64>() / 4000.0;
        assert_eq!(b.slot_shift(2, &mut rng), 0.0, "no data, no shift");
        // Every slot at the same 1 in 4 rate: no shift on average.
        for s in 0..N_SLOTS {
            b.update(0, Some(s), true);
            for _ in 0..3 {
                b.update(0, Some(s), false);
            }
        }
        for s in 0..N_SLOTS {
            assert!(mean(&b, s, &mut rng).abs() < 0.03, "slot {s}: {}", mean(&b, s, &mut rng));
        }
        let mut b = Bandit::default();
        for _ in 0..8 {
            b.update(0, Some(3), true);
            b.update(0, Some(0), false);
        }
        assert!(mean(&b, 3, &mut rng) > 0.2);
        assert!(mean(&b, 0, &mut rng) < -0.2);
        assert!(send_probability(0.5, 0.4) > 0.5 && send_probability(0.5, -0.4) < 0.5);
    }

    #[test]
    fn survives_json() {
        let mut b = Bandit::default();
        b.update(1, Some(2), true);
        let back: Bandit = serde_json::from_str(&serde_json::to_string(&b).unwrap()).unwrap();
        assert_eq!(back, b);
    }
}
