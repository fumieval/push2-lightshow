use embedded_graphics::{fonts, pixelcolor::Bgr565, prelude::*, primitives::Rectangle, style::*};
use midir::{Ignore, MidiIO, MidiInput, MidiOutput};
use midly::{
    live::LiveEvent,
    num::{u4, u7},
    MidiMessage,
};
use palette::rgb;
use push2_display::Push2Display;
use std::vec::Vec;
use std::{error, sync::mpsc, thread, time};
use entity::*;

mod entity;

fn select_port<T: MidiIO>(midi_io: &T, descr: &str) -> Result<T::Port, Box<dyn error::Error>> {
    let midi_ports = midi_io.ports();
    for p in midi_ports.iter() {
        println!("{}", midi_io.port_name(p)?);
        if midi_io.port_name(p)? == descr {
            return Ok(p.clone());
        }
    }
    Err(Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        "Push 2 not found",
    )))
}

struct App<'a> {
    entities: std::vec::Vec<Box<Entity>>,
    conn_out: &'a mut midir::MidiOutputConnection,
    display: Push2Display,
    logo_position: Point,
    step: i32,
    midi_buffer: Vec<u8>,
    tick: f64,
    hue: f64,
}

fn saturate(x: f64) -> f64 {
    1.0 - (-x).exp()
}

impl<'a> App<'a> {
    fn new(conn_out: &'a mut midir::MidiOutputConnection) -> Self {
        App {
            entities: std::vec::Vec::new(),
            conn_out: conn_out,
            display: Push2Display::new().unwrap(),
            logo_position: Point::new(0, 70),
            step: 1,
            midi_buffer: Vec::new(),
            tick: 0.0,
            hue: 0.0,
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

        self.send(MidiMessage::Controller {
            controller: u7::new(85),
            value: u7::new(127),
        })
    }
    fn step(&mut self) {
        self.tick += 1.0;
        let rainbow_velocity = 2.0;

        // Update upper button array
        for i in 0..16 {
            let color = palette::Hsv::new(
                palette::RgbHue::from_degrees(i as f64 * 22.5 + self.tick * rainbow_velocity),
                1.0,
                0.5,
            )
            .into();
            self.set_palette(i + 65, color);
        }

        // Update padsf
        for i in 0..8 {
            for j in 0..8 {
                let mut accum = rgb::Rgb::new(0.0, 0.0, 0.0);
                for e in &self.entities {
                    accum += e.render(self.tick, i as f64, j as f64)
                }
                let color = rgb::Rgb::new(
                    saturate(accum.red),
                    saturate(accum.green),
                    saturate(accum.blue),
                );
                self.set_palette(1 + i + j * 8, color);
            }
        }

        let mut next_entities: Vec<Box<Entity>> = Vec::new();
        for e in &self.entities {
            if self.tick < e.t1 {
                next_entities.push(e.clone())
            }
        }
        self.entities = next_entities;
    }
    fn handle(&mut self, message: MidiMessage) {
        match message {
            MidiMessage::NoteOff { key: _, vel: _ } => (),
            MidiMessage::Controller { controller, value } if controller == u7::new(79) => {
                if value == u7::new(127) {
                    self.hue += 1.0;
                } else {
                    self.hue -= 1.0;
                }
                self.set_palette(127, palette::Hsv::new(self.hue, 1.0, 0.5).into());
            }
            MidiMessage::NoteOn { key, vel } if key >= u7::new(36) && key <= u7::new(99) => {
                let i = key.as_int() - 36;
                let x = i % 8;
                let y = i / 8;
                let e = Entity {
                    x: x as f64,
                    y: y as f64,
                    t0: self.tick,
                    t1: self.tick + 15.0,
                    kind: Animation::Ripple,
                    color: palette::Hsv::new(self.hue, 1.0, vel.as_int() as f64 / 255.0).into(),
                };
                self.entities.push(Box::new(e))
            }
            _ => println!("{:?}", message),
        }
    }

    fn update_display(&mut self) -> Result<(), Box<dyn error::Error>> {
        self.display.clear(Bgr565::BLACK)?;

        Rectangle::new(Point::zero(), self.display.size())
            .into_styled(PrimitiveStyle::with_stroke(Bgr565::WHITE, 1))
            .draw(&mut self.display)?;

        self.logo_position.x += self.step;
        if self.logo_position.x + 400 >= self.display.size().width as i32
            || self.logo_position.x <= 0
        {
            self.step *= -1;
        }

        fonts::Text::new("DJ Monad Presents", self.logo_position)
            .into_styled(MonoTextStyle::new(fonts::Font24x32, Bgr565::WHITE))
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
    let mut midi_in = MidiInput::new("midir forwarding input")?;
    midi_in.ignore(Ignore::None);
    let midi_out = MidiOutput::new("midir forwarding output")?;

    let in_port = select_port(&midi_in, "MIDIIN2 (Ableton Push 2)")?;
    println!();
    let out_port = select_port(&midi_out, "MIDIOUT2 (Ableton Push 2)")?;

    let mut conn_out = midi_out.connect(&out_port, "midir-forward")?;

    let mut app = App::new(&mut conn_out);
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

    loop {
        for event in rx.try_iter() {
            app.handle(event)
        }
        app.update_display()?;
        app.step();
        thread::sleep(time::Duration::from_millis(1000 / 60));
    }
}
