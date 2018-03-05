use std::thread;
use std::sync::mpsc;
use std::f32;
use std::f32::consts;

use cpal;
use cpal::{EventLoop, UnknownTypeBuffer};

pub fn start_audio_thread() -> mpsc::Sender<usize> {
    let (send, recv) = mpsc::channel();

    thread::spawn(move || {
        let event_loop = EventLoop::new();
        let endpoint = cpal::default_endpoint().expect("no output device available");
        let mut supported_formats_range = endpoint.supported_formats()
            .expect("error while querying formats");
        let format = supported_formats_range.next()
            .expect("no supported format?!")
            .with_max_samples_rate();
        let voice_id = event_loop.build_voice(&endpoint, &format).unwrap();
        event_loop.play(voice_id);

        let mut phase: usize = 0;
        let mut playing = false;
        let mut sin: [f32; 128] = [0.0; 128];
        for (i, x) in sin.iter_mut().enumerate() {
            *x = (i as f32 * 2.0 * consts::PI / 128.0).sin();
        }

        event_loop.run(move |_voice_id, buffer| {
            for msg in recv.try_iter() {
                if msg == 1 {
                    playing = true;
                }
            }

            match buffer {
                UnknownTypeBuffer::U16(mut buffer) => {
                    for elem in buffer.iter_mut() {
                        *elem = u16::max_value() / 2;
                    }
                },
                UnknownTypeBuffer::I16(mut buffer) => {
                    for elem in buffer.iter_mut() {
                        *elem = 0;
                    }
                },
                UnknownTypeBuffer::F32(mut buffer) => {
                    for elem in buffer.iter_mut() {
                        if playing {
                            phase = (phase + 1) % sin.len();
                            let value = sin[phase];
                            *elem = value;
                        } else {
                            *elem = 0.0;
                        }
                    }
                },
            }
        });
    });

    send
}