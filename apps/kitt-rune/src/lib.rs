#[cfg(not(target_arch = "wasm32"))]
pub mod hud;

//! K.I.T.T. unified Rune presentation shell.
//!
//! Deliberately read-only in its first migration stage: no ability to bypass
//! Agent grants, modify service settings, or access browser credentials.
//! Both native and wasm use the same view and state projections.
use rune::tok::{self, type_};
use rune::{App, AppSpec, Color, Cx, El, ScrollSpec, col, el, row, text};
use serde_json::Value;

const MAX_SNAPSHOT_BYTES: usize = 256 * 1024;
const BG: Color = Color { r: 0.055, g: 0.065, b: 0.082, a: 1.0 };
const SIDE: Color = Color { r: 0.072, g: 0.080, b: 0.101, a: 1.0 };
const CARD: Color = Color { r: 0.102, g: 0.113, b: 0.135, a: 1.0 };
const ACCENT: Color = Color { r: 0.92, g: 0.20, b: 0.25, a: 1.0 };
const MUTED: Color = Color { r: 0.57, g: 0.61, b: 0.68, a: 1.0 };

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Screen {
    #[default]
    Overview,
    Assistant,
    Models,
    Gateway,
    Cortex,
    Agents,
    Memory,
    Sessions,
    Settings,
}

impl Screen {
    const ALL: [Self; 9] = [
        Self::Overview, Self::Assistant, Self::Models, Self::Gateway,
        Self::Cortex, Self::Agents, Self::Memory, Self::Sessions,
        Self::Settings,
    ];
    fn title(self) -> &'static str {
        match self {
            Self::Overview => "Visão geral",
            Self::Assistant => "Assistant",
            Self::Models => "Modelos e rotas",
            Self::Gateway => "Reverse Proxy",
            Self::Cortex => "Córtex",
            Self::Agents => "Agentes",
            Self::Memory => "Memória",
            Self::Sessions => "Sessões",
            Self::Settings => "Configurações",
        }
    }
    fn slug(self) -> &'static str {
        match self {
            Self::Overview => "overview", Self::Assistant => "assistant",
            Self::Models => "models", Self::Gateway => "gateway",
            Self::Cortex => "cortex", Self::Agents => "agents",
            Self::Memory => "memory", Self::Sessions => "sessions",
            Self::Settings => "settings",
        }
    }
}

#[derive(Default)]
struct Summary {
    control_online: bool,
    assistant_online: bool,
    version: String,
    uptime_seconds: u64,
    voice_enabled: bool,
    stt_online: bool,
    fast_model: String,
    heavy_model: String,
    memory_exists: bool,
    memory_bytes: u64,
    gateway_online: bool,
    gateway_phase: String,
    gateway_managed: bool,
}

#[derive(Default)]
struct State {
    screen: Screen,
    data: Summary,
    snapshots: u64,
}

fn bounded_str(value: &Value, path: &str) -> String {
    value.pointer(path).and_then(Value::as_str)
        .unwrap_or("Não configurado")
        .chars().take(120).collect()
}

