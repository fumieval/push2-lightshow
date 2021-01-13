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
use std::collections::BTreeMap;
use std::vec::Vec;
use std::{error, sync::mpsc, thread, time};

mod entity;

fn select_port<T: MidiIO>(midi_io: &T, descr: &str) -> Result<T::Port, Box<dyn error::Error>> {
    let midi_ports = midi_io.ports();
    for p in midi_ports.iter() {
        if midi_io.port_name(p)? == descr {
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
            conn_out: conn_out,
            display: Push2Display::new().unwrap(),
            midi_buffer: Vec::new(),
            tick: 0.0,
            config: config,
            active_config: 0,
            assigning: false,
            fresh_entity_id: 1000,
        }
    }
    fn send(&mut self, message: MidiMessage) {
        self.midi_buffer.clear();
        let ev = LiveEvent::Midi {
            channel: u4::new(1),
            message: message,
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

        // hue knob (top right)
        self.send(MidiMessage::Controller {
            controller: u7::new(85),
            value: u7::new(127),
        })
    }
    fn step(&mut self) {
        let rainbow_velocity = 2.0;

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

        // Update pads
        for i in 0..8 {
            for j in 0..8 {
                let pad_id = i + j * 8;
                let mut accum: rgb::LinSrgb<f64> = rgb::Rgb::new(0.0, 0.0, 0.0);
                for (_, e) in &self.entities {
                    accum += e.render(self.tick, i, j)
                }
                if self.assigning {
                    if let Some(cfg) = self.config.assignments.get(&pad_id) {
                        let color: rgb::LinSrgb<f64> =
                            palette::Hsv::new(palette::RgbHue::from_degrees(cfg.hue), 1.0, 0.5)
                                .into();
                        accum += color;
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
            if !e.kind.should_gate() && self.tick > e.t1 {
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
                });
                self.config
                    .assignments
                    .insert(self.active_config, obj.clone());
                obj
            }
            Some(cfg) => cfg.clone(),
        }
    }

    fn handle(&mut self, message: MidiMessage) {
        match message {
            // Hue knob (top right)
            MidiMessage::Controller { controller, value } if controller == u7::new(79) => {
                let mut cfg = self.get_active_config();
                if value == u7::new(127) {
                    cfg.hue += 1.0;
                } else {
                    cfg.hue -= 1.0;
                }
                self.set_palette(127, palette::Hsv::new(cfg.hue, 1.0, 0.5).into());
                self.config.assignments.insert(self.active_config, cfg);
            }
            // Jog dial (top left)
            MidiMessage::Controller { controller, value } if controller == u7::new(14) => {
                let mut cfg = self.get_active_config();
                if value == u7::new(127) {
                    cfg.kind += 1;
                } else {
                    cfg.kind -= 1;
                }
                self.config.assignments.insert(self.active_config, cfg);
            }
            // Duration knob (8th)
            MidiMessage::Controller { controller, value } if controller == u7::new(78) => {
                let mut cfg = self.get_active_config();
                if value == u7::new(127) {
                    cfg.duration /= 1.03;
                } else {
                    cfg.duration *= 1.03;
                }
                self.config.assignments.insert(self.active_config, cfg);
            }
            // Assign
            MidiMessage::Controller { controller, value } if controller == u7::new(86) => {
                self.assigning = value == u7::new(127);
            }
            MidiMessage::NoteOn { key, vel: _ } if key >= u7::new(36) && key <= u7::new(99) => {
                let i = key.as_int() - 36;
                let x = i % 8;
                let y = i / 8;

                let prev = self.get_active_config();
                if !self.config.assignments.contains_key(&i) || self.assigning {
                    self.config.assignments.insert(i, prev.clone());
                }
                self.active_config = i;
                let cfg = self.get_active_config();

                let eid = if Animation::from_int(cfg.kind).should_gate() {
                    i as usize
                } else {
                    self.fresh_entity_id += 1;
                    self.fresh_entity_id
                };

                self.entities
                    .insert(eid, Box::new(Entity::new(&cfg, self.tick, x, y)));
            }
            MidiMessage::NoteOff { key, vel: _ } if key >= u7::new(36) && key <= u7::new(99) => {
                self.entities.remove(&(key.as_int() as usize - 36));
            }
            _ => println!("{:?}", message),
        }
    }

    fn update_display(&mut self) -> Result<(), Box<dyn error::Error>> {
        self.display.clear(Bgr565::BLACK)?;

        Rectangle::new(Point::zero(), self.display.size())
            .into_styled(PrimitiveStyle::with_stroke(Bgr565::WHITE, 1))
            .draw(&mut self.display)?;

        let cfg = self.get_active_config();
        fonts::Text::new(
            &format!("{:?}/{}f", Animation::from_int(cfg.kind), cfg.duration),
            Point::new(0, 70),
        )
        .into_styled(MonoTextStyle::new(fonts::Font12x16, Bgr565::WHITE))
        .draw(&mut self.display)?;

        self.display.flush()?; // if no frame arrives in 2 seconds, the display is turned black

        Ok(())
    }

    fn set_palette(&mut self, i: u8, color: palette::rgb::Srgb<f64>) {
        let r = (color.red * 255.0).round() as u8;
        let g = (color.green * 255.0).round() as u8;
        let b = (color.blue * 255.0).round() as u8;
        let w = 0;
        self.conn_out
            .send(
                &[
                    SYSEX,
                    &[
                        0x03,
                        i,
                        r & 0x7f,
                        r >> 7,
                        g & 0x7f,
                        g >> 7,
                        b & 0x7f,
                        b >> 7,
                        w & 0x7f,
                        w >> 7,
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

    let in_port = select_port(&midi_in, "User Port")?;
    println!();
    let out_port = select_port(&midi_out, "User Port")?;

    let mut conn_out = midi_out.connect(&out_port, "midir-forward")?;

    let mut app = App::new(&mut conn_out, &mut config);
    app.initialise();

    let (tx, rx) = mpsc::channel();

    let _conn_in = midi_in.connect(
        &in_port,
        "midir-forward",
        move |_stamp, raw_message, _| {
            if let Ok(event) = LiveEvent::parse(raw_message) {
                match event {
                    LiveEvent::Midi {
                        channel: _,
                        message,
                    } => tx.send(message).unwrap(),
                    _ => (),
                }
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
