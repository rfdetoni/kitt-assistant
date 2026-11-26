//! K.I.T.T. unified Rune presentation shell.
//!
//! Deliberately read-only in its first migration stage: no ability to bypass
//! Agent grants, modify service settings, or access browser credentials.
//! Both native and wasm use the same view and state projections.
#[cfg(not(target_arch = "wasm32"))]
pub mod hud;

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
    Workers,
    Snapshots,
    Mounts,
}

impl Screen {
    const ALL: [Self; 12] = [
        Self::Overview, Self::Assistant, Self::Models, Self::Gateway,
        Self::Cortex, Self::Agents, Self::Memory, Self::Sessions,
        Self::Settings, Self::Workers, Self::Snapshots, Self::Mounts,
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
            Self::Workers => "Workers",
            Self::Snapshots => "Snapshots",
            Self::Mounts => "Mounts",
        }
    }
    fn slug(self) -> &'static str {
        match self {
            Self::Overview => "overview", Self::Assistant => "assistant",
            Self::Models => "models", Self::Gateway => "gateway",
            Self::Cortex => "cortex", Self::Agents => "agents",
            Self::Memory => "memory", Self::Sessions => "sessions",
            Self::Settings => "settings",
            Self::Workers => "workers",
            Self::Snapshots => "snapshots",
            Self::Mounts => "mounts",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum WorkerFilter {
    #[default]
    All,
    Running,
    Paused,
    Failed,
}

impl WorkerFilter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "Todos",
            Self::Running => "Ativos",
            Self::Paused => "Pausados",
            Self::Failed => "Falhos/Indisp.",
        }
    }
    fn matches(self, state: &str) -> bool {
        match self {
            Self::All => true,
            Self::Running => state == "RUNNING",
            Self::Paused => state == "PAUSED" || state == "UNBOUND",
            Self::Failed => state == "FAILED" || state == "UNAVAILABLE",
        }
    }
}

#[derive(Clone, Default)]
struct WorkerItem {
    id: String,
    label: String,
    os: String,
    backend: String,
    available: bool,
    capacity_available: bool,
    state: String,
    cpu_millis: u64,
    memory_bytes: u64,
    gpu_count: u64,
    gpu_detected_count: u64,
    gpu_vendor: String,
    active_leases: u64,
    executions: u64,
    execution_failures: u64,
    cache_bytes: u64,
}

#[derive(Clone, Default)]
struct OperationItem {
    worker_id: String,
    action: String,
    ok: bool,
    summary: String,
}

#[derive(Clone, Default)]
struct SnapshotItem {
    snapshot_id: String,
    workspace_id: String,
    version: u64,
    total_bytes: u64,
}

#[derive(Clone, Default)]
struct MountItem {
    alias: String,
    kind: String,
    source: String,
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
    worker_available: bool,
    worker_state: String,
    worker_executions: u64,
    worker_failures: u64,
    worker_cache_bytes: u64,
    worker_uploaded_bytes: u64,
    worker_downloaded_bytes: u64,
    workers: Vec<WorkerItem>,
    operations: Vec<OperationItem>,
    snapshot_items: Vec<SnapshotItem>,
    mounts: Vec<MountItem>,
}

#[derive(Default)]
struct State {
    screen: Screen,
    data: Summary,
    snapshots: u64,
    worker_filter: WorkerFilter,
    sort_by_execs: bool,
    selected_worker: String,
    last_action_status: String,
    #[cfg(not(target_arch = "wasm32"))]
    action_tx: Option<std::sync::mpsc::Sender<(String, String)>>,
}

fn bounded_str(value: &Value, path: &str) -> String {
    value.pointer(path).and_then(Value::as_str)
        .unwrap_or("Não configurado")
        .chars().take(120).collect()
}

