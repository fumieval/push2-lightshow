use palette::rgb;
use serde::{Deserialize, Serialize};
use std::ops::Neg;

#[derive(Debug, Clone, Copy)]
pub enum Animation {
    Ripple,
    Cross,
    VWave,
}

impl Animation {
    pub fn from_int(i: u8) -> Self {
        match i % 3 {
            0 => Animation::Ripple,
            1 => Animation::Cross,
            2 => Animation::VWave,
            _ => Animation::Ripple,
        }
    }
    pub fn should_gate(&self) -> bool {
        match self {
            Animation::VWave => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EntityConfig {
    pub kind: u8,
    pub hue: f64,
    pub duration: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct Entity {
    pub kind: Animation,
    pub t0: f64,
    pub t1: f64,
    pub x: u8,
    pub y: u8,
    pub color: rgb::LinSrgb<f64>,
}

// Bell-shaped function
// k
fn window(x: f64) -> f64 {
    (x * 2.5).powi(2).neg().exp()
}

impl Entity {
    pub fn new(config: &EntityConfig, t: f64, x: u8, y: u8) -> Self {
        let anim = Animation::from_int(config.kind);
        Entity {
            kind: anim,
            t0: t,
            t1: t + config.duration,
            x,
            y,
            color: palette::Hsv::new(config.hue, 1.0, 0.5).into(),
        }
    }
    pub fn phase(&self, t: f64) -> f64 {
        (t - self.t0) as f64 / (self.t1 - self.t0) as f64
    }
    pub fn render(&self, t: f64, x: u8, y: u8) -> rgb::LinSrgb<f64> {
        match &self.kind {
            Animation::Ripple => {
                let r: f64 = ((self.x as i64 - x as i64).pow(2) as f64
                    + (self.y as i64 - y as i64).pow(2) as f64)
                    .sqrt()
                    - self.phase(t) * 12.0;
                self.color * window(r)
            }
            Animation::Cross => {
                if y == self.y {
                    self.color * window((x as f64 - self.x as f64).abs() - self.phase(t) * 8.0)
                } else if x == self.x {
                    self.color * window((y as f64 - self.y as f64).abs() - self.phase(t) * 8.0)
                } else {
                    rgb::Rgb::new(0.0, 0.0, 0.0)
                }
            }
            Animation::VWave => {
                let theta = 2.0 * std::f64::consts::PI * (self.phase(t) * 2.0 + y as f64 / 8.0);
                self.color * window((theta.sin() * 4.0 - x as f64 + 4.0) * 0.5)
            }
        }
    }
}
