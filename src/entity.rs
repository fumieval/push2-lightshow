use palette::rgb;
use serde::{Deserialize, Serialize};
use std::ops::Neg;
use std::f64::consts::PI;

#[derive(Debug, Clone, Copy)]
pub enum Distance {
    Euclidean,
    Chebyshev,
    Manhattan,
    Rook,
    Bishop,
}

impl Distance {
    pub fn from_int(i: u8) -> Self {
        match i % 6 {
            0 => Distance::Euclidean,
            1 => Distance::Chebyshev,
            2 => Distance::Manhattan,
            3 => Distance::Rook,
            4 => Distance::Bishop,
            _ => Distance::Euclidean,
        }
    }
    pub fn eval(&self, x0: u8, y0: u8, x1: u8, y1: u8) -> f64 {
        match self {
            Distance::Euclidean => ((x1 as f64 - x0 as f64).powi(2) + (y1 as f64 - y0 as f64).powi(2)).sqrt(),
            Distance::Chebyshev => (x1 as f64 - x0 as f64).abs().max((y1 as f64 - y0 as f64).abs()),
            Distance::Manhattan => (x1 as f64 - x0 as f64).abs() + (y1 as f64 - y0 as f64).abs(),
            Distance::Rook => if x0 == x1
            {
                (y1 as f64 - y0 as f64).abs()
            } else if y0 == y1 {
                (x1 as f64 - x0 as f64).abs()
            } else {
                1024.0
            }
            Distance::Bishop => if (x1 as i64 - x0 as i64).abs() == (y1 as i64 - y0 as i64).abs()
            {
                (y1 as f64 - y0 as f64).abs()
            } else {
                1024.0
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Animation {
    Linear,
    VWave,
    Stream,
    DropTheBass,
}

pub const NUM_ANIMATIONS : u8 = 5;

impl Animation {
    pub fn from_int(i: u8) -> Self {
        match i % 5 {
            0 => Animation::Linear,
            1 => Animation::VWave,
            2 => Animation::Stream,
            3 => Animation::DropTheBass,
            _ => Animation::Linear,
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
    // Envelope function ID
    pub kind: u8,
    // Base hue
    pub hue: f64,
    // time constant
    pub duration: f64,
    // Factor for the window function. Higher = thicker the shape
    pub alpha: f64,
    // Multiplier of the distance function
    pub beta: f64,
    // Distance function ID
    pub distance: u8,
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
    pub distance: Distance,
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
            distance: Distance::from_int(config.distance),
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
        let distance: f64 = self.distance.eval(self.x, self.y, x, y);
        match &self.kind {
            Animation::Linear => {
                // let theta = (y as f64 - self.y as f64).atan2(x as f64 - self.x as f64);
                // let modulation = (2.0 * PI * (theta / 2.0 + t / 60.0)).sin();
                self.color * self.window(distance - self.phase(t) * 12.0)
            }
            Animation::VWave => {
                let theta = PI * t * self.params.beta;
                let phase = PI * (x as f64 - self.x as f64) / 4.0;
                let amp = (theta + phase).sin() * 4.0;
                self.color * self.window(amp - (y as f64 - self.y as f64)) * self.decay(t)
            }
            Animation::Stream => {
                let amp = (t / self.params.duration - distance * self.params.beta).sin();
                if distance < 12.0 {
                    self.color * self.window(amp) * self.decay(t)
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
