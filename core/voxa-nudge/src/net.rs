//! The tiny network. A fixed maximum topology (inputs to hidden, hidden to outputs, and inputs
//! straight to outputs) where every connection has an on/off mask, so evolution can add and remove
//! connections and hidden nodes NEAT style while genomes stay aligned for crossover.
//!
//! Outputs: P(send now) through a sigmoid, and a softmax over the message styles.

use serde::{Deserialize, Serialize};

use crate::features::{INPUT_NAMES, N_IN};
use crate::messages::{K_STYLES, Style};

pub const N_HID: usize = 6;
pub const N_OUT: usize = 1 + K_STYLES;

const IH: usize = N_HID * N_IN;
const HO: usize = N_OUT * N_HID;
const IO: usize = N_OUT * N_IN;
/// Connections in the full topology (the most parameters a network can have).
pub const N_GENES: usize = IH + HO + IO;

/// Where a connection goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conn {
    InHid { i: usize, h: usize },
    HidOut { h: usize, o: usize },
    InOut { i: usize, o: usize },
}

pub fn conn(g: usize) -> Conn {
    if g < IH {
        Conn::InHid { i: g % N_IN, h: g / N_IN }
    } else if g < IH + HO {
        let g = g - IH;
        Conn::HidOut { h: g % N_HID, o: g / N_HID }
    } else {
        let g = g - IH - HO;
        Conn::InOut { i: g % N_IN, o: g / N_IN }
    }
}

pub fn gene_in_hid(i: usize, h: usize) -> usize {
    h * N_IN + i
}
pub fn gene_hid_out(h: usize, o: usize) -> usize {
    IH + o * N_HID + h
}
pub fn gene_in_out(i: usize, o: usize) -> usize {
    IH + HO + o * N_IN + i
}

#[derive(Clone, Debug, PartialEq)]
pub struct Net {
    pub w: Vec<f32>,
    pub on: Vec<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Output {
    pub p_send: f32,
    /// Softmax over [`Style::ALL`].
    pub style: [f32; K_STYLES],
}

impl Output {
    pub fn best_style(&self) -> Style {
        let mut best = 0;
        for s in 1..K_STYLES {
            if self.style[s] > self.style[best] {
                best = s;
            }
        }
        Style::from_index(best)
    }
}

impl Default for Net {
    /// No connections: P(send) is exactly one half, which is not above the threshold, so it never
    /// sends; styles are uniform.
    fn default() -> Self {
        Self { w: vec![0.0; N_GENES], on: vec![false; N_GENES] }
    }
}

/// The send threshold: send when P(send now) is above one half.
pub const SEND_THRESHOLD: f32 = 0.5;

impl Output {
    pub fn wants_to_send(&self) -> bool {
        self.p_send > SEND_THRESHOLD
    }
}

impl Net {
    /// Enabled connections: the parameter count.
    pub fn params(&self) -> usize {
        self.on.iter().filter(|&&b| b).count()
    }

    /// Hidden nodes that take part (at least one connection in and one out).
    pub fn active_hidden(&self) -> usize {
        (0..N_HID)
            .filter(|&h| {
                (0..N_IN).any(|i| self.on[gene_in_hid(i, h)]) && (0..N_OUT).any(|o| self.on[gene_hid_out(h, o)])
            })
            .count()
    }

    pub fn forward(&self, x: &[f32; N_IN]) -> Output {
        let mut hid = [0.0f32; N_HID];
        for (h, v) in hid.iter_mut().enumerate() {
            let mut s = 0.0;
            for i in 0..N_IN {
                let g = gene_in_hid(i, h);
                if self.on[g] {
                    s += self.w[g] * x[i];
                }
            }
            *v = s.tanh();
        }
        let mut out = [0.0f32; N_OUT];
        for (o, v) in out.iter_mut().enumerate() {
            let mut s = 0.0;
            for (h, hv) in hid.iter().enumerate() {
                let g = gene_hid_out(h, o);
                if self.on[g] {
                    s += self.w[g] * hv;
                }
            }
            for i in 0..N_IN {
                let g = gene_in_out(i, o);
                if self.on[g] {
                    s += self.w[g] * x[i];
                }
            }
            *v = s;
        }
        let p_send = 1.0 / (1.0 + (-out[0]).exp());
        let m = out[1..].iter().cloned().fold(f32::MIN, f32::max);
        let mut style = [0.0f32; K_STYLES];
        let mut z = 0.0;
        for s in 0..K_STYLES {
            style[s] = (out[1 + s] - m).exp();
            z += style[s];
        }
        for v in &mut style {
            *v /= z;
        }
        Output { p_send, style }
    }

