use embedded_graphics::{fonts, pixelcolor::Bgr565, prelude::*, primitives::Rectangle, style::*};
use entity::*;
use midir::{Ignore, MidiIO, MidiInput, MidiOutput};
use midly::{
    live::LiveEvent,
    num::{u4, u7},
    MidiMessage,
};
use palette::rgb;
use push2_display::Push2Display;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::vec::Vec;
use std::{error, sync::mpsc, thread, time};
use regex::Regex;

mod entity;

fn select_port<T: MidiIO>(midi_io: &T, descr: Regex) -> Result<T::Port, Box<dyn error::Error>> {
    let midi_ports = midi_io.ports();
    for p in midi_ports.iter() {
        if descr.is_match(&midi_io.port_name(p)?) {
            return Ok(p.clone());
        }
    }
    for p in midi_ports.iter() {
        println!("{}", midi_io.port_name(p)?);
    }
    Err(Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        "Push 2 not found",
    )))
}

struct App<'a> {
    entities: BTreeMap<usize, Box<Entity>>,
    fresh_entity_id: usize,
    conn_out: &'a mut midir::MidiOutputConnection,
    display: Push2Display,
    midi_buffer: Vec<u8>,
    tick: f64,
    config: &'a mut AppConfig,
    active_config: u8,
    assigning: bool,
    focused_knobs: BTreeSet<u8>,
}

