use crate::{Runtime, ensure_hud, run_ask, settings_overlay};
use cpal::{
    FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use hound::{SampleFormat as WavSampleFormat, WavSpec, WavWriter};
use kitt_domain::RouteHint;
use kitt_protocol::{HudEvent, HudState};
use rustpotter::{AudioFmt, Rustpotter, RustpotterConfig, SampleFormat as PotterSampleFormat};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs,
    fs::OpenOptions,
    io::{BufWriter, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const AUDIO_CHUNK_QUEUE: usize = 32;
const EVENT_QUEUE: usize = 2;
const WAKEWORD_KEY: &str = "kitt";
const CAPTURE_RESTART_MIN: Duration = Duration::from_secs(1);
const CAPTURE_RESTART_MAX: Duration = Duration::from_secs(30);
const STALE_AUDIO_MAX_AGE: Duration = Duration::from_secs(6 * 60 * 60);
const STT_HEALTH_MAX_BYTES: u64 = 64 * 1024;
const STT_HEALTH_CONNECT_TIMEOUT: Duration = Duration::from_millis(750);
const STT_HEALTH_TIMEOUT: Duration = Duration::from_secs(2);
const STT_START_POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActivationMode {
    #[default]
    Auto,
    Wakeword,
    TranscriptPrefix,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceConfig {
    pub enabled: bool,
    pub locale: String,
    pub activation_mode: ActivationMode,
    pub wakeword_model_path: Option<String>,
    pub wake_phrases: Vec<String>,
    pub min_rms: f32,
    pub noise_multiplier: f32,
    pub pre_roll_ms: u64,
    pub min_speech_ms: u64,
    pub silence_ms: u64,
    pub max_utterance_ms: u64,
    pub command_timeout_ms: u64,
    pub stt_autostart: bool,
    pub stt_worker_model: String,
    pub stt_start_timeout_ms: u64,
    pub tts_enabled: bool,
    pub echo_guard_ms: u64,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            locale: "pt-BR".into(),
            activation_mode: ActivationMode::Auto,
            wakeword_model_path: Some("wakewords/kitt.rpw".into()),
            wake_phrases: vec![
                "kitt".into(),
                "kit".into(),
                "hey kitt".into(),
                "ei kitt".into(),
            ],
            min_rms: 0.015,
            noise_multiplier: 3.0,
            pre_roll_ms: 200,
            min_speech_ms: 250,
            silence_ms: 650,
            max_utterance_ms: 12_000,
            command_timeout_ms: 7_000,
            stt_autostart: true,
            stt_worker_model: "base".into(),
            stt_start_timeout_ms: 60_000,
            tts_enabled: true,
            echo_guard_ms: 350,
        }
    }
}

impl VoiceConfig {
    pub fn load_or_create(config_dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(config_dir).map_err(|e| format!("create voice config dir: {e}"))?;
        let path = config_dir.join("voice.json");
        if path.exists() {
            let config: Self = serde_json::from_str(
                &fs::read_to_string(&path).map_err(|e| format!("read voice.json: {e}"))?,
            )
            .map_err(|e| format!("parse voice.json: {e}"))?;
            config.validate()?;
            return Ok(config);
        }

        let config = Self::default();
        config.validate()?;
        fs::write(
            &path,
            serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("write voice.json: {e}"))?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        if self.locale.trim().is_empty() {
            return Err("voice locale cannot be empty".into());
        }
        if self.wake_phrases.is_empty()
            || self
                .wake_phrases
                .iter()
                .all(|phrase| phrase.trim().is_empty())
        {
            return Err("at least one wake phrase is required".into());
        }
        if !(0.0001..=0.5).contains(&self.min_rms) {
            return Err("voice min_rms must be between 0.0001 and 0.5".into());
        }
        if !(1.1..=20.0).contains(&self.noise_multiplier) {
            return Err("voice noise_multiplier must be between 1.1 and 20".into());
        }
        if self.min_speech_ms == 0
            || self.silence_ms == 0
            || self.max_utterance_ms <= self.min_speech_ms
            || self.command_timeout_ms == 0
        {
            return Err("invalid voice timing configuration".into());
        }
        if self.stt_worker_model.trim().is_empty() {
            return Err("voice stt_worker_model cannot be empty".into());
        }
        if !(1_000..=300_000).contains(&self.stt_start_timeout_ms) {
            return Err("voice stt_start_timeout_ms must be between 1000 and 300000".into());
        }
        Ok(())
    }

    fn wakeword_path(&self, config_dir: &Path) -> Option<PathBuf> {
        let raw = self.wakeword_model_path.as_deref()?.trim();
        if raw.is_empty() {
            return None;
        }
        let path = PathBuf::from(raw);
        Some(if path.is_absolute() {
            path
        } else {
            config_dir.join(path)
        })
    }

    fn resolved_mode(&self, config_dir: &Path) -> ActivationMode {
        match self.activation_mode {
            ActivationMode::Auto => {
                if self
                    .wakeword_path(config_dir)
                    .as_ref()
                    .is_some_and(|path| path.is_file())
                {
                    ActivationMode::Wakeword
                } else {
                    ActivationMode::TranscriptPrefix
                }
            }
            mode => mode,
        }
    }
}

#[derive(Debug)]
enum CaptureEvent {
    WakeDetected,
    WakeExpired,
    Utterance(PathBuf),
}

pub fn start(runtime: Arc<Runtime>, config_dir: &Path) -> Result<(), String> {
    let mut config = VoiceConfig::load_or_create(config_dir)?;
    settings_overlay::apply_voice(config_dir, &mut config)?;
    config.validate()?;
    if !config.enabled {
        eprintln!("kitt voice disabled by voice.json");
        return Ok(());
    }

    let mode = config.resolved_mode(config_dir);
    if config.activation_mode == ActivationMode::Auto && mode == ActivationMode::TranscriptPrefix {
        eprintln!(
            "kitt voice: wakeword model unavailable; auto mode is using local transcript-prefix fallback (higher CPU/STT usage)"
        );
    }
    let _ = cleanup_stale_voice_cache(config_dir, STALE_AUDIO_MAX_AGE);

    if mode == ActivationMode::TranscriptPrefix && !runtime.service.transcriber_is_local() {
        return Err(
            "hands-free transcript_prefix activation requires a local STT provider; use a local wakeword model before allowing remote STT"
                .into(),
        );
    }
    if mode == ActivationMode::Wakeword {
        let wakeword = config
            .wakeword_path(config_dir)
            .ok_or_else(|| "wakeword activation requires wakeword_model_path".to_string())?;
        if !wakeword.is_file() {
            return Err(format!(
                "wakeword model not found: {}; use activation_mode=auto for local-STT fallback or create the .rpw model",
                wakeword.display()
            ));
        }
    }

    if mode == ActivationMode::TranscriptPrefix && runtime.service.transcriber_is_local() {
        if let Err(error) = ensure_local_stt_ready(&runtime, &config) {
            show_voice_error(&runtime, &format!("STT local indisponível: {error}"));
            return Err(error);
        }
    }

    let paused = Arc::new(AtomicBool::new(false));
    let (events_tx, events_rx) = mpsc::sync_channel(EVENT_QUEUE);

    let capture_config = config.clone();
    let capture_dir = config_dir.to_path_buf();
    let capture_paused = paused.clone();
    let capture_events = events_tx.clone();
    let capture_runtime = runtime.clone();
    thread::Builder::new()
        .name("kitt-voice-capture".into())
        .spawn(move || {
            let mut retry = CAPTURE_RESTART_MIN;
            loop {
                let started = Instant::now();
                let result = capture_loop(
                    capture_config.clone(),
                    capture_dir.clone(),
                    mode,
                    capture_paused.clone(),
                    capture_events.clone(),
                );
                match result {
                    Ok(()) => eprintln!("kitt voice capture stream ended; reopening microphone"),
                    Err(error) => {
                        eprintln!("kitt voice capture failed: {error}; reopening microphone");
                        show_voice_error(
                            &capture_runtime,
                            &format!("Microfone indisponível: {error}"),
                        );
                    }
                }
                if started.elapsed() >= Duration::from_secs(60) {
                    retry = CAPTURE_RESTART_MIN;
                } else {
                    retry = (retry * 2).min(CAPTURE_RESTART_MAX);
                }
                thread::sleep(retry);
            }
        })
        .map_err(|e| format!("spawn voice capture: {e}"))?;

    let pipeline_runtime = runtime;
    let pipeline_config = config;
    thread::Builder::new()
        .name("kitt-voice-pipeline".into())
        .spawn(move || pipeline_loop(pipeline_runtime, pipeline_config, mode, paused, events_rx))
        .map_err(|e| format!("spawn voice pipeline: {e}"))?;

    eprintln!("kitt voice enabled ({})", mode_name(mode));
    Ok(())
}

fn mode_name(mode: ActivationMode) -> &'static str {
    match mode {
        ActivationMode::Auto => "auto",
        ActivationMode::Wakeword => "wakeword",
        ActivationMode::TranscriptPrefix => "transcript_prefix",
    }
}

