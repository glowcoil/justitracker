use portaudio as pa;

const SAMPLE_RATE: f64 = 44_100.0;
const FRAMES: u32 = 256;
const CHANNELS: i32 = 2;

pub struct Audio {
    pub portaudio: pa::PortAudio,
    pub stream: pa::Stream<pa::NonBlocking, pa::Output<f32>>,
}

impl Audio {
    pub fn start() -> Result<Audio, pa::Error> {
        let mut left_saw = 0.0;
        let mut right_saw = 0.0;

        let portaudio = pa::PortAudio::new()?;
        let settings = portaudio.default_output_stream_settings(CHANNELS, SAMPLE_RATE, FRAMES)?;
        let mut stream = portaudio.open_non_blocking_stream(settings, move |args| {
            for sample in args.buffer.iter_mut() {
                *sample = 0.0;
            }
            pa::Continue
        })?;

        stream.start()?;

        Ok(Audio { portaudio, stream })
    }
}
