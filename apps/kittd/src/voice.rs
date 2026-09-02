use crate::{Runtime, ensure_hud, run_ask, settings_overlay};
use cpal::{
    FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use hound::{SampleFormat as WavSampleFormat, WavSpec, WavWriter};
use kitt_domain::RouteHint;
use kitt_protocol::{HudEvent, HudState};
use rustpotter::{
    AudioFmt, Rustpotter, RustpotterConfig, SampleFormat as PotterSampleFormat, VADMode,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs,
    fs::OpenOptions,
    io::{BufWriter, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const AUDIO_CHUNK_QUEUE: usize = 64;
const EVENT_QUEUE: usize = 8;
const WAKEWORD_KEY: &str = "kitt";
const CAPTURE_RESTART_MIN: Duration = Duration::from_secs(1);
const CAPTURE_RESTART_MAX: Duration = Duration::from_secs(30);
const STALE_AUDIO_MAX_AGE: Duration = Duration::from_secs(6 * 60 * 60);
const STT_HEALTH_MAX_BYTES: u64 = 64 * 1024;
const STT_START_POLL_INTERVAL: Duration = Duration::from_millis(250);
const DEFAULT_WAKE_PHRASES: &[&str] = &["kitt", "ei kitt", "hey kitt", "olá kitt"];
const LEGACY_CONTROL_CENTER_WAKE_PHRASES: &[&str] = &["kitt", "kit", "hey kitt", "ei kitt"];
const LEGACY_BROAD_WAKE_PHRASES: &[&str] = &[
    "kitt",
    "kit",
    "kite",
    "quit",
    "quitt",
    "hey kitt",
    "ei kitt",
    "ola kitt",
    "computador",
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActivationMode {
    #[default]
    Auto,
    Wakeword,
    TranscriptPrefix,
    Degraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VoiceState {
    #[default]
    Idle,
    Listening,
    Captured,
    SttWarming,
    Transcribing,
    Thinking,
    Speaking,
    Cooldown,
    Recovering,
    Degraded,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceTelemetry {
    pub state: VoiceState,
    pub state_since_ms: u64,
    pub activation_mode: String,
    pub stt_ready: bool,
    pub stt_busy: bool,
    pub last_wake_at_ms: Option<u64>,
    pub last_transcript_ms: u64,
    pub last_llm_ms: u64,
    pub last_tts_ms: u64,
    pub last_total_ms: u64,
    pub stt_restarts: u64,
    pub audio_chunks_dropped: u64,
    pub events_dropped: u64,
    pub utterances_dropped: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceConfig {
    pub enabled: bool,
    pub locale: String,
    pub activation_mode: ActivationMode,
    pub allow_transcript_prefix_fallback: bool,
    pub wakeword_model_path: Option<String>,
    pub wake_phrases: Vec<String>,
    pub wake_fuzzy_enabled: bool,
    pub wake_fuzzy_max_distance: u8,
    pub wake_cooldown_ms: u64,
    pub wake_threshold: f32,
    pub wake_avg_threshold: f32,
    pub wake_min_scores: usize,
    pub wake_eager: bool,
    pub wake_vad_mode: String,
    pub wake_gain_normalizer: bool,
    pub wake_gain_ref: Option<f32>,
    pub min_rms: f32,
    pub noise_multiplier: f32,
    pub speech_start_ms: u64,
    pub vad_release_ratio: f32,
    pub pre_roll_ms: u64,
    pub min_speech_ms: u64,
    pub silence_ms: u64,
    pub max_utterance_ms: u64,
    pub command_timeout_ms: u64,
    pub stt_autostart: bool,
    pub stt_worker_model: String,
    pub stt_connect_timeout_ms: u64,
    pub stt_request_timeout_ms: u64,
    pub stt_start_timeout_ms: u64,
    pub stt_warm_strategy: String,
    pub stt_idle_shutdown_seconds: u64,
    pub stt_device: String,
    pub stt_compute_type: String,
    pub stt_cpu_threads: usize,
    pub stt_num_workers: usize,
    pub stt_beam_size: usize,
    pub stt_local_files_only: bool,
    pub stt_vad_filter: bool,
    pub stt_vad_min_silence_ms: u64,
    pub stt_vad_speech_pad_ms: u64,
    pub stt_no_speech_threshold: f32,
    pub voice_llm_timeout_ms: u64,
    pub tts_enabled: bool,
    pub tts_backend: String,
    pub tts_voice_name: Option<String>,
    pub tts_prefer_male: bool,
    pub tts_rate: i32,
    pub tts_pitch: i32,
    pub tts_volume: u8,
    pub tts_timeout_ms: u64,
    pub tts_piper_base_url: Option<String>,
    pub tts_piper_voice: Option<String>,
    pub tts_piper_speaker: Option<i32>,
    pub tts_piper_length_scale: Option<f32>,
    pub tts_piper_noise_scale: Option<f32>,
    pub tts_piper_noise_w_scale: Option<f32>,
    pub tts_fallback_to_system: bool,
    pub echo_guard_ms: u64,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            locale: "pt-BR".into(),
            activation_mode: ActivationMode::Auto,
            allow_transcript_prefix_fallback: false,
            wakeword_model_path: Some("wakewords/kitt.rpw".into()),
            wake_phrases: DEFAULT_WAKE_PHRASES
                .iter()
                .map(|value| (*value).into())
                .collect(),
            wake_fuzzy_enabled: true,
            wake_fuzzy_max_distance: 1,
            wake_cooldown_ms: 1_200,
            wake_threshold: 0.50,
            wake_avg_threshold: 0.20,
            wake_min_scores: 5,
            wake_eager: false,
            wake_vad_mode: "off".into(),
            wake_gain_normalizer: false,
            wake_gain_ref: None,
            min_rms: 0.008,
            noise_multiplier: 2.2,
            speech_start_ms: 80,
            vad_release_ratio: 0.65,
            pre_roll_ms: 350,
            min_speech_ms: 180,
            silence_ms: 500,
            max_utterance_ms: 12_000,
            command_timeout_ms: 8_000,
            stt_autostart: true,
            stt_worker_model: String::new(),
            stt_connect_timeout_ms: 1_000,
            stt_request_timeout_ms: 15_000,
            stt_start_timeout_ms: 30_000,
            stt_warm_strategy: "on_wake".into(),
            stt_idle_shutdown_seconds: 300,
            stt_device: "auto".into(),
            stt_compute_type: "default".into(),
            stt_cpu_threads: 0,
            stt_num_workers: 1,
            stt_beam_size: 2,
            stt_local_files_only: true,
            stt_vad_filter: true,
            stt_vad_min_silence_ms: 300,
            stt_vad_speech_pad_ms: 220,
            stt_no_speech_threshold: 0.6,
            voice_llm_timeout_ms: 30_000,
            tts_enabled: true,
            tts_backend: "system".into(),
            tts_voice_name: None,
            tts_prefer_male: true,
            tts_rate: -1,
            tts_pitch: -2,
            tts_volume: 95,
            tts_timeout_ms: 30_000,
            tts_piper_base_url: Some("http://127.0.0.1:5000".into()),
            tts_piper_voice: None,
            tts_piper_speaker: None,
            tts_piper_length_scale: None,
            tts_piper_noise_scale: None,
            tts_piper_noise_w_scale: None,
            tts_fallback_to_system: true,
            echo_guard_ms: 350,
        }
    }
}

impl VoiceConfig {
    pub fn load_or_create(config_dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(config_dir).map_err(|e| format!("create voice config dir: {e}"))?;
        let path = config_dir.join("voice.json");
        if path.exists() {
            let mut config: Self = serde_json::from_str(
                &fs::read_to_string(&path).map_err(|e| format!("read voice.json: {e}"))?,
            )
            .map_err(|e| format!("parse voice.json: {e}"))?;
            let migrated = config.migrate_known_broad_wake_default();
            config.validate()?;
            if migrated {
                write_voice_config(&path, &config)?;
            }
            return Ok(config);
        }

        let config = Self::default();
        config.validate()?;
        write_voice_config(&path, &config)?;
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
        if self.wake_phrases.len() > 16
            || self
                .wake_phrases
                .iter()
                .any(|phrase| phrase.chars().count() > 64)
        {
            return Err("voice wake phrases exceed safe limits".into());
        }
        if self.wake_fuzzy_max_distance > 2 {
            return Err("voice wake_fuzzy_max_distance must be between 0 and 2".into());
        }
        if self.wake_cooldown_ms > 10_000 {
            return Err("voice wake_cooldown_ms must be <= 10000".into());
        }
        if !(0.01..=1.0).contains(&self.wake_threshold) {
            return Err("voice wake_threshold must be between 0.01 and 1.0".into());
        }
        if !(0.01..=1.0).contains(&self.wake_avg_threshold) {
            return Err("voice wake_avg_threshold must be between 0.01 and 1.0".into());
        }
        if self.wake_min_scores == 0 || self.wake_min_scores > 50 {
            return Err("voice wake_min_scores must be between 1 and 50".into());
        }
        if !(20..=500).contains(&self.speech_start_ms) {
            return Err("voice speech_start_ms must be between 20 and 500".into());
        }
        if !(0.30..=1.0).contains(&self.vad_release_ratio) {
            return Err("voice vad_release_ratio must be between 0.30 and 1.0".into());
        }
        if self.min_speech_ms == 0
            || self.silence_ms == 0
            || self.max_utterance_ms <= self.min_speech_ms
            || self.command_timeout_ms == 0
        {
            return Err("invalid voice timing configuration".into());
        }
        if !(100..=30_000).contains(&self.stt_connect_timeout_ms) {
            return Err("voice stt_connect_timeout_ms must be between 100 and 30000".into());
        }
        if !(1_000..=120_000).contains(&self.stt_request_timeout_ms) {
            return Err("voice stt_request_timeout_ms must be between 1000 and 120000".into());
        }
        if !(1_000..=300_000).contains(&self.stt_start_timeout_ms) {
            return Err("voice stt_start_timeout_ms must be between 1000 and 300000".into());
        }
        if !(1_000..=300_000).contains(&self.voice_llm_timeout_ms) {
            return Err("voice voice_llm_timeout_ms must be between 1000 and 300000".into());
        }
        if !(1_000..=120_000).contains(&self.tts_timeout_ms) {
            return Err("voice tts_timeout_ms must be between 1000 and 120000".into());
        }
        Ok(())
    }

    fn migrate_known_broad_wake_default(&mut self) -> bool {
        let normalized: Vec<String> = self
            .wake_phrases
            .iter()
            .map(|phrase| normalize_token(phrase))
            .collect();
        let broad_legacy: Vec<String> = LEGACY_BROAD_WAKE_PHRASES
            .iter()
            .map(|phrase| normalize_token(phrase))
            .collect();
        let control_center_legacy: Vec<String> = LEGACY_CONTROL_CENTER_WAKE_PHRASES
            .iter()
            .map(|phrase| normalize_token(phrase))
            .collect();
        if normalized == broad_legacy || normalized == control_center_legacy {
            self.wake_phrases = DEFAULT_WAKE_PHRASES
                .iter()
                .map(|value| (*value).to_string())
                .collect();
            true
        } else {
            false
        }
    }

    fn resolved_stt_worker_model(&self) -> Option<String> {
        if let Some(model) = nonempty_owned(&self.stt_worker_model) {
            return Some(model);
        }
        for name in ["KITT_WHISPER_MODEL", "WHISPER_MODEL"] {
            if let Ok(value) = std::env::var(name) {
                if let Some(model) = nonempty_owned(&value) {
                    return Some(model);
                }
            }
        }
        None
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
                } else if self.allow_transcript_prefix_fallback {
                    ActivationMode::TranscriptPrefix
                } else {
                    ActivationMode::Degraded
                }
            }
            ActivationMode::Degraded => ActivationMode::Degraded,
            mode => mode,
        }
    }
}

fn write_voice_config(path: &Path, config: &VoiceConfig) -> Result<(), String> {
    fs::write(
        path,
        serde_json::to_string_pretty(config).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("write voice.json: {e}"))
}

fn nonempty_owned(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActivationId(pub u64);

#[allow(dead_code)]
#[derive(Debug)]
enum CaptureEvent {
    WakeDetected {
        activation_id: ActivationId,
        at: Instant,
    },
    Utterance {
        activation_id: Option<ActivationId>,
        captured_at: Instant,
        path: PathBuf,
    },
}

#[allow(dead_code)]
pub struct VoiceRuntimeTracker {
    state: Mutex<VoiceState>,
    state_since: Mutex<Instant>,
    activation_mode: Mutex<String>,
    last_wake_at: Mutex<Option<Instant>>,
    last_transcript_ms: AtomicU64,
    last_llm_ms: AtomicU64,
    last_tts_ms: AtomicU64,
    last_total_ms: AtomicU64,
    stt_restarts: AtomicU64,
    audio_chunks_dropped: AtomicU64,
    events_dropped: AtomicU64,
    utterances_dropped: AtomicU64,
    last_error: Mutex<Option<String>>,
    last_spoken: Mutex<Option<String>>,
}

#[allow(dead_code)]
impl VoiceRuntimeTracker {
    pub fn new(mode: &str) -> Self {
        Self {
            state: Mutex::new(VoiceState::Idle),
            state_since: Mutex::new(Instant::now()),
            activation_mode: Mutex::new(mode.to_string()),
            last_wake_at: Mutex::new(None),
            last_transcript_ms: AtomicU64::new(0),
            last_llm_ms: AtomicU64::new(0),
            last_tts_ms: AtomicU64::new(0),
            last_total_ms: AtomicU64::new(0),
            stt_restarts: AtomicU64::new(0),
            audio_chunks_dropped: AtomicU64::new(0),
            events_dropped: AtomicU64::new(0),
            utterances_dropped: AtomicU64::new(0),
            last_error: Mutex::new(None),
            last_spoken: Mutex::new(None),
        }
    }

    pub fn set_state(&self, state: VoiceState) {
        let mut s = self.state.lock().unwrap();
        *s = state;
        let mut since = self.state_since.lock().unwrap();
        *since = Instant::now();
    }

    pub fn record_wake(&self) {
        let mut wake = self.last_wake_at.lock().unwrap();
        *wake = Some(Instant::now());
    }

    pub fn record_transcript(&self, ms: u64) {
        self.last_transcript_ms.store(ms, Ordering::Relaxed);
    }

    pub fn record_llm(&self, ms: u64) {
        self.last_llm_ms.store(ms, Ordering::Relaxed);
    }

    pub fn record_tts(&self, ms: u64) {
        self.last_tts_ms.store(ms, Ordering::Relaxed);
    }

    pub fn record_total(&self, ms: u64) {
        self.last_total_ms.store(ms, Ordering::Relaxed);
    }

    pub fn inc_stt_restarts(&self) {
        self.stt_restarts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_chunks_dropped(&self) {
        self.audio_chunks_dropped.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_events_dropped(&self) {
        self.events_dropped.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_utterances_dropped(&self) {
        self.utterances_dropped.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_error(&self, err: Option<String>) {
        let mut last_err = self.last_error.lock().unwrap();
        *last_err = err;
    }

    pub fn set_last_spoken(&self, text: String) {
        let mut s = self.last_spoken.lock().unwrap();
        *s = Some(text);
    }

    pub fn get_last_spoken(&self) -> Option<String> {
        self.last_spoken.lock().unwrap().clone()
    }

    pub fn snapshot(&self, stt_ready: bool, stt_busy: bool) -> VoiceTelemetry {
        let state = *self.state.lock().unwrap();
        let state_since = self.state_since.lock().unwrap().elapsed().as_millis() as u64;
        let mode = self.activation_mode.lock().unwrap().clone();
        let last_wake = self.last_wake_at.lock().unwrap().map(|t| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .saturating_sub(t.elapsed().as_millis()) as u64
        });
        let last_err = self.last_error.lock().unwrap().clone();

        VoiceTelemetry {
            state,
            state_since_ms: state_since,
            activation_mode: mode,
            stt_ready,
            stt_busy,
            last_wake_at_ms: last_wake,
            last_transcript_ms: self.last_transcript_ms.load(Ordering::Relaxed),
            last_llm_ms: self.last_llm_ms.load(Ordering::Relaxed),
            last_tts_ms: self.last_tts_ms.load(Ordering::Relaxed),
            last_total_ms: self.last_total_ms.load(Ordering::Relaxed),
            stt_restarts: self.stt_restarts.load(Ordering::Relaxed),
            audio_chunks_dropped: self.audio_chunks_dropped.load(Ordering::Relaxed),
            events_dropped: self.events_dropped.load(Ordering::Relaxed),
            utterances_dropped: self.utterances_dropped.load(Ordering::Relaxed),
            last_error: last_err,
        }
    }
}

pub fn start(runtime: Arc<Runtime>, config_dir: &Path) -> Result<(), String> {
    let mut config = VoiceConfig::load_or_create(config_dir)?;
    settings_overlay::apply_voice(config_dir, &mut config)?;
    config.migrate_known_broad_wake_default();
    config.validate()?;
    if !config.enabled {
        eprintln!("kitt voice disabled by voice.json");
        return Ok(());
    }

    let mode = config.resolved_mode(config_dir);
    let tracker = Arc::new(VoiceRuntimeTracker::new(mode_name(mode)));

    if mode == ActivationMode::Degraded {
        eprintln!(
            "kitt voice: wakeword model not found ({:?}) and transcript fallback is disabled (allow_transcript_prefix_fallback=false). Voice runtime is in degraded/idle mode.",
            config
                .wakeword_path(config_dir)
                .map(|p| p.display().to_string())
        );
        tracker.set_state(VoiceState::Degraded);
        tracker.set_error(Some(
            "Modelo wakeword ausente; configure .rpw ou habilite fallback explicitamente.".into(),
        ));
        show_voice_error(
            &runtime,
            "Wakeword ausente; configure o modelo .rpw no Control Center",
        );
        return Ok(());
    }

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

    // If warmup strategy is startup, start worker early
    if config.stt_warm_strategy == "startup" && runtime.service.transcriber_is_local() {
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
    let capture_tracker = tracker.clone();
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
                    capture_tracker.clone(),
                );
                match result {
                    Ok(()) => eprintln!("kitt voice capture stream ended; reopening microphone"),
                    Err(error) => {
                        eprintln!("kitt voice capture failed: {error}; reopening microphone");
                        capture_tracker.set_error(Some(format!("microfone: {error}")));
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
    let pipeline_tracker = tracker;
    thread::Builder::new()
        .name("kitt-voice-pipeline".into())
        .spawn(move || {
            pipeline_loop(
                pipeline_runtime,
                pipeline_config,
                mode,
                paused,
                events_rx,
                pipeline_tracker,
            )
        })
        .map_err(|e| format!("spawn voice pipeline: {e}"))?;

    eprintln!("kitt voice enabled ({})", mode_name(mode));
    Ok(())
}

fn mode_name(mode: ActivationMode) -> &'static str {
    match mode {
        ActivationMode::Auto => "auto",
        ActivationMode::Wakeword => "wakeword",
        ActivationMode::TranscriptPrefix => "transcript_prefix",
        ActivationMode::Degraded => "degraded",
    }
}

fn capture_loop(
    config: VoiceConfig,
    config_dir: PathBuf,
    mode: ActivationMode,
    paused: Arc<AtomicBool>,
    events: SyncSender<CaptureEvent>,
    tracker: Arc<VoiceRuntimeTracker>,
) -> Result<(), String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "no default microphone/input device available".to_string())?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("default microphone config: {e}"))?;
    let sample_format = supported.sample_format();
    let stream_config: StreamConfig = supported.into();
    let sample_rate = stream_config.sample_rate as usize;
    let channels = stream_config.channels as usize;
    if sample_rate == 0 || channels == 0 {
        return Err("invalid audio stream parameters".into());
    }

    let mut potter = if mode == ActivationMode::Wakeword {
        let wakeword = config
            .wakeword_path(&config_dir)
            .ok_or_else(|| "wakeword activation requires wakeword_model_path".to_string())?;
        let mut potter_config = RustpotterConfig::default();
        potter_config.detector.threshold = config.wake_threshold;
        potter_config.detector.avg_threshold = config.wake_avg_threshold;
        potter_config.detector.min_scores = config.wake_min_scores;
        potter_config.detector.eager = config.wake_eager;
        potter_config.filters.gain_normalizer.enabled = config.wake_gain_normalizer;
        potter_config.filters.gain_normalizer.gain_ref = config.wake_gain_ref;
        potter_config.detector.vad_mode = match config.wake_vad_mode.to_lowercase().as_str() {
            "easy" => Some(VADMode::Easy),
            "medium" => Some(VADMode::Medium),
            "hard" => Some(VADMode::Hard),
            _ => None,
        };

        potter_config.fmt = AudioFmt {
            sample_rate,
            sample_format: PotterSampleFormat::F32,
            channels: channels as u16,
            endianness: rustpotter::Endianness::Little,
        };
        let mut detector = Rustpotter::new(&potter_config).map_err(|e| e.to_string())?;
        let wakeword_str = wakeword
            .to_str()
            .ok_or_else(|| "invalid wakeword path".to_string())?;
        detector
            .add_wakeword_from_file(WAKEWORD_KEY, wakeword_str)
            .map_err(|e| e.to_string())?;
        Some(detector)
    } else {
        None
    };

    let (audio_tx, audio_rx) = mpsc::sync_channel::<Vec<f32>>(AUDIO_CHUNK_QUEUE);
    let error_signal = Arc::new(AtomicBool::new(false));
    let err_cb = {
        let error_signal = error_signal.clone();
        move |err| {
            eprintln!("cpal voice stream error: {err}");
            error_signal.store(true, Ordering::Release);
        }
    };

    let stream = match sample_format {
        SampleFormat::F32 => {
            build_input_stream::<f32, _>(&device, &stream_config, audio_tx, err_cb)?
        }
        SampleFormat::I16 => {
            build_input_stream::<i16, _>(&device, &stream_config, audio_tx, err_cb)?
        }
        SampleFormat::U16 => {
            build_input_stream::<u16, _>(&device, &stream_config, audio_tx, err_cb)?
        }
        other => return Err(format!("unsupported audio sample format: {other:?}")),
    };

    stream
        .play()
        .map_err(|e| format!("start audio capture: {e}"))?;

    let mut segmenter = Segmenter::new(sample_rate, config.clone());
    let mut mono_buffer = Vec::new();
    let mut last_wake = Instant::now() - Duration::from_millis(config.wake_cooldown_ms + 1);
    let mut activation_counter = 0u64;
    let mut active_activation: Option<(ActivationId, Instant)> = None;
    let frame_size = potter
        .as_ref()
        .map(|p| p.get_samples_per_frame())
        .unwrap_or(0);
    let mut potter_buffer = Vec::new();

    while let Ok(chunk) = audio_rx.recv() {
        if error_signal.load(Ordering::Acquire) {
            return Err("audio stream encountered an error".into());
        }
        if paused.load(Ordering::Acquire) {
            segmenter.reset();
            continue;
        }

        mono_buffer.clear();
        if channels == 1 {
            mono_buffer.extend_from_slice(&chunk);
        } else {
            mono_buffer.reserve(chunk.len() / channels);
            for frame in chunk.chunks_exact(channels) {
                let sum: f32 = frame.iter().copied().sum();
                mono_buffer.push(sum / channels as f32);
            }
        }

        if let Some(potter) = potter.as_mut() {
            potter_buffer.extend_from_slice(&chunk);
            while potter_buffer.len() >= frame_size && frame_size > 0 {
                let frame: Vec<f32> = potter_buffer.drain(..frame_size).collect();
                if let Some(detection) = potter.process_samples(frame) {
                    let now = Instant::now();
                    if now.duration_since(last_wake).as_millis() as u64 >= config.wake_cooldown_ms {
                        last_wake = now;
                        activation_counter = activation_counter.wrapping_add(1);
                        let activation_id = ActivationId(activation_counter);
                        active_activation = Some((activation_id, now));
                        tracker.record_wake();

                        eprintln!(
                            "kitt voice: wake detected ({}, score={:.2})",
                            detection.name, detection.score
                        );
                        if let Err(TrySendError::Full(_)) =
                            events.try_send(CaptureEvent::WakeDetected {
                                activation_id,
                                at: now,
                            })
                        {
                            tracker.inc_events_dropped();
                        }
                    }
                }
            }
        }

        if let Some(samples) = segmenter.push(&mono_buffer) {
            let captured_at = Instant::now();
            let activation_id = match active_activation {
                Some((id, wake_time)) => {
                    let elapsed = wake_time.elapsed().as_millis() as u64;
                    if elapsed <= config.command_timeout_ms {
                        Some(id)
                    } else {
                        None
                    }
                }
                None => None,
            };

            match write_wav(&config_dir, sample_rate, &samples) {
                Ok(path) => {
                    if let Err(TrySendError::Full(_)) = events.try_send(CaptureEvent::Utterance {
                        activation_id,
                        captured_at,
                        path,
                    }) {
                        tracker.inc_utterances_dropped();
                    }
                }
                Err(error) => eprintln!("failed to write utterance WAV: {error}"),
            }
        }
    }

    Ok(())
}

fn build_input_stream<T, E>(
    device: &cpal::Device,
    config: &StreamConfig,
    sender: SyncSender<Vec<f32>>,
    error_cb: E,
) -> Result<Stream, String>
where
    T: Sample + SizedSample + FromSample<T>,
    f32: FromSample<T>,
    E: FnMut(cpal::Error) + Send + 'static,
{
    device
        .build_input_stream(
            *config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                let converted: Vec<f32> = data.iter().copied().map(f32::from_sample).collect();
                let _ = sender.try_send(converted);
            },
            error_cb,
            None,
        )
        .map_err(|e| format!("build audio input stream: {e}"))
}

pub enum LocalSttProbe {
    Ready,
    Degraded(String),
    Unreachable(String),
}

pub fn probe_local_stt(base_url: &str) -> LocalSttProbe {
    let health_url = stt_health_url(base_url);
    let client = match reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_millis(1_000))
        .timeout(Duration::from_millis(2_000))
        .build()
    {
        Ok(client) => client,
        Err(e) => return LocalSttProbe::Unreachable(e.to_string()),
    };

    let mut response = match client.get(&health_url).send() {
        Ok(res) => res,
        Err(e) => return LocalSttProbe::Unreachable(e.to_string()),
    };

    let status = response.status();
    let mut body = Vec::new();
    if let Err(e) = response
        .by_ref()
        .take(STT_HEALTH_MAX_BYTES)
        .read_to_end(&mut body)
    {
        return LocalSttProbe::Degraded(format!("read health body: {e}"));
    }
    let json: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return LocalSttProbe::Degraded(format!("parse health JSON: {e}")),
    };

    if status.is_success() && json.get("ready").and_then(serde_json::Value::as_bool) == Some(true) {
        LocalSttProbe::Ready
    } else {
        let msg = json
            .get("engine")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("engine unavailable");
        LocalSttProbe::Degraded(format!("{status}: {msg}"))
    }
}

pub fn stt_health_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        format!("{trimmed}/health")
    } else {
        format!("{trimmed}/v1/health")
    }
}