fn capture_loop(
    config: VoiceConfig,
    config_dir: PathBuf,
    mode: ActivationMode,
    paused: Arc<AtomicBool>,
    events: SyncSender<CaptureEvent>,
) -> Result<(), String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "no default microphone/input device available".to_string())?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("default microphone config: {e}"))?;
    let sample_format = supported.sample_format();
    let stream_config = supported.config();
    let sample_rate = stream_config.sample_rate as usize;
    let channels = stream_config.channels as usize;
    if channels == 0 || sample_rate == 0 {
        return Err("invalid microphone configuration".into());
    }

    let (samples_tx, samples_rx) = mpsc::sync_channel(AUDIO_CHUNK_QUEUE);
    let stream = build_input_stream(&device, stream_config, sample_format, channels, samples_tx)?;
    stream
        .play()
        .map_err(|e| format!("start microphone stream: {e}"))?;

    match mode {
        ActivationMode::Wakeword => run_wakeword_capture(
            &config,
            &config_dir,
            sample_rate,
            paused,
            events,
            samples_rx,
        ),
        ActivationMode::TranscriptPrefix | ActivationMode::Auto => run_transcript_capture(
            &config,
            &config_dir,
            sample_rate,
            paused,
            events,
            samples_rx,
        ),
    }
}

