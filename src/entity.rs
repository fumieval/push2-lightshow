use palette::rgb;
use std::ops::Neg;

#[derive(Debug, Clone, Copy)]
pub enum Animation {
    Ripple,
}

#[derive(Debug, Clone, Copy)]
pub struct Entity {
    pub kind: Animation,
    pub t0: f64,
    pub t1: f64,
    pub x: f64,
    pub y: f64,
    pub color: rgb::LinSrgb<f64>,
}

impl Entity {
    pub fn render(&self, t: f64, x: f64, y: f64) -> rgb::LinSrgb<f64> {
        match &self.kind {
            Animation::Ripple => {
                let phase = (t - self.t0) / (self.t1 - self.t0);
                let r = ((self.x - x).powi(2) + (self.y - y).powi(2)).sqrt() - phase * 12.0;
                let k = (r * 2.0).powi(2).neg().exp();
                self.color * k
            }
        }
    }
}
