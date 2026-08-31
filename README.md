# KITT Assistant

> 24/7 personal assistant daemon with authenticated loopback IPC, embedded KITT Control Center Web SPA, and ephemeral HUD overlay.

Follows **Clean Architecture** with strict dependency boundaries:
`domain <- application <- infrastructure <- apps`

---

## 🏛️ Components

- **`kittd`**: Low-footprint Rust daemon (~5-8 MB RAM, 0% CPU idle).
  - Serves authenticated IPC on `127.0.0.1:41827`.
  - Serves **KITT Control Center Web SPA** on `127.0.0.1:41828`.
  - Manages Fast/Heavy model routing, Hands-free Voice pipeline, memory integration, and ephemeral HUD lifecycle.
- **`kittctl`**: Authenticated command-line client for OS hotkeys, terminal queries, and automation scripts.
- **`kitt-hud`**: Transparent, borderless, always-on-top desktop overlay (Tauri + TypeScript + Vite) spawned on demand and automatically closed after response TTL.

---

## 🎛️ KITT Control Center Web GUI

`kittd` embeds and serves the **KITT Control Center** directly on loopback:

- **URL**: `http://127.0.0.1:41828/`
- **Features**: Single-page dark theme dashboard, global settings search, dynamic catalog generation, health checks, live diff viewer before persisting changes, and revisioned atomic configuration overlay.
- **Security**: Loopback only (`127.0.0.1`, `localhost`, `[::1]`), CSRF tokens on mutation methods, strict CSP headers, `X-Frame-Options: DENY`, `no-store` caching.

---

## 🎙️ Voice & Audio Pipeline

- **Activation Modes**:
  - `auto`: Uses local wakeword model if present; falls back gracefully to local transcript prefix matching.
  - `wakeword`: Uses `.rpw` Rustpotter model.
  - `transcript_prefix`: Prefix detection ("kitt", "hey kitt", "ei kitt").
- **Resilience**: Automatic microphone stream recovery with exponential backoff on device disconnection.
- **Privacy & Cleanup**: Audio utterance cache cleaned automatically; system TTS temporary files created with strict `0600` permissions.

---

## 🚀 Building & Running

### 1. Build Workspace

```bash
cargo build --release --workspace
```

### 2. Build Ephemeral HUD (Optional)

```bash
cd apps/kitt-hud
npm install
npm run build
cd ../..
```

### 3. Native Background Service Management

`kittctl` provides native commands to manage `kittd` as a background OS service (systemd on Linux, LaunchAgent on macOS, Scheduled Tasks on Windows):

```bash
# Install and register the background service
./target/release/kittctl service install

# Start the background service
./target/release/kittctl service start

# Check service status & daemon health
./target/release/kittctl service status

# Restart the service
./target/release/kittctl service restart

# Stop the service
./target/release/kittctl service stop

# Uninstall the service
./target/release/kittctl service uninstall
```

### 4. Interacting with `kittctl`

```bash
# Ping daemon health
./target/release/kittctl ping

# Query assistant (routes to Fast or Heavy model automatically)
./target/release/kittctl ask "Olá KITT"

# Explicit routing hint
./target/release/kittctl ask --route heavy "Escreva um algoritmo de ordenação em Rust"

# Store memory
./target/release/kittctl remember "Prefiro respostas concisas em português"

# Show image on HUD overlay
./target/release/kittctl image /path/to/screenshot.png
```

---

## ⚙️ Configuration & Overlay

Configuration files are loaded from `${XDG_CONFIG_HOME:-~/.config}/kitt/assistant/`:
- `config.json`: Core daemon settings (`listen`, `base_url`, `model`, `api_key_env`, `allow_personal_remote`, `hud_ttl_ms`).
- `models.json`: Fast/Heavy/STT routing profiles (`fast`, `heavy`, `speech_to_text`).
- `voice.json`: Voice parameters (`enabled`, `locale`, `activation_mode`, `min_rms`, `silence_ms`, `tts_enabled`).

Overrides applied in the Control Center GUI are layered atomically from `${XDG_CONFIG_HOME:-~/.config}/kitt/control-center/overrides.json`.

---

## 🔒 Security & Loopback Isolation

- Daemon binds strictly to loopback (`127.0.0.1`, `[::1]`). External addresses are rejected.
- All IPC calls require the secret auth token stored at `~/.config/kitt/assistant/auth.token` (permissions `0600`).
- Secret and private memories are stripped before calling remote providers.
- 1 MiB bounded stream reader protects against memory exhaustion attacks.

---

## 🧪 Testing & Linting

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

---

## 📄 License

MIT License. See [LICENSE](LICENSE).