fn build_input_stream(
    device: &cpal::Device,
    config: StreamConfig,
    sample_format: SampleFormat,
    channels: usize,
    tx: SyncSender<Vec<f32>>,
) -> Result<Stream, String> {
    match sample_format {
        SampleFormat::I8 => build_typed_input::<i8>(device, config, channels, tx),
        SampleFormat::I16 => build_typed_input::<i16>(device, config, channels, tx),
        SampleFormat::I24 => build_typed_input::<cpal::I24>(device, config, channels, tx),
        SampleFormat::I32 => build_typed_input::<i32>(device, config, channels, tx),
        SampleFormat::I64 => build_typed_input::<i64>(device, config, channels, tx),
        SampleFormat::U8 => build_typed_input::<u8>(device, config, channels, tx),
        SampleFormat::U16 => build_typed_input::<u16>(device, config, channels, tx),
        SampleFormat::U24 => build_typed_input::<cpal::U24>(device, config, channels, tx),
        SampleFormat::U32 => build_typed_input::<u32>(device, config, channels, tx),
        SampleFormat::U64 => build_typed_input::<u64>(device, config, channels, tx),
        SampleFormat::F32 => build_typed_input::<f32>(device, config, channels, tx),
        SampleFormat::F64 => build_typed_input::<f64>(device, config, channels, tx),
        other => Err(format!("unsupported microphone sample format: {other}")),
    }
}

