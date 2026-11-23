use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use hush_core::{
    read_wav_mono, write_wav_mono, list_devices, NsBackend, Pipeline, PipelineConfig,
    RealtimeSession, SessionConfig, SAMPLE_RATE,
};

#[derive(Parser)]
#[command(name = "hush", about = "48 kHz echo and noise cancellation")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List capture and playback devices.
    Devices,
    /// Clean a WAV file.
    Clean(CleanArgs),
    /// Process a live microphone. Prints one line per 10 ms frame.
    Run(RunArgs),
    /// Clean every WAV in a directory with one loaded model.
    Batch(BatchArgs),
}

#[derive(Parser)]
struct BatchArgs {
    #[arg(long)]
    input_dir: PathBuf,
    #[arg(long)]
    output_dir: PathBuf,
    #[arg(long, value_enum, default_value_t = NsChoice::DpdfNet)]
    ns: NsChoice,
    #[arg(long, default_value_t = 1.0)]
    strength: f32,
    /// Stop after this many files. Zero means all of them.
    #[arg(long, default_value_t = 0)]
    limit: usize,
}

#[derive(Parser)]
struct CleanArgs {
    #[arg(short, long)]
    input: PathBuf,
    #[arg(short, long)]
    output: PathBuf,
    /// Far-end reference. Required for echo cancellation to do anything.
    #[arg(long)]
    far: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = NsChoice::DpdfNet)]
    ns: NsChoice,
    #[arg(long, default_value_t = false)]
    aec: bool,
    /// 0 leaves noise in place, 1 is full suppression.
    #[arg(long, default_value_t = 1.0)]
    strength: f32,
    #[arg(long)]
    json: Option<PathBuf>,
}

#[derive(Parser)]
struct RunArgs {
    #[arg(long)]
    input: Option<String>,
    /// Optional monitor so you can hear the cleaned microphone.
    #[arg(long)]
    output: Option<String>,
    /// Loopback or reference device for echo cancellation.
    #[arg(long)]
    far_device: Option<String>,
    #[arg(long, value_enum, default_value_t = NsChoice::DpdfNet)]
    ns: NsChoice,
    #[arg(long, default_value_t = false)]
    aec: bool,
    #[arg(long, default_value_t = 1.0)]
    strength: f32,
    /// Publish cleaned audio to the virtual-microphone segment.
    #[arg(long, default_value_t = true)]
    ring: bool,
    /// Publish the virtual-speaker segment and play it (plus optional monitor) to --output.
    #[arg(long, default_value_t = false)]
    bridge: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum NsChoice {
    #[value(name = "dpdfnet", alias = "dpdf")]
    DpdfNet,
    #[value(name = "deepfilter", alias = "df")]
    DeepFilter,
    #[value(name = "rnnoise", alias = "rnn")]
    RnNoise,
    Off,
}

impl NsChoice {
    fn backend(self) -> NsBackend {
        match self {
            NsChoice::DpdfNet => NsBackend::DpdfNet,
            NsChoice::DeepFilter => NsBackend::DeepFilterNet3,
            NsChoice::RnNoise => NsBackend::RnNoise,
            NsChoice::Off => NsBackend::Off,
        }
    }
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Devices => devices(),
        Command::Clean(args) => clean(args),
        Command::Run(args) => run(args),
        Command::Batch(args) => batch(args),
    }
}