impl State {
    fn ingest(&mut self, source: &str, raw: &str) {
        if !matches!(
            source,
            "health" | "service" | "proxy" | "worker" | "workers" | "operations" | "snapshots" | "mounts"
        ) || raw.len() > MAX_SNAPSHOT_BYTES
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
            "worker" => {
                self.data.worker_available = value.get("available").and_then(Value::as_bool) == Some(true);
                self.data.worker_state = bounded_str(&value, "/state");
                self.data.worker_executions = value.get("executions").and_then(Value::as_u64).unwrap_or(0);
                self.data.worker_failures = value.get("execution_failures").and_then(Value::as_u64).unwrap_or(0);
                self.data.worker_cache_bytes = value.get("cache_bytes").and_then(Value::as_u64).unwrap_or(0);
                self.data.worker_uploaded_bytes = value.get("uploaded_bytes").and_then(Value::as_u64).unwrap_or(0);
                self.data.worker_downloaded_bytes = value.get("downloaded_bytes").and_then(Value::as_u64).unwrap_or(0);
            }
            "workers" => {
                if let Some(items) = value.get("workers").and_then(Value::as_array) {
                    self.data.workers = items.iter().take(16).filter_map(|w| {
                        let id: String = w.get("id")?.as_str()?.chars().take(64).collect();
                        if id.is_empty() { return None; }
                        Some(WorkerItem {
                            id: id.clone(),
                            label: w.get("label").and_then(Value::as_str).unwrap_or(&id).chars().take(64).collect(),
                            os: w.get("os").and_then(Value::as_str).unwrap_or("unknown").chars().take(24).collect(),
                            backend: w.get("backend").and_then(Value::as_str).unwrap_or("remote").chars().take(24).collect(),
                            available: w.get("available").and_then(Value::as_bool).unwrap_or(false),
                            capacity_available: w.get("capacity_available").and_then(Value::as_bool).unwrap_or(false),
                            state: w.get("state").and_then(Value::as_str).unwrap_or("UNAVAILABLE").chars().take(24).collect(),
                            cpu_millis: w.get("cpu_millis").and_then(Value::as_u64).unwrap_or(0),
                            memory_bytes: w.get("memory_bytes").and_then(Value::as_u64).unwrap_or(0),
                            gpu_count: w.get("gpu_count").and_then(Value::as_u64).unwrap_or(0),
                            gpu_detected_count: w.get("gpu_detected_count").and_then(Value::as_u64).unwrap_or(0),
                            gpu_vendor: w.get("gpu_vendor").and_then(Value::as_str).unwrap_or("none").chars().take(24).collect(),
                            active_leases: w.get("active_leases").and_then(Value::as_u64).unwrap_or(0),
                            executions: w.get("executions").and_then(Value::as_u64).unwrap_or(0),
                            execution_failures: w.get("execution_failures").and_then(Value::as_u64).unwrap_or(0),
                            cache_bytes: w.get("cache_bytes").and_then(Value::as_u64).unwrap_or(0),
                        })
                    }).collect();
                    if self.selected_worker.is_empty()
                        && let Some(first) = self.data.workers.first()
                    {
                        self.selected_worker = first.id.clone();
                    }
                }
                if let Some(ops) = value.get("operations").and_then(Value::as_array) {
                    self.ingest_ops(ops);
                }
            }
            "operations" => {
                if let Some(ops) = value.get("operations").and_then(Value::as_array) {
                    self.ingest_ops(ops);
                }
            }
            "snapshots" => {
                if let Some(items) = value.get("snapshots").and_then(Value::as_array) {
                    self.data.snapshot_items = items.iter().take(64).filter_map(|x| {
                        let sid: String = x.get("snapshot_id")?.as_str()?.chars().take(80).collect();
                        Some(SnapshotItem {
                            snapshot_id: sid,
                            workspace_id: x.get("workspace_id").and_then(Value::as_str).unwrap_or("").chars().take(64).collect(),
                            version: x.get("version").and_then(Value::as_u64).unwrap_or(1),
                            total_bytes: x.get("total_bytes").and_then(Value::as_u64).unwrap_or(0),
                        })
                    }).collect();
                }
            }
            "mounts" => {
                if let Some(items) = value.get("mounts").and_then(Value::as_array) {
                    self.data.mounts = items.iter().take(32).filter_map(|m| {
                        let alias: String = m.get("alias")?.as_str()?.chars().take(64).collect();
                        Some(MountItem {
                            alias,
                            kind: m.get("kind").and_then(Value::as_str).unwrap_or("").chars().take(16).collect(),
                            source: m.get("source").and_then(Value::as_str).unwrap_or("").chars().take(120).collect(),
                        })
                    }).collect();
                }
            }
            _ => unreachable!(),
        }
        self.snapshots = self.snapshots.saturating_add(1);
    }

    fn ingest_ops(&mut self, ops: &[Value]) {
        self.data.operations = ops.iter().take(32).filter_map(|o| {
            Some(OperationItem {
                worker_id: o.get("worker_id").and_then(Value::as_str).unwrap_or("").chars().take(64).collect(),
                action: o.get("action").and_then(Value::as_str).unwrap_or("").chars().take(32).collect(),
                ok: o.get("ok").and_then(Value::as_bool).unwrap_or(false),
                summary: o.get("summary").and_then(Value::as_str).unwrap_or("").chars().take(120).collect(),
            })
        }).collect();
    }

    fn trigger_action(&mut self, action: &str) {
        let worker_id = if self.selected_worker.is_empty() {
            self.data.workers.first().map(|w| w.id.clone()).unwrap_or_default()
        } else {
            self.selected_worker.clone()
        };
        if worker_id.is_empty() {
            self.last_action_status = "Nenhum worker selecionado".to_string();
            return;
        }
        self.last_action_status = format!("Ação '{action}' enviada para {worker_id}");
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(tx) = &self.action_tx {
            let _ = tx.send((worker_id, action.to_string()));
        }
        #[cfg(target_arch = "wasm32")]
        {
            kitt_rune_action(&worker_id, action);
        }
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
    col().w(230.0).h_full().padding(18.0).gap(10.0).fill(SIDE)
        .child(text("K.I.T.T.").font(type_::TITLE).color(tok::theme(true).text))
        .child(text("RUNE / CONTROL PLANE").font(type_::MONO_CAPTION).color(ACCENT))
        .child(el().w_full().h(1.0).fill(CARD))
        .children(Screen::ALL.map(|page| sidebar_item(page, s.screen)))
        .child(el().grow(1.0))
        .child(text(online).font(type_::MONO_CAPTION)
            .color(if s.data.control_online { tok::theme(true).text } else { MUTED }))
        .child(paragraph("APIs existentes · CSRF / Zero Trust"))
}