fn build_typed_input<T>(
    device: &cpal::Device,
    config: StreamConfig,
    channels: usize,
    tx: SyncSender<Vec<f32>>,
) -> Result<Stream, String>
where
    T: Sample + SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let err_fn = |error| eprintln!("microphone stream error: {error}");
    device
        .build_input_stream::<T, _, _>(
            config,
            move |data: &[T], _| {
                let mut mono = Vec::with_capacity(data.len().div_ceil(channels));
                for frame in data.chunks(channels) {
                    if frame.is_empty() {
                        continue;
                    }
                    let sum: f32 = frame.iter().copied().map(f32::from_sample).sum();
                    mono.push(sum / frame.len() as f32);
                }
                if !mono.is_empty() {
                    let _ = tx.try_send(mono);
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| format!("build microphone stream: {e}"))
}

fn run_transcript_capture(
    config: &VoiceConfig,
    config_dir: &Path,
    sample_rate: usize,
    paused: Arc<AtomicBool>,
    events: SyncSender<CaptureEvent>,
    samples: Receiver<Vec<f32>>,
) -> Result<(), String> {
    let mut segmenter = Segmenter::new(sample_rate, config.clone());
    while let Ok(chunk) = samples.recv() {
        if paused.load(Ordering::Acquire) {
            segmenter.reset();
            continue;
        }
        if let Some(audio) = segmenter.push(&chunk) {
            let path = write_wav(config_dir, sample_rate, &audio)?;
            if send_event(&events, CaptureEvent::Utterance(path.clone())).is_err() {
                let _ = fs::remove_file(path);
            }
        }
    }
    Ok(())
}

fn run_wakeword_capture(
    config: &VoiceConfig,
    config_dir: &Path,
    sample_rate: usize,
    paused: Arc<AtomicBool>,
    events: SyncSender<CaptureEvent>,
    samples: Receiver<Vec<f32>>,
) -> Result<(), String> {
    let wakeword_path = config
        .wakeword_path(config_dir)
        .ok_or_else(|| "wakeword_model_path is required".to_string())?;
    let wakeword_path = wakeword_path
        .to_str()
        .ok_or_else(|| "wakeword model path is not valid UTF-8".to_string())?;

    let mut potter_config = RustpotterConfig {
        fmt: AudioFmt {
            sample_rate,
            sample_format: PotterSampleFormat::F32,
            channels: 1,
            ..AudioFmt::default()
        },
        ..RustpotterConfig::default()
    };
    potter_config.detector.eager = true;
    let mut potter = Rustpotter::new(&potter_config)
        .map_err(|e| format!("initialize wakeword detector: {e}"))?;
    potter
        .add_wakeword_from_file(WAKEWORD_KEY, wakeword_path)
        .map_err(|e| format!("load wakeword model: {e}"))?;

    let frame_size = potter.get_samples_per_frame();
    if frame_size == 0 {
        return Err("wakeword detector returned zero frame size".into());
    }
    let mut detector_buffer = VecDeque::<f32>::with_capacity(frame_size * 2);
    let mut segmenter = Segmenter::new(sample_rate, config.clone());
    let mut armed_until: Option<Instant> = None;

    while let Ok(chunk) = samples.recv() {
        if paused.load(Ordering::Acquire) {
            detector_buffer.clear();
            segmenter.reset();
            armed_until = None;
            potter.reset();
            continue;
        }

        detector_buffer.extend(chunk.iter().copied());
        let mut detected_this_chunk = false;
        while detector_buffer.len() >= frame_size {
            let mut frame = Vec::with_capacity(frame_size);
            for _ in 0..frame_size {
                if let Some(sample) = detector_buffer.pop_front() {
                    frame.push(sample);
                }
            }
            if potter.process_samples(frame).is_some() {
                detected_this_chunk = true;
                armed_until =
                    Some(Instant::now() + Duration::from_millis(config.command_timeout_ms));
                segmenter.reset();
                let _ = send_event(&events, CaptureEvent::WakeDetected);
                break;
            }
        }

        let Some(deadline) = armed_until else {
            continue;
        };
        if Instant::now() >= deadline {
            armed_until = None;
            segmenter.reset();
            let _ = send_event(&events, CaptureEvent::WakeExpired);
            continue;
        }
        if detected_this_chunk {
            continue;
        }

        if let Some(audio) = segmenter.push(&chunk) {
            let path = write_wav(config_dir, sample_rate, &audio)?;
            if send_event(&events, CaptureEvent::Utterance(path.clone())).is_err() {
                let _ = fs::remove_file(path);
            }
            armed_until = None;
            segmenter.reset();
        }
    }
    Ok(())
}

fn send_event(tx: &SyncSender<CaptureEvent>, event: CaptureEvent) -> Result<(), ()> {
    match tx.try_send(event) {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => Err(()),
    }
}

#[derive(Debug)]
enum LocalSttProbe {
    Ready,
    Unreachable(String),
    Degraded(String),
}

fn stt_health_url(base_url: &str) -> String {
    format!("{}/health", base_url.trim_end_matches('/'))
}

fn probe_local_stt(base_url: &str) -> LocalSttProbe {
    let client = match reqwest::blocking::Client::builder()
        .connect_timeout(STT_HEALTH_CONNECT_TIMEOUT)
        .timeout(STT_HEALTH_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(error) => return LocalSttProbe::Unreachable(error.to_string()),
    };

    let response = match client.get(stt_health_url(base_url)).send() {
        Ok(response) => response,
        Err(error) => return LocalSttProbe::Unreachable(error.to_string()),
    };
    let status = response.status();
    if !status.is_success() {
        return LocalSttProbe::Degraded(format!("health endpoint returned HTTP {status}"));
    }
    if response
        .content_length()
        .is_some_and(|size| size > STT_HEALTH_MAX_BYTES)
    {
        return LocalSttProbe::Degraded("health response is too large".into());
    }

    let mut bytes = Vec::new();
    if let Err(error) = response
        .take(STT_HEALTH_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
    {
        return LocalSttProbe::Degraded(format!("health response read failed: {error}"));
    }
    if bytes.len() as u64 > STT_HEALTH_MAX_BYTES {
        return LocalSttProbe::Degraded("health response is too large".into());
    }

    let body: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(body) => body,
        Err(error) => {
            return LocalSttProbe::Degraded(format!("invalid STT health JSON: {error}"));
        }
    };
    if body.get("ready").and_then(serde_json::Value::as_bool) == Some(true) {
        LocalSttProbe::Ready
    } else {
        let status = body
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("degraded");
        let engine = body
            .get("engine")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        LocalSttProbe::Degraded(format!("status={status}, engine={engine}"))
    }
}

fn local_worker_endpoint(base_url: &str) -> Result<(String, u16), String> {
    let parsed =
        url::Url::parse(base_url).map_err(|error| format!("invalid local STT URL: {error}"))?;
    if parsed.scheme() != "http" {
        return Err("local STT worker auto-start requires an http URL".into());
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "local STT URL has no host".to_string())?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false);
    if !loopback {
        return Err("local STT worker auto-start requires a loopback host".into());
    }
    if host.contains(':') {
        return Err("local STT worker auto-start currently requires IPv4 loopback".into());
    }
    if parsed.path().trim_end_matches('/') != "/v1" {
        return Err("local STT worker auto-start requires base_url ending in /v1".into());
    }
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| "local STT URL has no usable port".to_string())?;
    Ok((host.to_string(), port))
}

