# K.I.T.T. Assistant — Hands-free Voice

This implementation keeps the assistant daemon resident and keeps the expensive pieces out of the idle path.

## Runtime flow

```text
microphone
   |
   +--> local wake word (.rpw) ------------------+
   |                                             |
   +--> fallback VAD -> local STT -> "KITT ..." |
                                                 v
                                           command audio
                                                 |
                                                 v
                                                STT
                                                 |
                                                 v
                                           FAST/HEAVY router
                                                 |
                                                 v
                                                LLM
                                                 |
                                  +--------------+--------------+
                                  |                             |
                                  v                             v
                                HUD                         system TTS
```

No keyboard shortcut is required during normal use.

## Activation modes

`voice.json` is created under the normal KITT Assistant configuration directory.

### `auto` (default)

- If `wakeword_model_path` exists, use the local Rustpotter wake-word detector.
- Otherwise use VAD + local STT and require the transcript to start with a configured wake phrase.

### `wakeword` (recommended)

This is the lowest-resource and most privacy-preserving path. Only the wake-word detector processes the idle microphone stream. STT is invoked only after local activation.

Create a Rustpotter reference from 3–8 recordings of the word `KITT`:

```bash
cargo install rustpotter-cli --version 3.0.2
mkdir -p ~/.config/kitt/assistant/wakewords/samples

# Record 3–8 short WAV files with rustpotter-cli (one KITT utterance per file), then:
rustpotter-cli build \
  --model-name "kitt" \
  --model-path ~/.config/kitt/assistant/wakewords/kitt.rpw \
  ~/.config/kitt/assistant/wakewords/samples/*.wav
```

Example `voice.json`:

```json
{
  "enabled": true,
  "locale": "pt-BR",
  "activation_mode": "wakeword",
  "wakeword_model_path": "wakewords/kitt.rpw",
  "wake_phrases": ["kitt", "kit", "hey kitt", "ei kitt"],
  "min_rms": 0.015,
  "noise_multiplier": 3.0,
  "pre_roll_ms": 200,
  "min_speech_ms": 250,
  "silence_ms": 650,
  "max_utterance_ms": 12000,
  "command_timeout_ms": 7000,
  "tts_enabled": true,
  "echo_guard_ms": 350
}
```

With a local wake-word model, STT may be local or remote. Remote STT still requires `speech_to_text.allow_remote=true` in `models.json`.

### `transcript_prefix` (zero-enrollment fallback)

This path needs no wake-word model. Voice activity is recorded to a temporary private WAV, sent to the configured **local** STT backend, and ignored unless it begins with a wake phrase such as `KITT`.

For privacy, this mode refuses to start when STT is remote: pre-activation ambient speech must never be uploaded just to discover whether it contains the wake word.

## STT configuration

The Assistant uses the existing OpenAI-compatible `/audio/transcriptions` adapter.
The KITT local STT default is `http://127.0.0.1:8000/v1`; port `11434` is reserved
for Ollama/model traffic and is not used as the Whisper default.

When local STT is selected and unavailable, Voice can supervise a `kitt-stt`
worker automatically. Install the STT extra first:

```bash
cd kitt-ai-workers
python3 -m pip install -e ".[stt]"
```

`kitt-stt` is then started on demand. `KITT_STT_WORKER_BIN` can point to a custom
worker executable and `KITT_STT_PYTHON` can select the Python interpreter.

A typical local configuration is:

```json
{
  "speech_to_text": {
    "base_url": "http://127.0.0.1:8000/v1",
    "model": "whisper-1",
    "api_key_env": null,
    "local_provider": true,
    "allow_remote": false
  }
}
```

`pt-BR`, `en-US`, etc. are normalized to their primary ISO language (`pt`, `en`) before they are sent as the `language` field.

## TTS

No TTS model is kept resident by default. The response is spoken with the operating-system adapter:

- Windows: `System.Speech` through a short-lived, fixed PowerShell script.
- macOS: `say`.
- Linux: `spd-say`, with `espeak-ng` / `espeak` fallback.

On Debian/Ubuntu:

```bash
sudo apt install speech-dispatcher espeak-ng
```

The microphone input is discarded while KITT is speaking and for a short echo guard afterwards, preventing self-trigger loops.

## Privacy and storage

- Raw microphone audio is never persisted as memory.
- `voice-cache/` is restricted to `0700` and temporary utterances are created atomically with mode `0600` on Unix, then deleted immediately after transcription.
- A bounded audio queue prevents unbounded RAM growth.
- `transcript_prefix` requires local STT.
- Remote STT can only be used safely after a local wake-word detector has activated the assistant.

## Idle footprint

The resident path is:

```text
kittd + CPAL microphone stream + local wake-word detector
```

The HUD, LLM request, STT request, and TTS process only become active when needed.
