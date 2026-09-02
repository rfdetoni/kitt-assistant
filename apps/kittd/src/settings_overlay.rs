//! Merge KITT Control Center overrides into kittd native configuration.
// Target: apps/kittd/src/settings_overlay.rs

use crate::{
    Config,
    model_config::ModelProfiles,
    voice::{ActivationMode, VoiceConfig},
};
use serde_json::{Map, Value};
use std::{fs, path::Path};

const MAX_OVERLAY_BYTES: u64 = 2 * 1024 * 1024;

fn section(config_dir: &Path, id: &str) -> Result<Map<String, Value>, String> {
    let kitt_root = config_dir.parent().unwrap_or(config_dir);
    let path = std::env::var_os("KITT_CONTROL_CENTER_CONFIG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| kitt_root.join("control-center").join("overrides.json"));
    if !path.exists() {
        return Ok(Map::new());
    }
    let metadata =
        fs::metadata(&path).map_err(|e| format!("Control Center overlay metadata: {e}"))?;
    if metadata.len() > MAX_OVERLAY_BYTES {
        return Err("Control Center overlay exceeds 2 MiB".into());
    }
    let root: Value = serde_json::from_slice(
        &fs::read(&path).map_err(|e| format!("read Control Center overlay: {e}"))?,
    )
    .map_err(|e| format!("parse Control Center overlay: {e}"))?;
    if root.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err("unsupported Control Center overlay schema".into());
    }
    Ok(root
        .get("components")
        .and_then(Value::as_object)
        .and_then(|components| components.get(id))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default())
}

pub(crate) fn apply_core(config_dir: &Path, config: &mut Config) -> Result<(), String> {
    let values = section(config_dir, "assistant.core")?;
    if let Some(value) = values.get("listen").and_then(Value::as_str) {
        config.listen = value.to_string();
    }
    if let Some(value) = values.get("base_url").and_then(Value::as_str) {
        config.base_url = value.to_string();
    }
    if let Some(value) = values.get("model").and_then(Value::as_str) {
        config.model = value.to_string();
    }
    set_optional_string(&values, "api_key_env", &mut config.api_key_env);
    if let Some(value) = values.get("local_provider").and_then(Value::as_bool) {
        config.local_provider = value;
    }
    if let Some(value) = values.get("hud_ttl_ms").and_then(Value::as_u64) {
        config.hud_ttl_ms = value;
    }
    if let Some(value) = values.get("allow_personal_remote").and_then(Value::as_bool) {
        config.allow_personal_remote = value;
    }

    let voice = section(config_dir, "assistant.voice")?;
    set_optional_string(&voice, "tts_voice_name", &mut config.tts_voice_name);
    if let Some(value) = voice.get("tts_prefer_male").and_then(Value::as_bool) {
        config.tts_prefer_male = value;
    }
    set_i32(&voice, "tts_rate", &mut config.tts_rate)?;
    set_i32(&voice, "tts_pitch", &mut config.tts_pitch)?;
    set_u8(&voice, "tts_volume", &mut config.tts_volume)?;
    Ok(())
}

pub(crate) fn apply_models(config_dir: &Path, profiles: &mut ModelProfiles) -> Result<(), String> {
    let v = section(config_dir, "assistant.models")?;
    set_string(&v, "fast.base_url", &mut profiles.fast.base_url);
    set_string(&v, "fast.model", &mut profiles.fast.model);
    set_optional_string(&v, "fast.api_key_env", &mut profiles.fast.api_key_env);
    if let Some(x) = v.get("fast.local_provider").and_then(Value::as_bool) {
        profiles.fast.local_provider = x;
    }

    set_string(&v, "heavy.base_url", &mut profiles.heavy.base_url);
    set_string(&v, "heavy.model", &mut profiles.heavy.model);
    set_optional_string(&v, "heavy.api_key_env", &mut profiles.heavy.api_key_env);
    if let Some(x) = v.get("heavy.local_provider").and_then(Value::as_bool) {
        profiles.heavy.local_provider = x;
    }

    set_string(
        &v,
        "speech_to_text.base_url",
        &mut profiles.speech_to_text.base_url,
    );
    set_string(
        &v,
        "speech_to_text.model",
        &mut profiles.speech_to_text.model,
    );
    set_optional_string(
        &v,
        "speech_to_text.api_key_env",
        &mut profiles.speech_to_text.api_key_env,
    );
    if let Some(x) = v
        .get("speech_to_text.local_provider")
        .and_then(Value::as_bool)
    {
        profiles.speech_to_text.local_provider = x;
    }
    if let Some(x) = v
        .get("speech_to_text.allow_remote")
        .and_then(Value::as_bool)
    {
        profiles.speech_to_text.allow_remote = x;
    }
    profiles.migrate_known_broken_stt_default();
    if let Some(x) = v.get("fast_max_chars").and_then(Value::as_u64) {
        profiles.fast_max_chars = usize::try_from(x).map_err(|_| "fast_max_chars out of range")?;
    }
    if let Some(x) = v.get("fast_max_lines").and_then(Value::as_u64) {
        profiles.fast_max_lines = usize::try_from(x).map_err(|_| "fast_max_lines out of range")?;
    }
    Ok(())
}

