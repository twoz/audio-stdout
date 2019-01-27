extern crate portaudio;
extern crate libc;
#[macro_use]
extern crate clap;

use std::error::Error;
use std::io::{stdout, Write};
use std::mem::size_of;
use clap::{App, Arg, ArgMatches};
use portaudio::*;

arg_enum! {
enum SampleFormat {
    S16,
    S32,
    F32,
}
}

struct Args {
    samplerate: Option<f64>,
    format: SampleFormat,
    frames_per_buffer: u32,
    deinterleaved: bool,
}

fn get_arg<T: std::str::FromStr>(arg: &str, matches: &ArgMatches) -> Result<T, String> {
    value_t!(matches.value_of(arg), T).map_err(|e: clap::Error| { e.message })
}

impl Args {
    fn from_matches(matches : ArgMatches) -> Result<Args, String> {
        Ok(Args {
            samplerate: match matches.is_present("sample-rate") {
                true => Some(get_arg("sample-rate", &matches)?),
                false => None
            },
            format: get_arg("format", &matches)?,
            frames_per_buffer: get_arg("buffer-size", &matches)?,
            deinterleaved: matches.is_present("deinterleaved"),
        })
    }
}

fn parse_args() -> Result<Args, String> {
    let matches = App::new("Audio Stdout")
        .arg(Arg::from_usage("-s --sample-rate=[SAMPLERATE] 'Sample rate in Hz [default: input device default sample rate]'"))
        .arg(Arg::from_usage("-f --format=[FORMAT] 'Sample format to use'")
            .possible_values(&SampleFormat::variants())
            .default_value("S16"))
        .arg(Arg::from_usage("-b --buffer-size=[BUFFERSIZE] 'Size of the buffer in frames'")
            .default_value("256"))
        .arg(Arg::from_usage("--deinterleaved 'Output deinterleaved samples'"))
        .get_matches_from(std::env::args());

    Args::from_matches(matches)
}

fn create_stream_settings<T>(device : (DeviceIndex, DeviceInfo), args : Args) -> InputStreamSettings<T> {
    let (dev_idx, dev_info) = device;
    let parameters = StreamParameters::<T>::new(
        dev_idx,
        dev_info.max_input_channels,
        !args.deinterleaved,
        dev_info.default_low_input_latency);

    let samplerate = match args.samplerate {
        Some(s) => s,
        None => dev_info.default_sample_rate
    };
    InputStreamSettings::new(parameters, samplerate, args.frames_per_buffer)
}

fn to_bytes<T>(src: &[T]) -> &[u8] {
    unsafe { ::std::slice::from_raw_parts(src.as_ptr() as *const u8, src.len() * size_of::<T>()) }
}

fn run<T>(pa: &PortAudio, args : Args) -> Result<(), portaudio::Error>
    where T: 'static + portaudio::Sample {

    let (sender, receiver) = std::sync::mpsc::channel();
    #[allow(unused)]
        let callback = move |InputStreamCallbackArgs { buffer, frames, flags, time }| {
        sender.send(buffer);
        portaudio::Continue
    };
    let in_idx = pa.default_input_device()?;
    let in_dev = pa.device_info(in_idx)?;
    let settings = create_stream_settings::<T>((in_idx, in_dev), args);
    let mut stream = pa.open_non_blocking_stream(settings, callback)?;
    stream.start()?;

    loop {
        let mut stdout = stdout();
        match receiver.recv() {
            Ok(buffer) => {
                stdout.write_all(to_bytes(buffer)).ok();
                stdout.flush().ok();
            }
            Err(_) => {}
        }
    }
}

fn main() -> Result<(), String> {
    parse_args().and_then(|args: Args| {
        // Suppress portaudio output
        unsafe { libc::close(2); }

        PortAudio::new().and_then(|pa| {
            match args.format {
                SampleFormat::S16 => run::<i16>(&pa, args),
                SampleFormat::S32 => run::<i32>(&pa, args),
                SampleFormat::F32 => run::<f32>(&pa, args),
            }
        }).map_err(|e: portaudio::Error| { e.description().to_string() })
    })
}