    /// What the JSON file keeps: weights rounded to 4 decimals, disabled ones dropped (zeroed).
    pub fn rounded(&self) -> Net {
        let w = self.w.iter().zip(&self.on).map(|(w, &on)| if on { (w * 1e4).round() / 1e4 } else { 0.0 });
        Net { w: w.collect(), on: self.on.clone() }
    }

    pub fn to_file(&self, note: ModelNote) -> ModelFile {
        let r = self.rounded();
        ModelFile {
            format: "voxa-nudge/1".into(),
            inputs: INPUT_NAMES.iter().map(|s| s.to_string()).collect(),
            hidden: N_HID,
            outputs: std::iter::once("send".to_string()).chain(Style::ALL.iter().map(|s| s.name().to_string())).collect(),
            params: r.params(),
            note,
            conns: (0..N_GENES).filter(|&g| r.on[g]).map(|g| (g as u16, r.w[g])).collect(),
        }
    }

    pub fn from_file(f: &ModelFile) -> Result<Net, String> {
        if f.format != "voxa-nudge/1" || f.inputs.len() != N_IN || f.hidden != N_HID || f.outputs.len() != N_OUT {
            return Err(format!("model shape does not match this build ({})", f.format));
        }
        let mut n = Net::default();
        for &(g, w) in &f.conns {
            let g = g as usize;
            if g >= N_GENES || !w.is_finite() {
                return Err(format!("bad connection {g}"));
            }
            n.on[g] = true;
            n.w[g] = w;
        }
        Ok(n)
    }

    pub fn to_json(&self, note: ModelNote) -> String {
        serde_json::to_string(&self.to_file(note)).unwrap_or_default()
    }

    pub fn from_json(json: &str) -> Result<Net, String> {
        let f: ModelFile = serde_json::from_str(json).map_err(|e| e.to_string())?;
        Net::from_file(&f)
    }
}

/// The model on disk: only the enabled connections, as (gene index, weight).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelFile {
    pub format: String,
    pub inputs: Vec<String>,
    pub hidden: usize,
    pub outputs: Vec<String>,
    pub params: usize,
    pub note: ModelNote,
    pub conns: Vec<(u16, f32)>,
}

/// How the model was made, for whoever opens the file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelNote {
    pub seed: u64,
    pub generations: usize,
    pub population: usize,
    pub params_before_pruning: usize,
    pub validation_fitness: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    fn random(seed: u64) -> Net {
        let mut r = Rng::new(seed);
        Net {
            w: (0..N_GENES).map(|_| r.normal() as f32).collect(),
            on: (0..N_GENES).map(|_| r.chance(0.5)).collect(),
        }
    }

    #[test]
    fn gene_layout_round_trips() {
        let mut seen = vec![false; N_GENES];
        for g in 0..N_GENES {
            let back = match conn(g) {
                Conn::InHid { i, h } => gene_in_hid(i, h),
                Conn::HidOut { h, o } => gene_hid_out(h, o),
                Conn::InOut { i, o } => gene_in_out(i, o),
            };
            assert_eq!(back, g);
            seen[g] = true;
        }
        assert!(seen.iter().all(|&s| s));
        assert!((50..=200).contains(&N_GENES), "{N_GENES} genes");
    }

    #[test]
    fn outputs_are_probabilities() {
        let n = random(1);
        let x = [0.3; N_IN];
        let o = n.forward(&x);
        assert!((0.0..=1.0).contains(&o.p_send));
        assert!((o.style.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        let empty = Net::default().forward(&x);
        assert!(!empty.wants_to_send(), "no connections never sends");
        assert!(empty.style.iter().all(|&p| (p - 0.25).abs() < 1e-6));
    }

    #[test]
    fn disabled_connections_do_not_matter() {
        let mut n = random(2);
        let x = [0.7; N_IN];
        let before = n.forward(&x);
        for g in 0..N_GENES {
            if !n.on[g] {
                n.w[g] = 1e6;
            }
        }
        assert_eq!(n.forward(&x), before);
    }

    #[test]
    fn json_is_small_and_round_trips() {
        let n = random(3);
        let json = n.to_json(ModelNote::default());
        assert!(json.len() < 5 * 1024, "{} bytes", json.len());
        let back = Net::from_json(&json).unwrap();
        assert_eq!(back, n.rounded());
        assert!(Net::from_json("{\"format\":\"other\"}").is_err());
    }
}