fn saturate(x: f64) -> f64 {
    1.0 - (-x).exp()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppConfig {
    assignments: BTreeMap<u8, Box<EntityConfig>>,
}

impl<'a> App<'a> {
    fn new(conn_out: &'a mut midir::MidiOutputConnection, config: &'a mut AppConfig) -> Self {
        App {
            entities: BTreeMap::new(),
            conn_out,
            display: Push2Display::new().unwrap(),
            midi_buffer: Vec::new(),
            tick: 0.0,
            config,
            active_config: 0,
            assigning: false,
            fresh_entity_id: 1000,
            focused_knobs: BTreeSet::new(),
        }
    }
    fn send(&mut self, message: MidiMessage) {
        self.midi_buffer.clear();
        let ev = LiveEvent::Midi {
            channel: u4::new(1),
            message,
        };
        ev.write(&mut self.midi_buffer).unwrap();
        self.conn_out.send(&self.midi_buffer[..]).unwrap();
    }
    fn initialise(&mut self) {
        // Activate User Mode
        self.conn_out
            .send(&[SYSEX, &[0x0A, 0x01, 0xF7]].concat())
            .unwrap();

        // Turn on pad LEDs
        // Update upper button array
        for i in 0..8 {
            self.send(MidiMessage::Controller {
                controller: u7::new(20 + i as u8),
                value: u7::new(65 + i),
            })
        }

        // Update lower button array
        for i in 0..8 {
            self.send(MidiMessage::Controller {
                controller: u7::new(102 + i as u8),
                value: u7::new(73 + i),
            })
        }

        // Turn on pad LEDs
        for i in 1..65 {
            self.send(MidiMessage::NoteOn {
                key: u7::new(35 + i as u8),
                vel: u7::new(i as u8),
            })
        }
    }
    fn step(&mut self) {
        let rainbow_velocity = 2.0;

        /*
        // Update button array
        for i in 0..16 {
            let color = palette::Hsv::new(
                palette::RgbHue::from_degrees(i as f64 * 22.5 + self.tick * rainbow_velocity),
                1.0,
                0.5,
            )
            .into();
            self.set_palette(i + 65, color);
        }
        */

        // Update pads
        for i in 0..8 {
            for j in 0..8 {
                let pad_id = i + j * 8;
                let mut accum: rgb::LinSrgb<f64> = rgb::Rgb::new(0.0, 0.0, 0.0);
                if self.assigning {
                    if let Some(cfg) = self.config.assignments.get(&pad_id) {
                        let color: rgb::LinSrgb<f64> =
                            palette::Hsv::new(palette::RgbHue::from_degrees(cfg.hue), 1.0, 0.5)
                                .into();
                        accum += color;
                    }
                } else {
                    for e in self.entities.values() {
                        accum += e.render(self.tick, i, j)
                    }
                }
                let color = rgb::Rgb::new(
                    saturate(accum.red),
                    saturate(accum.green),
                    saturate(accum.blue),
                );
                self.set_palette(1 + pad_id, color);
            }
        }

        for (i, e) in &self.entities.clone() {
            if e.is_dead(self.tick) {
                self.entities.remove(i);
            }
        }

        self.tick += 1.0;
    }

    fn save(&self) -> Result<(), Box<dyn error::Error>> {
        serde_yaml::to_writer(std::fs::File::create("config.yaml")?, self.config)?;
        Ok(())
    }

    fn get_active_config(&mut self) -> Box<EntityConfig> {
        match self.config.assignments.get(&self.active_config) {
            None => {
                let obj = Box::new(EntityConfig {
                    hue: 0.0,
                    kind: 0,
                    duration: 15.0,
                    alpha: 1.0,
                    beta: 0.0,
                    distance: 0,
                });
                self.config
                    .assignments
                    .insert(self.active_config, obj.clone());
                obj
            }
            Some(cfg) => cfg.clone(),
        }
    }

    fn dispatch_knob(&mut self, knob: u8, cw: bool) {
        let mut cfg = self.get_active_config();
        match knob {
            3 if cw => cfg.distance -= 1,
            9 if cw => cfg.distance += 1,
            14 => {
                if cw {
                    cfg.kind = (cfg.kind + 1) % NUM_ANIMATIONS;
                } else if cfg.kind > 0 {
                    cfg.kind = cfg.kind - 1;
                } else {
                    cfg.kind = NUM_ANIMATIONS - 1;
                }
            }
            76 => {
                if cw {
                    cfg.alpha *= 1.01;
                } else {
                    cfg.alpha /= 1.01;
                }
            }
            77 => {
                if cw {
                    cfg.beta += 0.01;
                } else {
                    cfg.beta -= 0.01;
                }
            }
            78 => {
                if cw {
                    cfg.duration *= 1.01;
                } else {
                    cfg.duration /= 1.01;
                }
            }
            79 => {
                if cw {
                    cfg.hue += 1.0;
                } else {
                    cfg.hue -= 1.0;
                }
            }
            _ => println!("Knob {}", knob),
        }
        self.config.assignments.insert(self.active_config, cfg);
    }

    fn handle(&mut self, message: MidiMessage) {
        match message {
            // Knob rotation
            MidiMessage::Controller { controller, value }
                if controller == u7::new(14) ||  controller == u7::new(3) ||  controller == u7::new(9)
                    || controller >= u7::new(72) && controller <= u7::new(79) =>
            {
                self.dispatch_knob(controller.as_int(), value != u7::new(127))
            }
            // Knob touch
            MidiMessage::NoteOn { key, vel } if key >= u7::new(0) && key <= u7::new(10) => {
                if vel == u7::new(127) {
                    self.focused_knobs.insert(key.as_int());
                } else {
                    self.focused_knobs.remove(&key.as_int());
                }
            }

            // Pad activation
            MidiMessage::NoteOn { key, vel: _ } if key >= u7::new(36) && key <= u7::new(99) => {
                let i = key.as_int() - 36;
                let x = i % 8;
                let y = i / 8;

                let prev = self.get_active_config();
                if !self.config.assignments.contains_key(&i) || self.assigning {
                    self.config.assignments.insert(i, prev);
                }
                self.active_config = i;
                let cfg = self.get_active_config();

                let e = Entity::new(&cfg, self.tick, x, y);

                let eid = if e.gated {
                    i as usize
                } else {
                    self.fresh_entity_id += 1;
                    self.fresh_entity_id
                };

                self.entities.insert(eid, Box::new(e));
            }
            MidiMessage::NoteOff { key, vel: _ } if key >= u7::new(36) && key <= u7::new(99) => {
                let i = key.as_int() as usize - 36;
                if let Some(e) = self.entities.get(&i) {
                    let mut obj = e.clone();
                    obj.release(self.tick);
                    self.entities.insert(i, obj);
                }
            }
            // Assign mode
            MidiMessage::Controller { controller, value } if controller == u7::new(86) => {
                self.assigning = value == u7::new(127);
            }
            MidiMessage::Aftertouch { .. } => (), // don't care about aftertouch for now
            _ => println!("{:?}", message),
        }
    }

    fn focus_marker(&self, i: u8) -> &str {
        if self.focused_knobs.contains(&i) {
            "*"
        } else {
            " "
        }
    }

    fn update_display(&mut self) -> Result<(), Box<dyn error::Error>> {
        self.display.clear(Bgr565::BLACK)?;

        Rectangle::new(Point::zero(), self.display.size())
            .into_styled(PrimitiveStyle::with_stroke(Bgr565::WHITE, 1))
            .draw(&mut self.display)?;

        let cfg = self.get_active_config();
        let color: rgb::Srgb<f64> =
            palette::Hsv::new(palette::RgbHue::from_degrees(cfg.hue), 1.0, 0.5).into();
        fonts::Text::new(
            &format!(
                "{} {} {:?}/{:?}\n\
                {} a={:.2}\n\
                {} b={:.2}\n\
                {} d={:.1}f\n\
                ",
                self.focus_marker(10),
                cfg.kind,
                Animation::from_int(cfg.kind), Distance::from_int(cfg.distance),
                self.focus_marker(5),
                cfg.alpha,
                self.focus_marker(6),
                cfg.beta,
                self.focus_marker(7),
                cfg.duration
            ),
            Point::new(16, 16),
        )
        .into_styled(MonoTextStyle::new(
            fonts::Font12x16,
            Bgr565::new(
                (color.red * 31.0).round() as u8,
                (color.green * 63.0).round() as u8,
                (color.blue * 31.0).round() as u8,
            ),
        ))
        .draw(&mut self.display)?;

        self.display.flush()?; // if no frame arrives in 2 seconds, the display is turned black

        Ok(())
    }

    fn set_palette(&mut self, i: u8, color: palette::rgb::Srgb<f64>) {
        let red = (color.red * 255.0).round() as u8;
        let green = (color.green * 255.0).round() as u8;
        let blue = (color.blue * 255.0).round() as u8;
        let white = 0;
        self.conn_out
            .send(
                &[
                    SYSEX,
                    &[
                        0x03,
                        i,
                        red & 0x7f,
                        red >> 7,
                        green & 0x7f,
                        green >> 7,
                        blue & 0x7f,
                        blue >> 7,
                        white & 0x7f,
                        white >> 7,
                        0xf7,
                    ],
                ]
                .concat(),
            )
            .unwrap();
    }
}

pub const SYSEX: &[u8] = &[0xF0, 0x00, 0x21, 0x1D, 0x01, 0x01];

fn main() -> Result<(), Box<dyn error::Error>> {
    let mut config = serde_yaml::from_reader(std::fs::File::open("config.yaml")?)?;

    let mut midi_in = MidiInput::new("midir forwarding input")?;
    midi_in.ignore(Ignore::None);
    let midi_out = MidiOutput::new("midir forwarding output")?;

    let in_port = select_port(&midi_in, Regex::new("User Port$")?)?;
    println!();
    let out_port = select_port(&midi_out, Regex::new("User Port$")?)?;

    let mut conn_out = midi_out.connect(&out_port, "midir-forward")?;

    let mut app = App::new(&mut conn_out, &mut config);
    app.initialise();

    let (tx, rx) = mpsc::channel();

    let _conn_in = midi_in.connect(
        &in_port,
        "midir-forward",
        move |_stamp, raw_message, _| {
            if let Ok(LiveEvent::Midi {
                channel: _,
                message,
            }) = LiveEvent::parse(raw_message)
            {
                tx.send(message).unwrap()
            }
        },
        (),
    )?;

    let mut autosave = 0;
    loop {
        let t0 = std::time::Instant::now();
        for event in rx.try_iter() {
            app.handle(event)
        }
        app.update_display()?;
        app.step();

        autosave += 1;
        if autosave % 30 == 0 {
            app.save()?;
        }

        let dt = t0.elapsed();
        let target = time::Duration::from_millis(1000 / 30);
        if dt < target {
            thread::sleep(target - dt)
        }
    }
}
