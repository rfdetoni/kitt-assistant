# KITT Assistant

> 24/7 personal assistant daemon with authenticated loopback IPC and ephemeral HUD overlay.

Follows **Clean Architecture** with strict dependency boundaries:
`domain <- application <- infrastructure <- apps`

---

## 🏛️ Components

- **`kittd`**: Low-footprint Rust daemon (~5 MB RAM, 0% CPU idle). Manages memory storage, local/remote model requests, auth tokens, and ephemeral HUD lifecycle.
- **`kittctl`**: Authenticated command-line client for OS hotkeys, scripts, and terminal interaction.
- **`kitt-hud`**: Transparent, borderless, always-on-top desktop overlay (Tauri + TypeScript + Vite) spawned on demand and automatically terminated after response TTL.

---

## 🚀 Building & Running

### 1. Build Workspace

```bash
cargo build --release --workspace
```

### 2. Build Ephemeral HUD

```bash
cd apps/kitt-hud
npm install
npm run build
```

### 3. Start Daemon

```bash
# Starts kittd listening on 127.0.0.1:41827
./target/release/kittd
```

### 4. Client CLI Interaction

```bash
# Ping daemon
./target/release/kittctl ping

# Ask question
./target/release/kittctl ask "Olá, KITT"

# Store explicit memory
./target/release/kittctl remember "Reunião toda segunda às 10h"

# Display image on HUD
./target/release/kittctl image /path/to/image.png
```

---

## 🔒 Security & Loopback Isolation

- Daemon binds strictly to loopback (`127.0.0.1`). External addresses are rejected.
- All IPC calls require the secret auth token stored at `~/.config/kitt/assistant/auth.token` (permissions `0600`).
- Model requests use an OpenAI-compatible adapter supporting local Ollama (`http://127.0.0.1:11434/v1`) or remote providers.
- Secret and private memories are stripped before calling remote providers.
- 1 MiB bounded stream reader protects against memory overflow attacks.

---

## ⚙️ Autostart & Packaging

Platform service templates are provided in [`packaging/`](packaging/):
- **Linux**: `packaging/linux/kitt-assistant.service` (systemd user unit)
- **macOS**: `packaging/macos/com.kitt.assistant.plist` (LaunchAgent)
- **Windows**: `packaging/windows/install-autostart.ps1` (Task Scheduler)

---

## 🧪 Testing

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

---

## 📄 License

MIT License. See [LICENSE](LICENSE).