fn action_button(key: &str, label: &str, action: &'static str, active: bool) -> El {
    el().key(key.to_string()).h(32.0).px(12.0).radius(8.0)
        .fill(if active { ACCENT } else { CARD })
        .border(1.0, if active { ACCENT } else { MUTED })
        .center_y()
        .role("button").label(label).focusable(true)
        .child(text(label).font(type_::MONO_CAPTION).color(tok::theme(true).text))
        .on_press(move |state: &mut State| state.trigger_action(action))
}

fn filter_pill(filter: WorkerFilter, selected: WorkerFilter) -> El {
    let on = filter == selected;
    el().key(format!("filter-{}", filter.label())).h(28.0).px(10.0).radius(8.0)
        .fill(if on { ACCENT } else { CARD })
        .center_y()
        .role("button").label(filter.label()).focusable(true)
        .child(text(filter.label()).font(type_::MONO_CAPTION).color(tok::theme(true).text))
        .on_press(move |state: &mut State| state.worker_filter = filter)
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
        Screen::Workers => section_heading("Gerenciamento de Workers",
            "Inventário centralizado, ciclo de vida, leases de GPU, health checks e histórico"),
        Screen::Snapshots => section_heading("Snapshots & CAS",
            "Snapshots incrementais chunked e coleta de lixo por workspace"),
        Screen::Mounts => section_heading("Mounts Imutáveis (GitHub / S3)",
            "Fontes somente leitura verificadas por digest; credenciais ficam somente no host"),
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
                    "O Rune aciona somente as APIs autorizadas; credenciais e execução ficam nos serviços originais."));
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
                    "Gerenciamento de sessões, ações e aprovações permanece na interface remota autenticada."));
        }
        Screen::Sessions => {
            body = body.child(note_card("Compatibilidade",
                    "Nenhum dado de sessão ou aprovação automática é exposto sem autenticação explícita."));
        }
        Screen::Workers => {
            let mut filtered: Vec<&WorkerItem> = d.workers.iter()
                .filter(|w| s.worker_filter.matches(&w.state))
                .collect();
            if s.sort_by_execs {
                filtered.sort_by(|a, b| b.executions.cmp(&a.executions));
            }
            body = body.child(row().w_full().gap(8.0)
                .child(filter_pill(WorkerFilter::All, s.worker_filter))
                .child(filter_pill(WorkerFilter::Running, s.worker_filter))
                .child(filter_pill(WorkerFilter::Paused, s.worker_filter))
                .child(filter_pill(WorkerFilter::Failed, s.worker_filter))
                .child(el().grow(1.0))
                .child(el().key("sort-workers").h(28.0).px(10.0).radius(8.0).fill(CARD).center_y()
                    .role("button").label("Ordenar").focusable(true)
                    .child(text(if s.sort_by_execs { "Ordem: Execuções" } else { "Ordem: Padrão" }).font(type_::MONO_CAPTION).color(MUTED))
                    .on_press(|st: &mut State| st.sort_by_execs = !st.sort_by_execs)));

            if !s.last_action_status.is_empty() {
                body = body.child(note_card("Último comando", &s.last_action_status));
            }
            if !filtered.is_empty() {
                for w in filtered.iter().take(8) {
                    let is_sel = s.selected_worker == w.id;
                    let wid = w.id.clone();
                    body = body.child(
                        el().key(format!("worker-{}", w.id)).w_full().padding(14.0).radius(12.0).fill(CARD)
                            .border(if is_sel { 1.5 } else { 0.0 }, if is_sel { ACCENT } else { CARD })
                            .role("button").label(&w.label).focusable(true)
                            .on_press(move |st: &mut State| st.selected_worker = wid.clone())
                            .child(col().w_full().gap(6.0)
                                .child(row().w_full().gap(12.0)
                                    .child(text(format!("{} ({}) · {}", w.label, w.os, w.backend)).font(type_::LABEL).color(tok::theme(true).text))
                                    .child(el().grow(1.0))
                                    .child(text(w.state.clone()).font(type_::MONO_CAPTION).color(ACCENT)))
                                .child(paragraph(format!(
                                    "Disp: {}/cap {} · CPU {}m · RAM {} MiB · GPU {} ({}, {} det, leases {}) · Execs {} (falhas {}) · Cache {} KiB",
                                    if w.available { "sim" } else { "não" },
                                    if w.capacity_available { "sim" } else { "não" },
                                    w.cpu_millis, w.memory_bytes / (1024 * 1024),
                                    w.gpu_count, w.gpu_vendor, w.gpu_detected_count, w.active_leases,
                                    w.executions, w.execution_failures, w.cache_bytes / 1024
                                ))))
                    );
                }
                body = body.child(row().w_full().gap(8.0)
                    .child(action_button("act-provision", "Provisionar", "provision", false))
                    .child(action_button("act-resume", "Iniciar/Retomar", "resume", true))
                    .child(action_button("act-pause", "Pausar", "pause", false))
                    .child(action_button("act-hibernate", "Hibernar", "hibernate", false)))
                    .child(row().w_full().gap(8.0)
                        .child(action_button("act-terminate", "Encerrar", "terminate", false))
                        .child(action_button("act-reconnect", "Reconectar", "reconnect", false))
                        .child(action_button("act-health", "Health", "health", false))
                        .child(action_button("act-reconcile", "Reconciliar Leases", "leases_reconcile", false)));
            } else if d.worker_available {
                body = body.child(row().w_full().gap(14.0)
                    .child(tile("Estado", d.worker_state.clone(), "Runtime remoto"))
                    .child(tile("Execuções", d.worker_executions.to_string(), "Desde o início do worker"))
                    .child(tile("Falhas", d.worker_failures.to_string(), "Execuções com falha")))
                    .child(row().w_full().gap(14.0)
                        .child(tile("Cache", format!("{} KiB", d.worker_cache_bytes / 1024), "Armazenamento"))
                        .child(tile("Upload", format!("{} KiB", d.worker_uploaded_bytes / 1024), "Dados recebidos"))
                        .child(tile("Download", format!("{} KiB", d.worker_downloaded_bytes / 1024), "Dados enviados")));
            } else {
                body = body.child(note_card("Nenhum worker configurado", "Configure KITT_RUNE_WORKERS_JSON ou KITT_RUNE_WORKER_ENDPOINT no serviço local."));
            }
            if !d.operations.is_empty() {
                let mut ops_col = col().w_full().gap(6.0);
                for op in d.operations.iter().take(6) {
                    ops_col = ops_col.child(paragraph(format!(
                        "[{}] {} · {}: {}",
                        if op.ok { "OK" } else { "ERRO" }, op.worker_id, op.action, op.summary
                    )));
                }
                body = body.child(col().w_full().padding(16.0).gap(8.0).radius(12.0).fill(CARD)
                    .child(text("Histórico operacional recente").font(type_::LABEL).color(tok::theme(true).text))
                    .child(ops_col));
            }
        }
        Screen::Snapshots => {
            body = body.child(row().w_full().gap(8.0)
                .child(action_button("snap-export", "Exportar Snapshot", "snapshot_export", true))
                .child(action_button("snap-gc", "Executar CAS GC", "cas_gc", false)));
            if d.snapshot_items.is_empty() {
                body = body.child(note_card("Sem snapshots registrados", "Snapshots incrementais v2 aparecerão após exportação via runtime."));
            } else {
                for snap in d.snapshot_items.iter().take(12) {
                    body = body.child(note_card(
                        &snap.snapshot_id,
                        &format!("Workspace: {} · Formato v{} · {} KiB", snap.workspace_id, snap.version, snap.total_bytes / 1024),
                    ));
                }
            }
        }
        Screen::Mounts => {
            if d.mounts.is_empty() {
                body = body.child(note_card("Sem mounts configurados", "Mounts GitHub/S3 são materializados no host sem expor chaves ao guest."));
            } else {
                for m in d.mounts.iter().take(12) {
                    body = body.child(note_card(
                        &format!("{} ({})", m.alias, m.kind),
                        &format!("Origem verificada: {}", m.source),
                    ));
                }
            }
        }
        Screen::Settings => {
            body = body.child(note_card("Alterações seguras",
                    "Configurações sensíveis não são carregadas nessa visualização. Utilize o Control Center para edições versionadas com CSRF."));
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

// Receive a bounded allowlisted JSON snapshot from the browser's same-origin
// authenticated host. This function intentionally has NO mutation tool API.
#[cfg(target_arch = "wasm32")]
thread_local! {
    static HANDLE: std::cell::RefCell<Option<rune::AppHandle<State>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(inline_js = "export function kitt_rune_action(w, a) { if (typeof window !== 'undefined' && typeof window.__kittRuneAction === 'function') { void window.__kittRuneAction(w, a); } }")]
extern "C" {
    fn kitt_rune_action(worker_id: &str, action: &str);
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn update_snapshot(source: &str, raw: &str) {
    if !matches!(
        source,
        "health" | "service" | "proxy" | "worker" | "workers" | "operations" | "snapshots" | "mounts"
    ) || raw.len() > MAX_SNAPSHOT_BYTES
    {
        return;
    }
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
fn native_refresh(
    handle: rune::AppHandle<State>,
    alive: std::sync::Arc<std::sync::atomic::AtomicBool>,
    action_rx: std::sync::mpsc::Receiver<(String, String)>,
) {
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
        ("worker", "/api/v1/runtime-worker/status"),
        ("workers", "/api/v1/runtime/workers"),
        ("operations", "/api/v1/runtime/operations"),
        ("snapshots", "/api/v1/runtime/snapshots"),
        ("mounts", "/api/v1/runtime/mounts"),
    ];
    while alive.load(Ordering::Relaxed) {
        let mut last_csrf = String::new();
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
            if source == "health"
                && let Ok(v) = serde_json::from_str::<Value>(&raw)
                && let Some(tok) = v.get("csrf_token").and_then(Value::as_str)
            {
                last_csrf = tok.to_string();
            }
            let source = source.to_string();
            handle.update(move |state, tx| {
                state.ingest(&source, &raw);
                tx.instant();
            });
        }
        for _ in 0..60 {
            if !alive.load(Ordering::Relaxed) { break; }
            if let Ok((worker_id, action)) = action_rx.recv_timeout(Duration::from_millis(250))
                && !last_csrf.is_empty()
            {
                let resp = client
                    .post("http://127.0.0.1:41828/api/v1/runtime/workers/action")
                    .header("Origin", "http://127.0.0.1:41828")
                    .header("Content-Type", "application/json")
                    .header("X-Kitt-Csrf", &last_csrf)
                    .body(serde_json::json!({"worker_id": worker_id, "action": action}).to_string())
                    .send();
                let summary = match resp {
                    Ok(r) if r.status().is_success() => format!("Ação '{action}' concluída"),
                    Ok(r) => format!("Erro HTTP {} em '{action}'", r.status().as_u16()),
                    Err(e) => format!("Falha ao enviar '{action}': {e}"),
                };
                handle.update(move |st, tx| {
                    st.last_action_status = summary;
                    tx.instant();
                });
                break;
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(start))]
pub fn start() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        let app = App::new(AppSpec::new("K.I.T.T. / Rune", (1180.0, 740.0), State::default, view));
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let alive = running.clone();
        let handle = app.handle();
        handle.update(move |state, _| state.action_tx = Some(tx));
        std::thread::spawn(move || native_refresh(handle, alive, rx));
        app.run();
        running.store(false, std::sync::atomic::Ordering::Relaxed);
    }
    #[cfg(target_arch = "wasm32")]
    {
        let app = App::new(AppSpec::new("K.I.T.T. / Rune", (1180.0, 740.0), State::default, view));
        HANDLE.with(|state| *state.borrow_mut() = Some(app.handle()));
        app.run();
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

    #[test]
    fn workers_fleet_filter_select_and_action_queue_work() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut state = State { action_tx: Some(tx), ..State::default() };
        state.ingest(
            "workers",
            r#"{"workers":[{"id":"w1","label":"GPU Node","os":"linux","backend":"remote","available":true,"state":"RUNNING","cpu_millis":2000,"memory_bytes":536870912,"gpu_count":1,"gpu_detected_count":1,"gpu_vendor":"amd","active_leases":1,"executions":5,"execution_failures":0,"cache_bytes":4096}],"operations":[{"worker_id":"w1","action":"resume","ok":true,"summary":"RUNNING"}]}"#,
        );
        assert_eq!(state.data.workers.len(), 1);
        assert_eq!(state.selected_worker, "w1");
        assert!(WorkerFilter::Running.matches("RUNNING"));
        assert!(!WorkerFilter::Paused.matches("RUNNING"));
        state.trigger_action("pause");
        assert_eq!(rx.try_recv().unwrap(), ("w1".to_string(), "pause".to_string()));
    }
}