fn worker_launch_candidates() -> Vec<(String, Vec<String>)> {
    let mut candidates = Vec::new();
    if let Ok(bin) = std::env::var("KITT_STT_WORKER_BIN") {
        if !bin.trim().is_empty() {
            candidates.push((bin, Vec::new()));
        }
    }
    candidates.push(("kitt-stt".into(), Vec::new()));

    // Automatically probe candidate virtualenv locations in the workspace / system
    let mut probe_dirs: Vec<PathBuf> = Vec::new();
    if let Ok(curr) = std::env::current_dir() {
        probe_dirs.push(curr.clone());
        if let Some(p) = curr.parent() {
            probe_dirs.push(p.to_path_buf());
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(p) = exe.parent() {
            probe_dirs.push(p.to_path_buf());
            if let Some(p2) = p.parent() {
                probe_dirs.push(p2.to_path_buf());
                if let Some(p3) = p2.parent() {
                    probe_dirs.push(p3.to_path_buf());
                }
            }
        }
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        probe_dirs.push(home.join("otherProjects/kitt"));
        probe_dirs.push(home.join(".local/bin"));
    }

    for dir in &probe_dirs {
        for rel in [
            "kitt-agent-cli/.venv/bin/kitt-stt",
            "kitt-ai-workers/.venv/bin/kitt-stt",
            ".venv/bin/kitt-stt",
            "kitt-agent-cli/.venv/Scripts/kitt-stt.exe",
            "kitt-ai-workers/.venv/Scripts/kitt-stt.exe",
            ".venv/Scripts/kitt-stt.exe",
        ] {
            let path = dir.join(rel);
            if path.is_file() {
                candidates.push((path.to_string_lossy().to_string(), Vec::new()));
            }
        }
    }

    if let Ok(python) = std::env::var("KITT_STT_PYTHON") {
        if !python.trim().is_empty() {
            candidates.push((python, vec!["-m".into(), "kitt_workers.stt_server".into()]));
        }
    }

    for dir in &probe_dirs {
        for rel in [
            "kitt-agent-cli/.venv/bin/python3",
            "kitt-agent-cli/.venv/bin/python",
            "kitt-ai-workers/.venv/bin/python3",
            "kitt-ai-workers/.venv/bin/python",
            ".venv/bin/python3",
            ".venv/bin/python",
            "kitt-agent-cli/.venv/Scripts/python.exe",
            "kitt-ai-workers/.venv/Scripts/python.exe",
            ".venv/Scripts/python.exe",
        ] {
            let path = dir.join(rel);
            if path.is_file() {
                candidates.push((path.to_string_lossy().to_string(), vec!["-m".into(), "kitt_workers.stt_server".into()]));
            }
        }
    }

    #[cfg(windows)]
    {
        candidates.push((
            "py".into(),
            vec!["-3".into(), "-m".into(), "kitt_workers.stt_server".into()],
        ));
        candidates.push((
            "python".into(),
            vec!["-m".into(), "kitt_workers.stt_server".into()],
        ));
    }
    #[cfg(not(windows))]
    {
        candidates.push((
            "python3".into(),
            vec!["-m".into(), "kitt_workers.stt_server".into()],
        ));
        candidates.push((
            "python".into(),
            vec!["-m".into(), "kitt_workers.stt_server".into()],
        ));
    }
    candidates
}

fn spawn_local_stt_worker(runtime: &Arc<Runtime>, config: &VoiceConfig) -> Result<(), String> {
    let (host, port) = local_worker_endpoint(&runtime.stt_base_url)?;
    let mut guard = runtime
        .stt_worker_process
        .lock()
        .map_err(|_| "local STT worker lock poisoned".to_string())?;

    let existing_status = match guard.as_mut() {
        Some(child) => child
            .try_wait()
            .map_err(|error| format!("inspect local STT worker: {error}"))?,
        None => None,
    };
    match existing_status {
        None if guard.is_some() => return Ok(()),
        Some(_) => *guard = None,
        None => {}
    }

    let common_args = vec![
        "--host".to_string(),
        host,
        "--port".to_string(),
        port.to_string(),
        "--model".to_string(),
        config.stt_worker_model.trim().to_string(),
        "--parent-stdin-lifecycle".to_string(),
    ];

    let mut failures = Vec::new();
    for (program, prefix_args) in worker_launch_candidates() {
        let mut command = Command::new(&program);
        command.args(prefix_args).args(&common_args);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        match command.spawn() {
            Ok(child) => {
                *guard = Some(child);
                eprintln!("kitt voice: started local STT worker using {program}");
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                failures.push(format!("{program}: not found"));
            }
            Err(error) => failures.push(format!("{program}: {error}")),
        }
    }

    Err(format!(
        "could not start local STT worker ({}). Install kitt-ai-workers[stt] or set KITT_STT_WORKER_BIN/KITT_STT_PYTHON",
        failures.join("; ")
    ))
}

fn owned_stt_worker_exit(runtime: &Arc<Runtime>) -> Result<Option<String>, String> {
    let mut guard = runtime
        .stt_worker_process
        .lock()
        .map_err(|_| "local STT worker lock poisoned".to_string())?;
    let status = match guard.as_mut() {
        Some(child) => child
            .try_wait()
            .map_err(|error| format!("inspect local STT worker: {error}"))?,
        None => return Ok(None),
    };
    if let Some(status) = status {
        *guard = None;
        Ok(Some(status.to_string()))
    } else {
        Ok(None)
    }
}

fn stop_owned_stt_worker(runtime: &Arc<Runtime>) {
    let Ok(mut guard) = runtime.stt_worker_process.lock() else {
        return;
    };
    let Some(mut child) = guard.take() else {
        return;
    };
    let _ = child.kill();
    let _ = child.wait();
}

fn ensure_local_stt_ready(runtime: &Arc<Runtime>, config: &VoiceConfig) -> Result<(), String> {
    match probe_local_stt(&runtime.stt_base_url) {
        LocalSttProbe::Ready => return Ok(()),
        LocalSttProbe::Degraded(detail) => {
            return Err(format!("local STT server is not ready: {detail}"));
        }
        LocalSttProbe::Unreachable(error) if !config.stt_autostart => {
            return Err(format!("local STT endpoint is unreachable: {error}"));
        }
        LocalSttProbe::Unreachable(_) => {}
    }

    spawn_local_stt_worker(runtime, config)?;
    let deadline = Instant::now() + Duration::from_millis(config.stt_start_timeout_ms);
    loop {
        match probe_local_stt(&runtime.stt_base_url) {
            LocalSttProbe::Ready => return Ok(()),
            LocalSttProbe::Degraded(detail) => {
                stop_owned_stt_worker(runtime);
                return Err(format!(
                    "local STT worker started but is not ready: {detail}. Install the STT extra with pip install -e '.[stt]'"
                ));
            }
            LocalSttProbe::Unreachable(_) => {}
        }

        if let Some(status) = owned_stt_worker_exit(runtime)? {
            return Err(format!(
                "local STT worker exited before becoming ready: {status}"
            ));
        }
        if Instant::now() >= deadline {
            stop_owned_stt_worker(runtime);
            return Err(format!(
                "local STT worker did not become ready within {} ms",
                config.stt_start_timeout_ms
            ));
        }
        thread::sleep(STT_START_POLL_INTERVAL);
    }
}

fn show_voice_error(runtime: &Arc<Runtime>, message: &str) {
    ensure_hud(runtime);
    runtime.hud.send(HudEvent::Text {
        content: format!("KITT Voice: {message}"),
        ttl_ms: 10_000,
    });
}

fn pipeline_loop(
    runtime: Arc<Runtime>,
    config: VoiceConfig,
    mode: ActivationMode,
    paused: Arc<AtomicBool>,
    events: Receiver<CaptureEvent>,
) {
    let matcher = WakePhraseMatcher::new(&config.wake_phrases);
    let mut awaiting_command_until: Option<Instant> = None;

    while let Ok(event) = events.recv() {
        match event {
            CaptureEvent::WakeDetected => {
                awaiting_command_until =
                    Some(Instant::now() + Duration::from_millis(config.command_timeout_ms));
                show_listening(&runtime, config.command_timeout_ms);
                if runtime.service.transcriber_is_local() {
                    if let Err(error) = ensure_local_stt_ready(&runtime, &config) {
                        awaiting_command_until = None;
                        eprintln!("kitt voice: local STT startup failed after wake: {error}");
                        show_voice_error(&runtime, &format!("STT local indisponível: {error}"));
                    }
                }
            }
            CaptureEvent::WakeExpired => {
                awaiting_command_until = None;
                runtime.hud.send(HudEvent::Hide);
            }
            CaptureEvent::Utterance(path) => {
                let _audio = TempAudioGuard(path.clone());
                paused.store(true, Ordering::Release);
                let _pause = PauseReset(paused.clone());

                let transcript = match runtime
                    .service
                    .transcribe(&path, Some(config.locale.as_str()))
                {
                    Ok(text) => text.trim().to_string(),
                    Err(error) => {
                        static LAST_STT_ERROR_LOG: std::sync::atomic::AtomicU64 =
                            std::sync::atomic::AtomicU64::new(0);
                        let now_secs = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        let last = LAST_STT_ERROR_LOG.load(std::sync::atomic::Ordering::Relaxed);
                        if now_secs.saturating_sub(last) >= 15 {
                            LAST_STT_ERROR_LOG
                                .store(now_secs, std::sync::atomic::Ordering::Relaxed);
                            eprintln!(
                                "kitt voice: STT endpoint unreachable ({error}). Install kitt-ai-workers[stt] or start kitt-stt on port 8000."
                            );
                        }
                        show_voice_error(&runtime, &format!("Falha na transcrição: {error}"));
                        thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                };
                if transcript.is_empty() {
                    continue;
                }

                let now = Instant::now();
                let waiting = awaiting_command_until.is_some_and(|deadline| now < deadline);
                if awaiting_command_until.is_some() && !waiting {
                    awaiting_command_until = None;
                }

                let command = match mode {
                    ActivationMode::Wakeword => {
                        if waiting {
                            matcher
                                .strip_prefix(&transcript)
                                .unwrap_or_else(|| transcript.clone())
                        } else {
                            continue;
                        }
                    }
                    ActivationMode::TranscriptPrefix | ActivationMode::Auto => {
                        if waiting {
                            transcript.clone()
                        } else if let Some(command) = matcher.strip_prefix(&transcript) {
                            command
                        } else {
                            continue;
                        }
                    }
                };

                let command = command.trim();
                if command.is_empty() {
                    awaiting_command_until =
                        Some(Instant::now() + Duration::from_millis(config.command_timeout_ms));
                    show_listening(&runtime, config.command_timeout_ms);
                    continue;
                }
                awaiting_command_until = None;

                match run_ask(&runtime, command, RouteHint::Auto, true) {
                    Ok(answer) => {
                        if config.tts_enabled {
                            if let Err(error) = runtime
                                .service
                                .speak(&answer.text, Some(config.locale.as_str()))
                            {
                                eprintln!("voice TTS unavailable: {error}");
                            }
                        }
                    }
                    Err((_, error)) => eprintln!("voice assistant request failed: {error}"),
                }
                thread::sleep(Duration::from_millis(config.echo_guard_ms));
            }
        }
    }
}

fn show_listening(runtime: &Arc<Runtime>, ttl_ms: u64) {
    ensure_hud(runtime);
    runtime.hud.send(HudEvent::Status {
        state: HudState::Listening,
        message: Some("Ouvindo…".into()),
    });
    runtime.hud.send(HudEvent::Text {
        content: "Ouvindo…".into(),
        ttl_ms,
    });
}

struct PauseReset(Arc<AtomicBool>);

impl Drop for PauseReset {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

struct TempAudioGuard(PathBuf);

impl Drop for TempAudioGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

struct WakePhraseMatcher {
    phrases: Vec<Vec<String>>,
}

impl WakePhraseMatcher {
    fn new(phrases: &[String]) -> Self {
        let phrases = phrases
            .iter()
            .map(|phrase| normalize_tokens(phrase))
            .filter(|tokens| !tokens.is_empty())
            .collect();
        Self { phrases }
    }

    fn strip_prefix(&self, text: &str) -> Option<String> {
        let original: Vec<&str> = text.split_whitespace().collect();
        let normalized: Vec<String> = original
            .iter()
            .map(|token| normalize_token(token))
            .collect();
        for phrase in &self.phrases {
            if phrase.is_empty() {
                continue;
            }
            if normalized.len() >= phrase.len() && normalized[..phrase.len()] == phrase[..] {
                return Some(original[phrase.len()..].join(" "));
            }
            for offset in 1..=2 {
                if normalized.len() >= offset + phrase.len()
                    && normalized[offset..offset + phrase.len()] == phrase[..]
                {
                    return Some(original[offset + phrase.len()..].join(" "));
                }
            }
        }
        None
    }
}

fn normalize_tokens(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(normalize_token)
        .filter(|token| !token.is_empty())
        .collect()
}

fn normalize_token(token: &str) -> String {
    token
        .chars()
        .map(|ch| match ch.to_ascii_lowercase() {
            'á' | 'à' | 'ã' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'õ' | 'ô' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

struct Segmenter {
    sample_rate: usize,
    config: VoiceConfig,
    noise_floor: f32,
    pre_roll: VecDeque<f32>,
    current: Vec<f32>,
    speaking: bool,
    speech_samples: usize,
    silence_samples: usize,
}

impl Segmenter {
    fn new(sample_rate: usize, config: VoiceConfig) -> Self {
        Self {
            sample_rate,
            config,
            noise_floor: 0.005,
            pre_roll: VecDeque::new(),
            current: Vec::new(),
            speaking: false,
            speech_samples: 0,
            silence_samples: 0,
        }
    }

    fn push(&mut self, samples: &[f32]) -> Option<Vec<f32>> {
        if samples.is_empty() {
            return None;
        }
        let rms = rms(samples);
        let threshold = self
            .config
            .min_rms
            .max(self.noise_floor * self.config.noise_multiplier);
        let voiced = rms >= threshold;

        if !self.speaking {
            if !voiced {
                self.noise_floor = self.noise_floor * 0.98 + rms * 0.02;
                self.extend_pre_roll(samples);
                return None;
            }
            self.speaking = true;
            self.current.extend(self.pre_roll.drain(..));
            self.current.extend_from_slice(samples);
            self.speech_samples = samples.len();
            self.silence_samples = 0;
        } else {
            self.current.extend_from_slice(samples);
            if voiced {
                self.speech_samples += samples.len();
                self.silence_samples = 0;
            } else {
                self.silence_samples += samples.len();
            }
        }

        let silence_limit = ms_to_samples(self.sample_rate, self.config.silence_ms);
        let max_limit = ms_to_samples(self.sample_rate, self.config.max_utterance_ms);
        if self.silence_samples < silence_limit && self.current.len() < max_limit {
            return None;
        }

        let min_speech = ms_to_samples(self.sample_rate, self.config.min_speech_ms);
        let valid = self.speech_samples >= min_speech;
        let audio = std::mem::take(&mut self.current);
        self.speaking = false;
        self.speech_samples = 0;
        self.silence_samples = 0;
        self.pre_roll.clear();
        valid.then_some(audio)
    }

    fn extend_pre_roll(&mut self, samples: &[f32]) {
        let limit = ms_to_samples(self.sample_rate, self.config.pre_roll_ms);
        self.pre_roll.extend(samples.iter().copied());
        while self.pre_roll.len() > limit {
            self.pre_roll.pop_front();
        }
    }

    fn reset(&mut self) {
        self.pre_roll.clear();
        self.current.clear();
        self.speaking = false;
        self.speech_samples = 0;
        self.silence_samples = 0;
    }
}

fn ms_to_samples(sample_rate: usize, millis: u64) -> usize {
    ((sample_rate as u128 * millis as u128) / 1000).max(1) as usize
}

fn rms(samples: &[f32]) -> f32 {
    let sum = samples
        .iter()
        .map(|sample| {
            let sample = sample.clamp(-1.0, 1.0);
            sample * sample
        })
        .sum::<f32>();
    (sum / samples.len() as f32).sqrt()
}

fn write_wav(config_dir: &Path, sample_rate: usize, samples: &[f32]) -> Result<PathBuf, String> {
    let cache_dir = config_dir.join("voice-cache");
    ensure_private_directory(&cache_dir)?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = cache_dir.join(format!("utterance-{}-{nanos}.wav", std::process::id()));
    let spec = WavSpec {
        channels: 1,
        sample_rate: sample_rate as u32,
        bits_per_sample: 16,
        sample_format: WavSampleFormat::Int,
    };
    let file = create_private_file(&path)?;
    let mut writer =
        WavWriter::new(BufWriter::new(file), spec).map_err(|e| format!("create WAV: {e}"))?;
    for sample in samples {
        let sample = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer
            .write_sample(sample)
            .map_err(|e| format!("write WAV: {e}"))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("finalize WAV: {e}"))?;
    Ok(path)
}

fn cleanup_stale_voice_cache(config_dir: &Path, max_age: Duration) -> Result<usize, String> {
    let cache_dir = config_dir.join("voice-cache");
    if !cache_dir.exists() {
        return Ok(0);
    }
    ensure_private_directory(&cache_dir)?;
    let now = SystemTime::now();
    let mut removed = 0usize;
    for entry in fs::read_dir(&cache_dir).map_err(|e| format!("read voice cache: {e}"))? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("utterance-") || !name.ends_with(".wav") {
            continue;
        }
        let stale = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= max_age);
        if stale && fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(unix)]
fn ensure_private_directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(path).map_err(|e| format!("create voice cache: {e}"))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("protect voice cache directory: {e}"))
}

#[cfg(not(unix))]
fn ensure_private_directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| format!("create voice cache: {e}"))
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> Result<fs::File, String> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("create private voice file: {e}"))
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> Result<fs::File, String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("create voice file: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_matcher_removes_kitt_prefix() {
        let matcher = WakePhraseMatcher::new(&["KITT".into(), "ei KITT".into()]);
        assert_eq!(
            matcher.strip_prefix("KITT, que horas são?"),
            Some("que horas são?".into())
        );
        assert_eq!(
            matcher.strip_prefix("Ei KITT abra o calendário"),
            Some("abra o calendário".into())
        );
        assert_eq!(matcher.strip_prefix("conversa normal"), None);
    }

    #[test]
    fn segmenter_emits_only_after_minimum_speech_and_silence() {
        let config = VoiceConfig {
            min_rms: 0.01,
            noise_multiplier: 2.0,
            pre_roll_ms: 10,
            min_speech_ms: 20,
            silence_ms: 20,
            max_utterance_ms: 500,
            ..VoiceConfig::default()
        };
        let mut segmenter = Segmenter::new(1_000, config);
        assert!(segmenter.push(&[0.2; 20]).is_none());
        let result = segmenter.push(&[0.0; 20]);
        assert!(result.is_some());
    }

    #[test]
    fn local_stt_supervisor_targets_openai_compatible_worker_endpoint() {
        assert_eq!(
            stt_health_url("http://127.0.0.1:8000/v1"),
            "http://127.0.0.1:8000/v1/health"
        );
        assert_eq!(
            local_worker_endpoint("http://127.0.0.1:8000/v1").unwrap(),
            ("127.0.0.1".into(), 8000)
        );
        assert!(local_worker_endpoint("http://192.168.1.10:8000/v1").is_err());
        assert!(local_worker_endpoint("http://127.0.0.1:8000/custom").is_err());
    }

    #[test]
    fn auto_mode_prefers_transcript_prefix_when_model_is_missing() {
        let config = VoiceConfig::default();
        let temp = std::env::temp_dir().join(format!(
            "kitt-voice-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp).unwrap();
        assert_eq!(
            config.resolved_mode(&temp),
            ActivationMode::TranscriptPrefix
        );
        let _ = fs::remove_dir_all(temp);
    }
}
