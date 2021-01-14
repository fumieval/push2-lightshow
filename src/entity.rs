use palette::rgb;
use serde::{Deserialize, Serialize};
use std::ops::Neg;

#[derive(Debug, Clone, Copy)]
pub enum Animation {
    Ripple,
    Zap,
    VWave,
    Stream,
    DropTheBass,
}

impl Animation {
    pub fn from_int(i: u8) -> Self {
        match i % 5 {
            0 => Animation::Ripple,
            1 => Animation::Zap,
            2 => Animation::VWave,
            3 => Animation::Stream,
            4 => Animation::DropTheBass,
            _ => Animation::Ripple,
        }
    }
    pub fn should_gate(&self) -> bool {
        matches!(
            self,
            Animation::VWave | Animation::Stream | Animation::DropTheBass
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EntityConfig {
    pub kind: u8,
    pub hue: f64,
    pub duration: f64,
    // higher = thicker the shape
    pub alpha: f64,
    pub beta: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct Entity {
    pub kind: Animation,
    pub t0: f64,
    pub t1: f64,
    pub gated: bool,
    pub params: EntityConfig,
    pub x: u8,
    pub y: u8,
    pub color: rgb::LinSrgb<f64>,
}

impl Entity {
    pub fn new(config: &EntityConfig, t: f64, x: u8, y: u8) -> Self {
        let anim = Animation::from_int(config.kind);
        Entity {
            kind: anim,
            t0: t,
            t1: t + config.duration,
            params: *config,
            gated: anim.should_gate(),
            x,
            y,
            color: palette::Hsv::new(config.hue, 1.0, 0.5).into(),
        }
    }

    fn phase(&self, t: f64) -> f64 {
        if self.gated {
            0.0
        } else {
            (t - self.t0) / self.params.duration
        }
    }

    // Bell-shaped function
    fn window(&self, x: f64) -> f64 {
        (2.5 * x / self.params.alpha).powi(2).neg().exp()
    }

    pub fn is_dead(&self, t: f64) -> bool {
        !self.gated && t >= self.t1
    }

    pub fn release(&mut self, t: f64) {
        if self.gated {
            // TODO configurable release time
            self.t1 = t + 5.0;
            self.gated = false;
        }
    }

    fn decay(&self, t: f64) -> f64 {
        // TODO exponential decay
        1.0 - self.phase(t)
    }

    pub fn render(&self, t: f64, x: u8, y: u8) -> rgb::LinSrgb<f64> {
        match &self.kind {
            Animation::Ripple => {
                let r: f64 = ((self.x as i64 - x as i64).pow(2) as f64
                    + (self.y as i64 - y as i64).pow(2) as f64)
                    .sqrt()
                    - self.phase(t) * 12.0;
                self.color * self.window(r)
            }
            Animation::Zap => {
                if y == self.y {
                    self.color * self.window((x as f64 - self.x as f64).abs() - self.phase(t) * 8.0)
                } else if x == self.x {
                    self.color * self.window((y as f64 - self.y as f64).abs() - self.phase(t) * 8.0)
                } else {
                    rgb::Rgb::new(0.0, 0.0, 0.0)
                }
            }
            Animation::VWave => {
                let theta = std::f64::consts::PI * t * self.params.beta;
                let phase = std::f64::consts::PI * (x as f64 - self.x as f64) / 4.0;
                let amp = (theta + phase).sin() * 4.0;
                self.color * self.window(amp - (y as f64 - self.y as f64)) * self.decay(t)
            }
            Animation::Stream => {
                let intensity = (t * self.params.alpha
                    + (x - self.x).pow(2) as f64 / 2.0
                    + (y - self.y).pow(2) as f64 / 2.0)
                    .sin()
                    * 0.5
                    + 0.5;
                if y == self.y || x == self.x {
                    self.color * intensity * self.decay(t)
                } else {
                    rgb::Rgb::new(0.0, 0.0, 0.0)
                }
            }
            Animation::DropTheBass => {
                let intensity = self.decay(t);
                // O
                if [
                    (0, 1),
                    (0, 2),
                    (1, 0),
                    (2, 0),
                    (3, 1),
                    (3, 2),
                    (1, 3),
                    (2, 3),
                ]
                .contains(&(x, y))
                {
                    rgb::Rgb::new(1.0, 0.0, 1.0) * intensity
                } else if [
                    (4, 0),
                    (4, 1),
                    (4, 2),
                    (4, 3),
                    (5, 1),
                    (5, 3),
                    (6, 1),
                    (6, 3),
                    (7, 2),
                ]
                .contains(&(x, y))
                {
                    rgb::Rgb::new(1.0, 0.0, 0.0) * intensity
                } else if [
                    (0, 4),
                    (0, 5),
                    (0, 6),
                    (0, 7),
                    (1, 4),
                    (1, 7),
                    (2, 4),
                    (2, 7),
                    (3, 5),
                    (3, 6),
                ]
                .contains(&(x, y))
                {
                    rgb::Rgb::new(0.0, 0.0, 1.0) * intensity
                } else if [
                    (4, 4),
                    (4, 5),
                    (4, 6),
                    (4, 7),
                    (5, 5),
                    (5, 7),
                    (6, 5),
                    (6, 7),
                    (7, 4),
                    (7, 6),
                ]
                .contains(&(x, y))
                {
                    rgb::Rgb::new(0.0, 1.0, 1.0) * intensity
                } else {
                    rgb::Rgb::new(0.0, 0.0, 0.0)
                }
            }
        }
    }
}