impl State {
    fn ingest(&mut self, source: &str, raw: &str) {
        if !matches!(source, "health" | "service" | "proxy")
            || raw.len() > MAX_SNAPSHOT_BYTES
        {
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(raw) else { return };
        if !value.is_object() { return; }
        match source {
            "health" => {
                self.data.control_online = value.get("status").and_then(Value::as_str) == Some("ok");
            }
            "service" => {
                self.data.assistant_online = value.pointer("/daemon/active")
                    .and_then(Value::as_bool).unwrap_or(false);
                self.data.version = bounded_str(&value, "/daemon/version");
                self.data.uptime_seconds = value.pointer("/daemon/uptime_seconds")
                    .and_then(Value::as_u64).unwrap_or(0);
                self.data.voice_enabled = value.pointer("/voice/enabled")
                    .and_then(Value::as_bool).unwrap_or(false);
                self.data.stt_online = value.pointer("/voice/stt_worker_online")
                    .and_then(Value::as_bool).unwrap_or(false);
                self.data.fast_model = bounded_str(&value, "/models/fast_model");
                self.data.heavy_model = bounded_str(&value, "/models/heavy_model");
                self.data.memory_exists = value.pointer("/memory/exists")
                    .and_then(Value::as_bool).unwrap_or(false);
                self.data.memory_bytes = value.pointer("/memory/size_bytes")
                    .and_then(Value::as_u64).unwrap_or(0);
            }
            "proxy" => {
                self.data.gateway_online = value.get("api_online")
                    .and_then(Value::as_bool).unwrap_or(false);
                self.data.gateway_phase = bounded_str(&value, "/phase");
                self.data.gateway_managed = value.get("managed")
                    .and_then(Value::as_bool).unwrap_or(false);
            }
            _ => unreachable!(),
        }
        self.snapshots = self.snapshots.saturating_add(1);
    }
}

fn paragraph(value: impl Into<String>) -> El {
    text(value.into()).font(type_::BODY).color(MUTED).w_full()
}

fn section_heading(title: &str, subtitle: &str) -> El {
    col().gap(8.0)
        .child(text(title.to_string()).font(type_::TITLE).color(tok::theme(true).text))
        .child(paragraph(subtitle))
}

fn tile(title: &str, value: impl Into<String>, note: &str) -> El {
    col().grow(1.0).min_w(0.0).padding(20.0).gap(12.0)
        .fill(CARD).radius(14.0)
        .child(text(title.to_string()).font(type_::LABEL).color(MUTED))
        .child(text(value.into()).font(type_::TITLE).color(tok::theme(true).text))
        .child(paragraph(note))
}

fn note_card(title: &str, description: &str) -> El {
    col().w_full().padding(20.0).gap(9.0).radius(14.0).fill(CARD)
        .child(text(title.to_string()).font(type_::LABEL).color(tok::theme(true).text))
        .child(paragraph(description))
}

fn sidebar_item(page: Screen, selected: Screen) -> El {
    let on = page == selected;
    el().key(format!("nav-{}", page.slug())).w_full().h(44.0).px(14.0)
        .radius(10.0)
        .fill(if on { CARD } else { SIDE })
        .border(if on { 1.0 } else { 0.0 }, if on { ACCENT } else { SIDE })
        .center_y()
        .role("button").label(page.title()).focusable(true)
        .child(text(page.title()).font(type_::LABEL)
            .color(if on { tok::theme(true).text } else { MUTED }))
        .on_press(move |state: &mut State| state.screen = page)
}

fn workspace(s: &State) -> El {
    let online = if s.data.control_online { "CONECTADO" } else { "SEM CONEXÃO" };
    col().w(230.0).h_full().padding(18.0).gap(12.0).fill(SIDE)
        .child(text("K.I.T.T.").font(type_::TITLE).color(tok::theme(true).text))
        .child(text("RUNE / CONTROL PLANE").font(type_::MONO_CAPTION).color(ACCENT))
        .child(el().w_full().h(1.0).fill(CARD))
        .children(Screen::ALL.map(|page| sidebar_item(page, s.screen)))
        .child(el().grow(1.0))
        .child(text(online).font(type_::MONO_CAPTION)
            .color(if s.data.control_online { tok::theme(true).text } else { MUTED }))
        .child(paragraph("Somente leitura · backend existente"))
}

fn screen_content(s: &State) -> El {
    let d = &s.data;
    let mut body = col().w_full().gap(16.0);
    body = body.child(match s.screen {
        Screen::Overview => section_heading("KITT Operations Center",
            "Saúde dos componentes, disponibilidade e serviços residentes"),
        Screen::Assistant => section_heading("Assistant / Daemon",
            "Ciclo de vida do residente e recursos locais"),
        Screen::Models => section_heading("Modelos e roteamento",
            "Perfis que já estão configurados no KITT Assistant"),
        Screen::Gateway => section_heading("Reverse Proxy",
            "Status do gateway. Tokens e credenciais não são mostrados."),
        Screen::Cortex => section_heading("Córtex",
            "Eventos duráveis e políticas existentes — painel em integração"),
        Screen::Agents => section_heading("Agentes especializados",
            "A execução continua sob a autoridade do KITT Agent"),
        Screen::Memory => section_heading("Memória semântica",
            "Persistência e isolamento permanecem sob o serviço de memória"),
        Screen::Sessions => section_heading("Sessões de conversação",
            "Sessões e aprovações continuam disponíveis na interface remota segura"),
        Screen::Settings => section_heading("Configurações do ecossistema",
            "Alterações versionadas continuam no Control Center até paridade"),
    });

    match s.screen {
        Screen::Overview => {
            body = body.child(row().w_full().gap(14.0)
                .child(tile("Control Center", if d.control_online { "ONLINE" } else { "OFFLINE" }, "API local do daemon"))
                .child(tile("Assistant", if d.assistant_online { "ATIVO" } else { "INDISPONÍVEL" }, "KITT residente"))
                .child(tile("Gateway", if d.gateway_online { "ONLINE" } else { "OFFLINE" }, "Reverse Proxy")))
                .child(note_card("Runtime único, interfaces compartilhadas",
                    "O Rune apresenta as informações; a segurança e a execução permanecem nos serviços originais."));
        }
        Screen::Assistant => {
            body = body.child(row().w_full().gap(14.0)
                .child(tile("Versão", d.version.clone(), "kittd"))
                .child(tile("Uptime", format!("{} s", d.uptime_seconds), "Tempo desde início"))
                .child(tile("Voz", if d.voice_enabled { "ATIVA" } else { "DESATIVADA" },
                    if d.stt_online { "STT disponível" } else { "STT indisponível" })));
        }
        Screen::Models => {
            body = body.child(note_card("Modelo fast", &d.fast_model))
                .child(note_card("Modelo heavy", &d.heavy_model))
                .child(note_card("Alteração de modelos", "Os perfis e a política de roteamento devem ser alterados pela API versionada existente. A UI Rune não armazena chaves."));
        }
        Screen::Gateway => {
            body = body.child(row().w_full().gap(14.0)
                .child(tile("API", if d.gateway_online { "ONLINE" } else { "OFFLINE" }, "Disponibilidade do Reverse Proxy"))
                .child(tile("Estado", d.gateway_phase.clone(), "Fase reportada"))
                .child(tile("Gerenciado", if d.gateway_managed { "SIM" } else { "NÃO" }, "Processo supervisionado")))
                .child(note_card("Transporte", "Sessões temporárias e providers seguem os contratos e permissões já implementados."));
        }
        Screen::Memory => {
            body = body.child(row().w_full().gap(14.0)
                .child(tile("Banco local", if d.memory_exists { "DISPONÍVEL" } else { "NÃO ENCONTRADO" }, "Sem leitura de conteúdo privado"))
                .child(tile("Tamanho", format!("{} KiB", d.memory_bytes / 1024), "Armazenamento reportado pelo Assistant")));
        }
        Screen::Cortex => {
            body = body.child(note_card("Event Plane",
                    "Eventos duráveis, preflight e System One disponíveis via IPC autenticado; dashboard específico ainda não conectado."))
                .child(note_card("Execução governada",
                    "Nenhum evento dispara automaticamente uma ação privilegiada ou altera permissões de ferramentas."));
        }
        Screen::Agents => {
            body = body.child(note_card("Agent CLI",
                    "Planejamento, subagentes, aprovação e orçamento continuam no runtime Python."))
                .child(note_card("Superfície remota",
                    "Gerenciamento de sessões, ações e aprovações permanece na interface remota autenticada durante a migração."));
        }
        Screen::Sessions => {
            body = body.child(note_card("Compatibilidade",
                    "A migração de editor, streaming e aprovações requer testes de paridade com o backend remoto; nenhum dado de sessão é exposto sem autenticação."));
        }
        Screen::Settings => {
            body = body.child(note_card("Alterações seguras",
                    "Configurações sensíveis não são carregadas nessa visualização read-only. Utilize o Control Center clássico até a implantação das mutações com CSRF e revisão de diff."));
        }
    }
    body
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let content = col().grow(1.0).h_full().min_w(0.0).min_h(0.0)
        .padding(28.0).gap(18.0)
        .child(row().w_full().center_y().gap(8.0)
            .child(text("K.I.T.T. / OPERATIONS").font(type_::MONO_CAPTION).color(ACCENT))
            .child(el().grow(1.0))
            .child(text(format!("Estado recebido: {}", s.snapshots)).font(type_::MONO_CAPTION).color(MUTED)))
        .child(col().grow(1.0).min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .child(screen_content(s)))
        .child(text("KITT · RUNE SHELL · nenhuma autorização é concedida pela interface")
            .font(type_::MONO_CAPTION).color(MUTED));
    row().w_full().h_full().fill(BG)
        .child(workspace(s))
        .child(content)
}

/// Receive a bounded allowlisted JSON snapshot from the browser's same-origin
/// authenticated host. This function intentionally has NO mutation tool API.
#[cfg(target_arch = "wasm32")]
thread_local! {
    static HANDLE: std::cell::RefCell<Option<rune::AppHandle<State>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn update_snapshot(source: &str, raw: &str) {
    if !matches!(source, "health" | "service" | "proxy") ||
        raw.len() > MAX_SNAPSHOT_BYTES { return; }
    let source = source.to_owned();
    let raw = raw.to_owned();
    HANDLE.with(|item| {
        if let Some(handle) = item.borrow().as_ref() {
            handle.update(move |state, tx| {
                state.ingest(&source, &raw);
                tx.instant();
            });
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn native_refresh(handle: rune::AppHandle<State>, alive: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    use std::io::Read;
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    let Ok(client) = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(4))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build() else { return };
    let sources = [
        ("health", "/api/v1/health"),
        ("service", "/api/v1/service/status"),
        ("proxy", "/api/v1/reverse-proxy/status"),
    ];
    while alive.load(Ordering::Relaxed) {
        for (source, path) in sources {
            let address = format!("http://127.0.0.1:41828{path}");
            let raw = client.get(address).send().ok()
                .and_then(|resp| {
                    if !resp.status().is_success() { return None; }
                    let mut reader = resp.take((MAX_SNAPSHOT_BYTES + 1) as u64);
                    let mut bytes = Vec::new();
                    reader.read_to_end(&mut bytes).ok()?;
                    if bytes.len() > MAX_SNAPSHOT_BYTES { return None; }
                    String::from_utf8(bytes).ok()
                })
                .unwrap_or_else(|| r#"{"available":false}"#.to_string());
            let source = source.to_string();
            handle.update(move |state, tx| {
                state.ingest(&source, &raw);
                tx.instant();
            });
        }
        for _ in 0..150 {
            if !alive.load(Ordering::Relaxed) { break; }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(start))]
pub fn start() {
    let app = App::new(AppSpec::new("K.I.T.T. / Rune", (1180.0, 740.0), State::default, view));
    #[cfg(target_arch = "wasm32")]
    {
        HANDLE.with(|state| *state.borrow_mut() = Some(app.handle()));
        app.run();
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let alive = running.clone();
        let handle = app.handle();
        std::thread::spawn(move || native_refresh(handle, alive));
        app.run();
        running.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    #[test]
    fn projects_only_allowlisted_nonsecret_status() {
        let mut state = State::default();
        state.ingest("service", r#"{"daemon":{"active":true,"version":"1.2.3"},"models":{"fast_model":"fast","heavy_model":"heavy"},"api_key":"SHOULD_NOT_SURFACE"}"#);
        assert!(state.data.assistant_online);
        assert_eq!(state.data.version, "1.2.3");
        assert_eq!(state.data.fast_model, "fast");
        state.ingest("unknown", r#"{"daemon":{"active":false}}"#);
        assert!(state.data.assistant_online);
        state.ingest("service", "not-json");
        assert!(state.data.assistant_online);
        assert_eq!(state.snapshots, 1);
    }

    #[test]
    fn rune_navigation_is_real_and_accessible() {
        let mut rt = Runtime::new(State::default(), view, (1180.0, 740.0), Env::new(true, 1.0));
        rt.frame(0.0);
        let (x, y) = rt.find("nav-models").expect("models navigation is mounted");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.1);
        assert_eq!(rt.state.screen, Screen::Models);
    }

    #[test]
    fn bad_status_is_not_considered_online() {
        let mut state = State::default();
        state.ingest("health", r#"{"available":false}"#);
        state.ingest("proxy", r#"{"status":"ok","api_online":false}"#);
        assert!(!state.data.control_online);
        assert!(!state.data.gateway_online);
    }
}
