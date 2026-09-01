# Manual do K.I.T.T. Assistant (`kitt-assistant`)

> Daemon de assistência contínua 24/7 (`kittd`), utilitário de controle CLI (`kittctl`), motor de voz viva-voz (Hands-free Voice), roteamento de modelos (Fast vs Heavy) e interface HUD gráfica.

---

## 1. Visão Geral e Arquitetura

O **`kitt-assistant`** é o núcleo de presença contínua do ecossistema.
Ele opera como um daemon de segundo plano que processa comandos de voz, integra o Control Center Web e coordena a interação com o usuário através de um HUD gráfico ou TUI.

### Arquitetura de Módulos:
```text
                  +-----------------------------------+
                  |      apps/kitt-hud (Vue/Vite)     |
                  +-----------------+-----------------+
                                    |
                                    v
+-------------------------------------------------------------------------+
|                         apps/kittd (Daemon Rust)                        |
|                                                                         |
|  [Voice Pipeline]       [Control Center Web]      [Intelligent Router]  |
|  - Microfone (CPAL)     - HTTP 127.0.0.1:41828    - Fast (ex: 1.5b)     |
|  - Wake Word & VAD      - Overlay Manager         - Heavy (ex: 14b)     |
|  - STT Whisper Client   - Config API              - Tool Dispatch       |
+-----------------------------------+-------------------------------------+
                                    ^
                                    | IPC / Socket (127.0.0.1:41827)
                  +-----------------+-----------------+
                  |       apps/kittctl (CLI Rust)     |
                  +-----------------------------------+
```

---

## 2. Requisitos de Sistema

- **Rust**: 1.80+ (com Cargo)
- **Node.js**: 20+ e `npm` (para compilação da interface HUD)
- **Linux**: `libasound2-dev` e `pkg-config` (para captura de áudio ALSA)
- **macOS**: Frameworks nativos CoreAudio e AVFoundation
- **Windows**: Windows Media Foundation / WASAPI (incluso no SDK do Windows)

---

## 3. Instalação e Compilação por Sistema Operacional

### 🐧 A. LINUX (Ubuntu/Debian)

```bash
# 1. Instalar dependências de áudio e compilação
sudo apt-get update && sudo apt-get install -y libasound2-dev pkg-config

# 2. Compilar binários Rust do daemon e CLI
cargo build --release --workspace

# 3. Compilar interface web do HUD
cd apps/kitt-hud
npm ci
npm run build
cd ../..
```

### 🍏 B. macOS

```bash
# 1. Compilar binários Rust
cargo build --release --workspace

# 2. Compilar interface do HUD
cd apps/kitt-hud
npm ci
npm run build
cd ../..
```

### 🪟 C. WINDOWS (PowerShell)

```powershell
# 1. Compilar binários Rust (kittd.exe e kittctl.exe)
cargo build --release --workspace

# 2. Compilar interface do HUD
cd apps/kitt-hud
npm ci
npm run build
cd ..\..
```

---

## 4. Configuração do Assistente e Arquivos de Provedores

### Arquivo de Configuração de Modelos (`models.json`):
Localizado em:
- **Linux**: `~/.config/kitt/models.json`
- **macOS**: `~/Library/Application Support/kitt/models.json`
- **Windows**: `%APPDATA%\kitt\models.json`

```json
{
  "fast": {
    "base_url": "http://127.0.0.1:11434/v1",
    "model": "qwen2.5-coder:1.5b",
    "temperature": 0.2
  },
  "heavy": {
    "base_url": "http://127.0.0.1:11434/v1",
    "model": "qwen2.5-coder:14b",
    "temperature": 0.4
  }
}
```

*Regra de Segurança R5: As `base_url` dos provedores não devem conter credenciais embutidas (`user:pass@`), nem `?query` ou `#fragment`.*

---

## 5. Guia de Operação e Uso

### 1. Iniciar o Daemon do Assistente (`kittd`):
```bash
cargo run --release --bin kittd
```
*Portas ativas:*
- **IPC do Daemon**: `127.0.0.1:41827`
- **Control Center Web**: `http://127.0.0.1:41828`

### 2. Controlar o Assistente via Linha de Comando (`kittctl`):

```bash
# Verificar status do daemon
cargo run --release --bin kittctl -- status

# Enviar comando de texto para o assistente
cargo run --release --bin kittctl -- ask "Qual é o status da bateria e memória?"

# Ativar/Desativar modo de escuta por voz
cargo run --release --bin kittctl -- voice on
cargo run --release --bin kittctl -- voice off

# Encerrar o daemon graciosamente
cargo run --release --bin kittctl -- stop
```

---

## 6. Validação e Testes
```bash
# Testes Rust
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Testes de integração Node
node --test tests/control_center_r3.test.mjs
```