pub fn local_worker_endpoint(base_url: &str) -> Result<(String, u16), String> {
    let parsed = reqwest::Url::parse(base_url).map_err(|e| format!("invalid STT URL: {e}"))?;
    if parsed.scheme() != "http" {
        return Err("local STT worker auto-start requires http:// scheme".into());
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
                candidates.push((
                    path.to_string_lossy().to_string(),
                    vec!["-m".into(), "kitt_workers.stt_server".into()],
                ));
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

    let model = config.resolved_stt_worker_model().ok_or_else(|| {
        "local STT autostart has no model configured; set assistant.voice.stt_worker_model, KITT_WHISPER_MODEL or WHISPER_MODEL"
            .to_string()
    })?;
    let mut common_args = vec![
        "--host".to_string(),
        host,
        "--port".to_string(),
        port.to_string(),
        "--model".to_string(),
        model,
        "--device".to_string(),
        config.stt_device.clone(),
        "--compute-type".to_string(),
        config.stt_compute_type.clone(),
        "--beam-size".to_string(),
        config.stt_beam_size.to_string(),
        "--num-workers".to_string(),
        config.stt_num_workers.to_string(),
        "--parent-stdin-lifecycle".to_string(),
    ];
    if config.stt_cpu_threads > 0 {
        common_args.push("--cpu-threads".to_string());
        common_args.push(config.stt_cpu_threads.to_string());
    }
    if config.stt_local_files_only {
        common_args.push("--local-files-only".to_string());
    } else {
        common_args.push("--allow-download".to_string());
    }

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

pub fn ensure_local_stt_ready(runtime: &Arc<Runtime>, config: &VoiceConfig) -> Result<(), String> {
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

fn trigger_async_warmup(runtime: Arc<Runtime>, config: VoiceConfig) {
    static WARMUP_RUNNING: AtomicBool = AtomicBool::new(false);
    if WARMUP_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::Builder::new()
        .name("kitt-stt-warmup".into())
        .spawn(move || {
            let _ = ensure_local_stt_ready(&runtime, &config);
            WARMUP_RUNNING.store(false, Ordering::SeqCst);
        })
        .ok();
}

fn show_voice_error(runtime: &Arc<Runtime>, message: &str) {
    ensure_hud(runtime);
    runtime.hud.send(HudEvent::Text {
        content: format!("KITT Voice: {message}"),
        ttl_ms: 10_000,
    });
}

#[derive(Debug, PartialEq, Eq)]
enum LocalInstantCommand {
    Cancel,
    VoiceStatus,
    Repeat,
    None,
}

fn detect_instant_command(text: &str) -> LocalInstantCommand {
    let lower = text.trim().to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "pare" | "parar" | "cancelar" | "cancele" | "silêncio" | "silencio" | "stop"
    ) {
        return LocalInstantCommand::Cancel;
    }
    if matches!(
        lower.as_str(),
        "status da voz" | "status voz" | "status do sistema" | "status"
    ) {
        return LocalInstantCommand::VoiceStatus;
    }
    if matches!(
        lower.as_str(),
        "repita" | "repita a resposta" | "o que você disse" | "repetir"
    ) {
        return LocalInstantCommand::Repeat;
    }
    LocalInstantCommand::None
}

fn pipeline_loop(
    runtime: Arc<Runtime>,
    config: VoiceConfig,
    mode: ActivationMode,
    paused: Arc<AtomicBool>,
    events: Receiver<CaptureEvent>,
    tracker: Arc<VoiceRuntimeTracker>,
) {
    let matcher = WakePhraseMatcher::new(
        &config.wake_phrases,
        config.wake_fuzzy_enabled,
        config.wake_fuzzy_max_distance,
    );
    let activation_prompt = matcher.prompt_hint().to_string();
    let mut awaiting_activation: Option<ActivationId> = None;

    while let Ok(event) = events.recv() {
        match event {
            CaptureEvent::WakeDetected {
                activation_id,
                at: _,
            } => {
                awaiting_activation = Some(activation_id);
                tracker.set_state(VoiceState::Listening);
                show_listening(&runtime, config.command_timeout_ms);

                // Asynchronous non-blocking warmup on wake
                if runtime.service.transcriber_is_local() {
                    trigger_async_warmup(runtime.clone(), config.clone());
                }
            }
            CaptureEvent::Utterance {
                activation_id,
                captured_at: _,
                path,
            } => {
                let _audio = TempAudioGuard(path.clone());
                paused.store(true, Ordering::Release);
                let _pause = PauseReset(paused.clone());

                let was_waiting = match (awaiting_activation, activation_id) {
                    (Some(expected), Some(actual)) => expected == actual,
                    _ => false,
                };
                awaiting_activation = None;

                let activation_probe = matches!(
                    mode,
                    ActivationMode::TranscriptPrefix | ActivationMode::Auto
                ) && !was_waiting;

                // Ensure local STT worker is ready before transcribing
                tracker.set_state(VoiceState::SttWarming);
                if runtime.service.transcriber_is_local() {
                    if let Err(error) = ensure_local_stt_ready(&runtime, &config) {
                        tracker.set_state(VoiceState::Recovering);
                        tracker.inc_stt_restarts();
                        tracker.set_error(Some(format!("stt startup: {error}")));
                        show_voice_error(&runtime, &format!("STT local indisponível: {error}"));
                        tracker.set_state(VoiceState::Idle);
                        continue;
                    }
                }

                tracker.set_state(VoiceState::Transcribing);
                let t_start = Instant::now();
                let transcription_res = runtime.service.transcribe_rich(
                    &path,
                    Some(config.locale.as_str()),
                    activation_probe.then_some(activation_prompt.as_str()),
                );
                let t_duration = t_start.elapsed().as_millis() as u64;
                tracker.record_transcript(t_duration);

                let transcript_obj = match transcription_res {
                    Ok(res) => res,
                    Err(error) => {
                        tracker.set_state(VoiceState::Recovering);
                        tracker.inc_stt_restarts();
                        tracker.set_error(Some(format!("transcription: {error}")));
                        stop_owned_stt_worker(&runtime);
                        show_voice_error(&runtime, &format!("Reconhecimento reiniciado: {error}"));
                        tracker.set_state(VoiceState::Idle);
                        continue;
                    }
                };

                let transcript = transcript_obj.text.trim().to_string();
                if transcript.is_empty() {
                    tracker.set_state(VoiceState::Idle);
                    continue;
                }

                eprintln!("kitt voice heard transcript: {:?}", transcript);

                let command = match mode {
                    ActivationMode::Wakeword => {
                        if was_waiting {
                            matcher
                                .strip_prefix(&transcript)
                                .unwrap_or_else(|| transcript.clone())
                        } else {
                            tracker.set_state(VoiceState::Idle);
                            continue;
                        }
                    }
                    ActivationMode::TranscriptPrefix | ActivationMode::Auto => {
                        if was_waiting {
                            transcript.clone()
                        } else if let Some(command) = matcher.strip_prefix(&transcript) {
                            command
                        } else {
                            eprintln!("kitt voice: no wake phrase matched in {:?}", transcript);
                            tracker.set_state(VoiceState::Idle);
                            continue;
                        }
                    }
                    ActivationMode::Degraded => {
                        tracker.set_state(VoiceState::Degraded);
                        continue;
                    }
                };

                let command = command.trim();
                if command.is_empty() {
                    eprintln!("kitt voice: wake phrase matched! Listening for command...");
                    tracker.set_state(VoiceState::Listening);
                    show_listening(&runtime, config.command_timeout_ms);
                    continue;
                }

                // Check for local instant commands that bypass LLM
                match detect_instant_command(command) {
                    LocalInstantCommand::Cancel => {
                        eprintln!("kitt voice: instant command 'cancel' received");
                        ensure_hud(&runtime);
                        runtime.hud.send(HudEvent::Text {
                            content: "Cancelado.".into(),
                            ttl_ms: 3_000,
                        });
                        tracker.set_state(VoiceState::Idle);
                        continue;
                    }
                    LocalInstantCommand::VoiceStatus => {
                        eprintln!("kitt voice: instant command 'status da voz' received");
                        let status_msg = format!(
                            "KITT operacional. Modo {}, reconhecimento pronto.",
                            mode_name(mode)
                        );
                        ensure_hud(&runtime);
                        runtime.hud.send(HudEvent::Text {
                            content: status_msg.clone(),
                            ttl_ms: 5_000,
                        });
                        if config.tts_enabled {
                            tracker.set_state(VoiceState::Speaking);
                            let _ = runtime
                                .service
                                .speak(&status_msg, Some(config.locale.as_str()));
                        }
                        tracker.set_state(VoiceState::Idle);
                        continue;
                    }
                    LocalInstantCommand::Repeat => {
                        if let Some(prev) = tracker.get_last_spoken() {
                            eprintln!("kitt voice: repeating previous answer");
                            ensure_hud(&runtime);
                            runtime.hud.send(HudEvent::Text {
                                content: prev.clone(),
                                ttl_ms: 6_000,
                            });
                            if config.tts_enabled {
                                tracker.set_state(VoiceState::Speaking);
                                let _ = runtime.service.speak(&prev, Some(config.locale.as_str()));
                            }
                        }
                        tracker.set_state(VoiceState::Idle);
                        continue;
                    }
                    LocalInstantCommand::None => {}
                }

                eprintln!("kitt voice: executing command: {:?}", command);
                tracker.set_state(VoiceState::Thinking);
                ensure_hud(&runtime);
                runtime.hud.send(HudEvent::Status {
                    state: HudState::Thinking,
                    message: Some("Pensando…".into()),
                });

                let llm_start = Instant::now();
                let ask_res = run_ask(&runtime, command, RouteHint::Auto, true);
                let llm_duration = llm_start.elapsed().as_millis() as u64;
                tracker.record_llm(llm_duration);

                match ask_res {
                    Ok(answer) => {
                        eprintln!("kitt voice answer: {:?}", answer.text);
                        tracker.set_last_spoken(answer.text.clone());
                        if config.tts_enabled {
                            tracker.set_state(VoiceState::Speaking);
                            ensure_hud(&runtime);
                            runtime.hud.send(HudEvent::Text {
                                content: answer.text.clone(),
                                ttl_ms: 8_000,
                            });
                            let tts_start = Instant::now();
                            if let Err(error) = runtime
                                .service
                                .speak(&answer.text, Some(config.locale.as_str()))
                            {
                                eprintln!("voice TTS unavailable: {error}");
                                tracker.set_error(Some(format!("tts: {error}")));
                            }
                            let tts_duration = tts_start.elapsed().as_millis() as u64;
                            tracker.record_tts(tts_duration);
                        }
                        tracker.record_total(t_duration + llm_duration);
                    }
                    Err((_, error)) => {
                        eprintln!("voice assistant request failed: {error}");
                        tracker.set_error(Some(format!("llm: {error}")));
                    }
                }

                tracker.set_state(VoiceState::Cooldown);
                thread::sleep(Duration::from_millis(config.echo_guard_ms));
                tracker.set_state(VoiceState::Idle);
                eprintln!("kitt voice: ready and listening");
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
    prompt_hint: String,
    fuzzy_enabled: bool,
    max_distance: u8,
}

impl WakePhraseMatcher {
    fn new(phrases: &[String], fuzzy_enabled: bool, max_distance: u8) -> Self {
        let prompt_hint = phrases
            .iter()
            .map(|phrase| phrase.trim())
            .filter(|phrase| !phrase.is_empty())
            .collect::<Vec<_>>()
            .join(". ");
        let phrases = phrases
            .iter()
            .map(|phrase| normalize_tokens(phrase))
            .filter(|tokens| !tokens.is_empty())
            .collect();
        Self {
            phrases,
            prompt_hint,
            fuzzy_enabled,
            max_distance,
        }
    }

    fn prompt_hint(&self) -> &str {
        &self.prompt_hint
    }

    fn strip_prefix(&self, text: &str) -> Option<String> {
        let original: Vec<&str> = text.split_whitespace().collect();
        let normalized: Vec<String> = original
            .iter()
            .map(|token| normalize_token(token))
            .collect();

        for phrase in &self.phrases {
            if normalized.len() < phrase.len() {
                continue;
            }
            let candidate = &normalized[..phrase.len()];
            if candidate == phrase.as_slice()
                || (self.fuzzy_enabled && wake_phrase_matches(candidate, phrase, self.max_distance))
            {
                return Some(original[phrase.len()..].join(" "));
            }
        }
        None
    }
}

fn wake_phrase_matches(candidate: &[String], phrase: &[String], max_distance: u8) -> bool {
    if candidate.len() != phrase.len() || candidate.is_empty() {
        return false;
    }
    if candidate.len() > 1 && candidate[..candidate.len() - 1] != phrase[..phrase.len() - 1] {
        return false;
    }
    wake_token_matches(
        candidate.last().map(String::as_str).unwrap_or_default(),
        phrase.last().map(String::as_str).unwrap_or_default(),
        max_distance,
    )
}

fn wake_token_matches(candidate: &str, expected: &str, max_distance: u8) -> bool {
    if candidate == expected {
        return true;
    }
    if matches!(candidate, "quit" | "quitt" | "kite") {
        return false;
    }
    if candidate.len() < 2 || candidate.len() > 6 {
        return false;
    }
    let first = candidate.as_bytes().first().copied();
    if !matches!(first, Some(b'k') | Some(b'q')) {
        return false;
    }
    let candidate = wake_phonetic_key(candidate);
    let expected = wake_phonetic_key(expected);
    bounded_edit_distance(&candidate, &expected, usize::from(max_distance))
        .is_some_and(|distance| distance <= usize::from(max_distance))
}

fn wake_phonetic_key(token: &str) -> String {
    let normalized = normalize_token(token);
    let mut value = if let Some(stripped) = normalized.strip_prefix("qu") {
        format!("k{stripped}")
    } else {
        normalized
    };

    let mut collapsed = String::with_capacity(value.len());
    let mut previous = None;
    for ch in value.chars() {
        if previous != Some(ch) {
            collapsed.push(ch);
            previous = Some(ch);
        }
    }
    value = collapsed;

    if value.len() >= 4 && matches!(value.chars().last(), Some('e') | Some('i')) {
        value.pop();
    }
    value
}

fn bounded_edit_distance(left: &str, right: &str, max_distance: usize) -> Option<usize> {
    if left.len().abs_diff(right.len()) > max_distance {
        return None;
    }
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];

    for (row, left_byte) in left.bytes().enumerate() {
        current[0] = row + 1;
        for (column, right_byte) in right.bytes().enumerate() {
            let substitution = previous[column] + usize::from(left_byte != right_byte);
            let insertion = current[column] + 1;
            let deletion = previous[column + 1] + 1;
            current[column + 1] = substitution.min(insertion).min(deletion);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    let distance = previous[right.len()];
    (distance <= max_distance).then_some(distance)
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
        .map(|ch| match ch {
            'á' | 'à' | 'ã' | 'â' | 'ä' | 'Á' | 'À' | 'Ã' | 'Â' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
            'ó' | 'ò' | 'õ' | 'ô' | 'ö' | 'Ó' | 'Ò' | 'Õ' | 'Ô' | 'Ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            other => other.to_ascii_lowercase(),
        })
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

struct Segmenter {
    sample_rate: usize,
    config: VoiceConfig,
    noise_floor: f32,
    pre_roll: VecDeque<f32>,
    current: Vec<f32>,
    speaking: bool,
    candidate_voiced_samples: usize,
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
            candidate_voiced_samples: 0,
            speech_samples: 0,
            silence_samples: 0,
        }
    }

    fn push(&mut self, samples: &[f32]) -> Option<Vec<f32>> {
        if samples.is_empty() {
            return None;
        }
        let rms = rms(samples);
        let start_threshold = self
            .config
            .min_rms
            .max(self.noise_floor * self.config.noise_multiplier);
        let release_threshold = start_threshold * self.config.vad_release_ratio;
        let voiced = if self.speaking {
            rms >= release_threshold
        } else {
            rms >= start_threshold
        };

        if !self.speaking {
            self.extend_pre_roll(samples);
            if !voiced {
                self.candidate_voiced_samples = 0;
                let bounded_noise = rms.min(self.config.min_rms * 4.0);
                self.noise_floor = self.noise_floor * 0.985 + bounded_noise * 0.015;
                return None;
            }

            self.candidate_voiced_samples += samples.len();
            let attack = ms_to_samples(self.sample_rate, self.config.speech_start_ms);
            if self.candidate_voiced_samples < attack {
                return None;
            }

            self.speaking = true;
            self.current.extend(self.pre_roll.drain(..));
            self.speech_samples = self.candidate_voiced_samples;
            self.candidate_voiced_samples = 0;
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
        self.candidate_voiced_samples = 0;
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
        self.candidate_voiced_samples = 0;
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
    fn wake_matcher_is_phonetic_but_does_not_scan_arbitrary_offsets() {
        let matcher = WakePhraseMatcher::new(
            &["KITT".into(), "ei KITT".into(), "olá KITT".into()],
            true,
            1,
        );
        assert_eq!(
            matcher.strip_prefix("KITT, que horas são?"),
            Some("que horas são?".into())
        );
        assert_eq!(
            matcher.strip_prefix("Kit abra o calendário"),
            Some("abra o calendário".into())
        );
        assert_eq!(
            matcher.strip_prefix("Quite, qual é a temperatura?"),
            Some("qual é a temperatura?".into())
        );
        assert_eq!(
            matcher.strip_prefix("Ei kit abra o calendário"),
            Some("abra o calendário".into())
        );
        assert_eq!(matcher.strip_prefix("quit now"), None);
        assert_eq!(matcher.strip_prefix("computador abra a agenda"), None);
        assert_eq!(matcher.strip_prefix("agora KITT abra a agenda"), None);
        assert_eq!(matcher.strip_prefix("conversa normal"), None);
    }

    #[test]
    fn segmenter_emits_only_after_minimum_speech_and_silence() {
        let config = VoiceConfig {
            min_rms: 0.01,
            noise_multiplier: 2.0,
            speech_start_ms: 20,
            pre_roll_ms: 40,
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
    fn segmenter_release_hysteresis_keeps_quieter_speech_active() {
        let config = VoiceConfig {
            min_rms: 0.01,
            noise_multiplier: 2.0,
            speech_start_ms: 20,
            vad_release_ratio: 0.5,
            pre_roll_ms: 40,
            min_speech_ms: 20,
            silence_ms: 20,
            max_utterance_ms: 500,
            ..VoiceConfig::default()
        };
        let mut segmenter = Segmenter::new(1_000, config);
        assert!(segmenter.push(&[0.03; 20]).is_none());
        assert!(segmenter.push(&[0.008; 20]).is_none());
        assert!(segmenter.push(&[0.0; 20]).is_some());
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
    fn auto_mode_prefers_transcript_prefix_when_fallback_is_allowed() {
        let config = VoiceConfig {
            allow_transcript_prefix_fallback: true,
            ..VoiceConfig::default()
        };
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

    #[test]
    fn auto_mode_prefers_degraded_when_fallback_is_disallowed() {
        let config = VoiceConfig {
            allow_transcript_prefix_fallback: false,
            ..VoiceConfig::default()
        };
        let temp = std::env::temp_dir().join(format!(
            "kitt-voice-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp).unwrap();
        assert_eq!(config.resolved_mode(&temp), ActivationMode::Degraded);
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn detect_instant_commands_correctly() {
        assert_eq!(detect_instant_command("pare"), LocalInstantCommand::Cancel);
        assert_eq!(
            detect_instant_command("cancelar"),
            LocalInstantCommand::Cancel
        );
        assert_eq!(
            detect_instant_command("silêncio"),
            LocalInstantCommand::Cancel
        );
        assert_eq!(
            detect_instant_command("status da voz"),
            LocalInstantCommand::VoiceStatus
        );
        assert_eq!(
            detect_instant_command("repita"),
            LocalInstantCommand::Repeat
        );
        assert_eq!(
            detect_instant_command("qual é a previsão do tempo?"),
            LocalInstantCommand::None
        );
    }
}