pub(crate) fn apply_voice(config_dir: &Path, config: &mut VoiceConfig) -> Result<(), String> {
    let v = section(config_dir, "assistant.voice")?;
    if let Some(x) = v.get("enabled").and_then(Value::as_bool) {
        config.enabled = x;
    }
    set_string(&v, "locale", &mut config.locale);
    if let Some(x) = v.get("activation_mode").and_then(Value::as_str) {
        config.activation_mode = match x {
            "auto" => ActivationMode::Auto,
            "wakeword" => ActivationMode::Wakeword,
            "transcript_prefix" => ActivationMode::TranscriptPrefix,
            _ => return Err("invalid assistant.voice.activation_mode".into()),
        };
    }
    if let Some(items) = v.get("wake_phrases").and_then(Value::as_array) {
        let phrases: Vec<String> = items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        if !phrases.is_empty() {
            config.wake_phrases = phrases;
        }
    }
    if let Some(x) = v.get("wakeword_model_path").and_then(Value::as_str) {
        config.wakeword_model_path = if x.trim().is_empty() {
            None
        } else {
            Some(x.to_string())
        };
    }
    if let Some(x) = v.get("min_rms").and_then(Value::as_f64) {
        config.min_rms = x as f32;
    }
    if let Some(x) = v.get("noise_multiplier").and_then(Value::as_f64) {
        config.noise_multiplier = x as f32;
    }
    if let Some(x) = v.get("wake_fuzzy_enabled").and_then(Value::as_bool) {
        config.wake_fuzzy_enabled = x;
    }
    if let Some(x) = v.get("wake_fuzzy_max_distance").and_then(Value::as_u64) {
        config.wake_fuzzy_max_distance =
            u8::try_from(x).map_err(|_| "wake_fuzzy_max_distance out of range")?;
    }
    set_u64(&v, "wake_cooldown_ms", &mut config.wake_cooldown_ms);
    set_u64(&v, "speech_start_ms", &mut config.speech_start_ms);
    if let Some(x) = v.get("vad_release_ratio").and_then(Value::as_f64) {
        config.vad_release_ratio = x as f32;
    }
    set_u64(&v, "pre_roll_ms", &mut config.pre_roll_ms);
    set_u64(&v, "min_speech_ms", &mut config.min_speech_ms);
    set_u64(&v, "silence_ms", &mut config.silence_ms);
    set_u64(&v, "max_utterance_ms", &mut config.max_utterance_ms);
    set_u64(&v, "command_timeout_ms", &mut config.command_timeout_ms);
    if let Some(x) = v.get("stt_autostart").and_then(Value::as_bool) {
        config.stt_autostart = x;
    }
    set_string(&v, "stt_worker_model", &mut config.stt_worker_model);
    set_u64(&v, "stt_start_timeout_ms", &mut config.stt_start_timeout_ms);
    if let Some(x) = v.get("tts_enabled").and_then(Value::as_bool) {
        config.tts_enabled = x;
    }
    if let Some(x) = v
        .get("allow_transcript_prefix_fallback")
        .and_then(Value::as_bool)
    {
        config.allow_transcript_prefix_fallback = x;
    }
    if let Some(x) = v.get("wake_threshold").and_then(Value::as_f64) {
        config.wake_threshold = x as f32;
    }
    if let Some(x) = v.get("wake_avg_threshold").and_then(Value::as_f64) {
        config.wake_avg_threshold = x as f32;
    }
    if let Some(x) = v.get("wake_min_scores").and_then(Value::as_u64) {
        config.wake_min_scores = x as usize;
    }
    if let Some(x) = v.get("wake_eager").and_then(Value::as_bool) {
        config.wake_eager = x;
    }
    set_string(&v, "wake_vad_mode", &mut config.wake_vad_mode);
    if let Some(x) = v.get("wake_gain_normalizer").and_then(Value::as_bool) {
        config.wake_gain_normalizer = x;
    }
    if let Some(x) = v.get("wake_gain_ref").and_then(Value::as_f64) {
        config.wake_gain_ref = Some(x as f32);
    }
    set_u64(
        &v,
        "stt_connect_timeout_ms",
        &mut config.stt_connect_timeout_ms,
    );
    set_u64(
        &v,
        "stt_request_timeout_ms",
        &mut config.stt_request_timeout_ms,
    );
    set_string(&v, "stt_warm_strategy", &mut config.stt_warm_strategy);
    set_u64(
        &v,
        "stt_idle_shutdown_seconds",
        &mut config.stt_idle_shutdown_seconds,
    );
    set_string(&v, "stt_device", &mut config.stt_device);
    set_string(&v, "stt_compute_type", &mut config.stt_compute_type);
    if let Some(x) = v.get("stt_cpu_threads").and_then(Value::as_u64) {
        config.stt_cpu_threads = x as usize;
    }
    if let Some(x) = v.get("stt_num_workers").and_then(Value::as_u64) {
        config.stt_num_workers = x as usize;
    }
    if let Some(x) = v.get("stt_beam_size").and_then(Value::as_u64) {
        config.stt_beam_size = x as usize;
    }
    if let Some(x) = v.get("stt_local_files_only").and_then(Value::as_bool) {
        config.stt_local_files_only = x;
    }
    if let Some(x) = v.get("stt_vad_filter").and_then(Value::as_bool) {
        config.stt_vad_filter = x;
    }
    set_u64(
        &v,
        "stt_vad_min_silence_ms",
        &mut config.stt_vad_min_silence_ms,
    );
    set_u64(
        &v,
        "stt_vad_speech_pad_ms",
        &mut config.stt_vad_speech_pad_ms,
    );
    if let Some(x) = v.get("stt_no_speech_threshold").and_then(Value::as_f64) {
        config.stt_no_speech_threshold = x as f32;
    }
    set_u64(&v, "voice_llm_timeout_ms", &mut config.voice_llm_timeout_ms);
    set_string(&v, "tts_backend", &mut config.tts_backend);
    set_optional_string(&v, "tts_voice_name", &mut config.tts_voice_name);
    if let Some(x) = v.get("tts_prefer_male").and_then(Value::as_bool) {
        config.tts_prefer_male = x;
    }
    set_i32(&v, "tts_rate", &mut config.tts_rate)?;
    set_i32(&v, "tts_pitch", &mut config.tts_pitch)?;
    set_u8(&v, "tts_volume", &mut config.tts_volume)?;
    set_u64(&v, "tts_timeout_ms", &mut config.tts_timeout_ms);
    set_optional_string(&v, "tts_piper_base_url", &mut config.tts_piper_base_url);
    set_optional_string(&v, "tts_piper_voice", &mut config.tts_piper_voice);
    if let Some(x) = v.get("tts_piper_speaker").and_then(Value::as_i64) {
        config.tts_piper_speaker = Some(x as i32);
    }
    if let Some(x) = v.get("tts_piper_length_scale").and_then(Value::as_f64) {
        config.tts_piper_length_scale = Some(x as f32);
    }
    if let Some(x) = v.get("tts_piper_noise_scale").and_then(Value::as_f64) {
        config.tts_piper_noise_scale = Some(x as f32);
    }
    if let Some(x) = v.get("tts_piper_noise_w_scale").and_then(Value::as_f64) {
        config.tts_piper_noise_w_scale = Some(x as f32);
    }
    if let Some(x) = v.get("tts_fallback_to_system").and_then(Value::as_bool) {
        config.tts_fallback_to_system = x;
    }
    set_u64(&v, "echo_guard_ms", &mut config.echo_guard_ms);
    Ok(())
}

fn set_string(map: &Map<String, Value>, key: &str, target: &mut String) {
    if let Some(value) = map.get(key).and_then(Value::as_str) {
        *target = value.to_string();
    }
}
fn set_optional_string(map: &Map<String, Value>, key: &str, target: &mut Option<String>) {
    if let Some(value) = map.get(key).and_then(Value::as_str) {
        *target = if value.trim().is_empty() {
            None
        } else {
            Some(value.to_string())
        };
    }
}
fn set_u64(map: &Map<String, Value>, key: &str, target: &mut u64) {
    if let Some(value) = map.get(key).and_then(Value::as_u64) {
        *target = value;
    }
}

fn set_i32(map: &Map<String, Value>, key: &str, target: &mut i32) -> Result<(), String> {
    if let Some(value) = map.get(key).and_then(Value::as_i64) {
        *target = i32::try_from(value).map_err(|_| format!("{key} out of range"))?;
    }
    Ok(())
}

fn set_u8(map: &Map<String, Value>, key: &str, target: &mut u8) -> Result<(), String> {
    if let Some(value) = map.get(key).and_then(Value::as_u64) {
        *target = u8::try_from(value).map_err(|_| format!("{key} out of range"))?;
    }
    Ok(())
}