fn devices() -> ExitCode {
    match list_devices() {
        Ok(lists) => {
            println!("inputs:");
            if lists.inputs.is_empty() {
                println!("  (none)");
            }
            for name in lists.inputs {
                println!("  {name}");
            }
            println!("outputs:");
            if lists.outputs.is_empty() {
                println!("  (none)");
            }
            for name in lists.outputs {
                println!("  {name}");
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("hush: {err}");
            ExitCode::from(1)
        }
    }
}

fn clean(args: CleanArgs) -> ExitCode {
    let result = (|| {
        let (near, rate) = read_wav_mono(&args.input)?;
        if rate != SAMPLE_RATE {
            return Err(hush_core::Error::msg("internal resample did not reach 48 kHz"));
        }
        let far = match &args.far {
            Some(path) => Some(read_wav_mono(path)?.0),
            None => None,
        };
        if args.aec && far.is_none() {
            eprintln!("hush: --aec has no --far reference, so echo cancellation will not run");
        }
        let mut pipeline = Pipeline::new(PipelineConfig {
            enabled: true,
            muted: false,
            aec_enabled: args.aec && far.is_some(),
            ns_backend: args.ns.backend(),
            strength: args.strength,
        })?;
        let processed = pipeline.process_buffer(&near, far.as_deref())?;
        write_wav_mono(&args.output, &processed.samples, SAMPLE_RATE)?;
        println!(
            "wrote {}  frames {}  latency {:.2} ms  cpu mean {:.2}%  p95 {:.2}%  ns {}",
            args.output.display(),
            processed.stats.frames,
            processed.stats.algorithmic_latency_ms,
            processed.stats.cpu_percent_mean,
            processed.stats.cpu_percent_p95,
            processed.stats.ns_backend.as_str(),
        );
        if let Some(path) = &args.json {
            let text = serde_json::to_string_pretty(&processed.stats)
                .map_err(|err| hush_core::Error::msg(err.to_string()))?;
            std::fs::write(path, text)?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("hush: {err}");
            ExitCode::from(1)
        }
    }
}

fn batch(args: BatchArgs) -> ExitCode {
    let result = (|| {
        fs::create_dir_all(&args.output_dir)?;
        let mut names: Vec<_> = fs::read_dir(&args.input_dir)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
            })
            .collect();
        names.sort();
        if args.limit > 0 {
            names.truncate(args.limit);
        }
        let mut pipeline = Pipeline::new(PipelineConfig {
            enabled: true,
            muted: false,
            aec_enabled: false,
            ns_backend: args.ns.backend(),
            strength: args.strength,
        })?;
        let mut cpu = Vec::new();
        for path in &names {
            let (audio, _) = read_wav_mono(path)?;
            let processed = pipeline.process_buffer(&audio, None)?;
            let dest = args.output_dir.join(path.file_name().unwrap_or_default());
            write_wav_mono(&dest, &processed.samples, SAMPLE_RATE)?;
            cpu.push(processed.stats.cpu_percent_p95);
        }
        cpu.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p95 = if cpu.is_empty() {
            0.0
        } else {
            cpu[(cpu.len() - 1) * 95 / 100.max(1)]
        };
        println!(
            "batch {} files  ns {}  worst-file cpu p95 {p95:.2}%",
            names.len(),
            args.ns.backend().as_str()
        );
        Ok::<(), hush_core::Error>(())
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("hush: {err}");
            ExitCode::from(1)
        }
    }
}

fn run(args: RunArgs) -> ExitCode {
    let monitor_cleaned = args.output.is_some() && !args.bridge;
    let config = SessionConfig {
        input_device: args.input,
        output_device: args.output,
        far_device: args.far_device,
        pipeline: PipelineConfig {
            enabled: true,
            muted: false,
            aec_enabled: args.aec,
            ns_backend: args.ns.backend(),
            strength: args.strength,
        },
        publish_ring: args.ring,
        bridge_speaker: args.bridge,
        monitor_cleaned,
    };
    let mut session = match RealtimeSession::start(config) {
        Ok(session) => session,
        Err(err) => {
            eprintln!("hush: {err}");
            return ExitCode::from(1);
        }
    };
    println!(
        "live  ns {}  strength {:.2}  ring {}  ctrl-c to stop",
        args.ns.backend().as_str(),
        args.strength.clamp(0.0, 1.0),
        if args.ring { "on" } else { "off" }
    );
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let _ = ctrlc::set_handler(move || flag.store(true, Ordering::SeqCst));
    let mut seen = 0u64;
    while !stop.load(Ordering::SeqCst) {
        let stats = session.stats();
        if stats.frames != seen {
            println!(
                "frame {:6}  {:>7.3} ms  cpu {:>6.2}%  algo {:>5.2} ms  buf {:>5.2} ms  ns {}",
                stats.frames,
                stats.process_ns as f64 / 1_000_000.0,
                stats.cpu_percent,
                stats.algorithmic_latency_ms,
                stats.buffer_ms,
                stats.ns_backend.as_str(),
            );
            seen = stats.frames;
        }
        if let Some(err) = &stats.error {
            eprintln!("hush: {err}");
        }
        thread::sleep(Duration::from_millis(2));
    }
    session.stop();
    ExitCode::SUCCESS
}
