"use strict";

const state = {
  view: "config",
  catalog: null,
  snapshot: null,
  serviceStatus: null,
  serviceLogs: "",
  monitorAutoRefresh: true,
  monitorTimer: null,
  monitorTick: 0,
  statusRequestInFlight: false,
  logsRequestInFlight: false,
  showAdvanced: false,
  searchTimer: null,
  pending: new Map(),
  section: null,
  csrf: null,
  modelsCache: new Map(),
  modelDiscoveryGeneration: new Map(),
  customInputMode: new Set()
};

const $ = (id) => document.getElementById(id);

async function api(path, options = {}, isRetry = false) {
  const headers = {
    "Accept": "application/json",
    ...(options.body ? { "Content-Type": "application/json" } : {}),
    ...(options.headers || {})
  };
  if (options.method && !["GET", "HEAD"].includes(options.method)) {
    if (!state.csrf) {
      try {
        const h = await fetch("/api/v1/health", { headers: { "Accept": "application/json" }, credentials: "same-origin" }).then((r) => r.json());
        if (h.csrf_token) state.csrf = h.csrf_token;
      } catch (_) {}
    }
    if (state.csrf) {
      headers["X-KITT-CSRF"] = state.csrf;
    }
  }
  const response = await fetch(path, { ...options, headers, credentials: "same-origin" });
  const payload = await response.json().catch(() => ({}));
  if (payload.csrf_token) state.csrf = payload.csrf_token;
  if (!response.ok) {
    if (response.status === 403 && payload.error && payload.error.includes("CSRF") && !isRetry) {
      try {
        const h = await fetch("/api/v1/health", { headers: { "Accept": "application/json" }, credentials: "same-origin" }).then((r) => r.json());
        if (h.csrf_token) {
          state.csrf = h.csrf_token;
          return await api(path, options, true);
        }
      } catch (_) {}
    }
    throw new Error(payload.error || `HTTP ${response.status}`);
  }
  return payload;
}

function esc(value) {
  return String(value ?? "").replace(/[&<>"']/g, (c) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    "\"": "&quot;",
    "'": "&#39;"
  }[c]));
}

function key(section, field) {
  return `${section.id}::${field.key}`;
}

function current(section, field) {
  const k = key(section, field);
  if (state.pending.has(k)) return state.pending.get(k);
  return state.snapshot?.values?.[section.id]?.[field.key] ?? field.default ?? null;
}

function isChanged(section, field) {
  return state.pending.has(key(section, field));
}

function isModelField(field) {
  const k = field.key || "";
  if (k === "stt_worker_model") return false;
  return k === "model" || k.endsWith(".model") || k.endsWith("_model");
}

function modelCacheKey(sectionId, fieldKey) {
  return `${sectionId}::${fieldKey}`;
}

function bumpModelDiscoveryGeneration(sectionId, fieldKey) {
  const cacheKey = modelCacheKey(sectionId, fieldKey);
  const next = (state.modelDiscoveryGeneration.get(cacheKey) || 0) + 1;
  state.modelDiscoveryGeneration.set(cacheKey, next);
  return next;
}

function isCurrentModelDiscovery(sectionId, fieldKey, generation) {
  return state.modelDiscoveryGeneration.get(modelCacheKey(sectionId, fieldKey)) === generation;
}

function modelBaseUrlCandidates(modelFieldKey) {
  const candidates = [];
  if (modelFieldKey.endsWith(".model")) {
    candidates.push(`${modelFieldKey.slice(0, -".model".length)}.base_url`);
  } else if (modelFieldKey.endsWith("_model")) {
    candidates.push(`${modelFieldKey.slice(0, -"_model".length)}_base_url`);
  }
  candidates.push("base_url", "ollama_url");
  return candidates;
}

function modelBaseUrlFieldKey(section, modelFieldKey) {
  return modelBaseUrlCandidates(modelFieldKey)
    .find((candidate) => section.fields.some((field) => field.key === candidate)) || null;
}

function invalidateModelCachesForChangedField(section, changedFieldKey) {
  for (const modelField of section.fields.filter(isModelField)) {
    if (modelBaseUrlFieldKey(section, modelField.key) === changedFieldKey) {
      state.modelsCache.delete(modelCacheKey(section.id, modelField.key));
      state.customInputMode.delete(modelCacheKey(section.id, modelField.key));
      bumpModelDiscoveryGeneration(section.id, modelField.key);
    }
  }
}

function getSectionBaseUrl(sectionId, modelFieldKey) {
  const s = state.catalog?.sections.find((sec) => sec.id === sectionId);
  if (!s) return "http://127.0.0.1:11434/v1";

  for (const candidate of modelBaseUrlCandidates(modelFieldKey)) {
    const f = s.fields.find((field) => field.key === candidate);
    if (f) {
      const val = current(s, f);
      if (val && typeof val === "string" && val.trim()) return val.trim();
    }
  }
  return "http://127.0.0.1:11434/v1";
}

function control(section, field) {
  const k = key(section, field);
  const value = current(section, field);
  const attr = `data-key="${esc(k)}" data-section="${esc(section.id)}" data-field="${esc(field.key)}"`;

  if (field.type === "boolean") {
    return `<label class="switch"><input type="checkbox" ${attr} ${value ? "checked" : ""}><span></span></label>`;
  }

  if (field.type === "enum") {
    return `<select ${attr}>${(field.options || []).map((o) => `<option value="${esc(o)}" ${String(value) === o ? "selected" : ""}>${esc(o)}</option>`).join("")}</select>`;
  }

  const inputType = field.type === "integer" || field.type === "number" ? "number" : "text";
  const min = field.minimum !== undefined ? `min="${field.minimum}"` : "";
  const max = field.maximum !== undefined ? `max="${field.maximum}"` : "";
  const step = field.type === "number" ? 'step="any"' : "";
  const shown = field.type === "string_list" && Array.isArray(value) ? value.join(", ") : (value ?? "");

  if (isModelField(field)) {
    const cKey = modelCacheKey(section.id, field.key);
    const cachedModels = state.modelsCache.get(cKey) || [];
    const isCustom = state.customInputMode.has(cKey);
    const listId = `list-${esc(k.replace(/::/g, "_"))}`;

    if (cachedModels.length > 0 && !isCustom) {
      const hasCurrentInList = cachedModels.includes(String(shown));
      return `<div class="model-picker-wrap">
        <select ${attr} class="model-select">
          ${!hasCurrentInList && shown ? `<option value="${esc(shown)}" selected>${esc(shown)} (atual)</option>` : ""}
          <option value="" disabled ${!shown ? "selected" : ""}>-- Selecione um modelo --</option>
          ${cachedModels.map((m) => `<option value="${esc(m)}" ${String(shown) === m ? "selected" : ""}>${esc(m)}</option>`).join("")}
          <option value="__custom_mode__">✏️ Digitar modelo customizado...</option>
        </select>
        <button type="button" class="btn-discover" data-discover-section="${esc(section.id)}" data-discover-field="${esc(field.key)}" title="Atualizar modelos disponíveis">🔄 Atualizar</button>
      </div>`;
    }

    const datalistOptions = cachedModels.map((m) => `<option value="${esc(m)}">${esc(m)}</option>`).join("");
    return `<div class="model-picker-wrap">
      <input type="${inputType}" ${attr} list="${listId}" ${min} ${max} ${step} value="${esc(shown)}" placeholder="${esc(field.placeholder || "Selecione ou digite o modelo")}" autocomplete="off">
      <datalist id="${listId}">${datalistOptions}</datalist>
      <button type="button" class="btn-discover" data-discover-section="${esc(section.id)}" data-discover-field="${esc(field.key)}" title="Listar modelos disponíveis no provedor">🔍 Listar Modelos</button>
      ${cachedModels.length > 0 ? `<button type="button" class="ghost btn-toggle-select" data-section="${esc(section.id)}" data-field="${esc(field.key)}" title="Voltar para lista de seleção">▼ Lista</button>` : ""}
    </div>`;
  }

  return `<input type="${inputType}" ${attr} ${min} ${max} ${step} value="${esc(shown)}" placeholder="${esc(field.placeholder || "")}" autocomplete="off">`;
}

async function discoverModels(sectionId, fieldKey, btnEl = null) {
  const baseUrl = getSectionBaseUrl(sectionId, fieldKey);
  const generation = bumpModelDiscoveryGeneration(sectionId, fieldKey);
  if (btnEl) {
    btnEl.classList.add("loading");
    btnEl.textContent = "⏳ Buscando...";
  }

  try {
    const result = await api("/api/v1/models/discover", {
      method: "POST",
      body: JSON.stringify({ base_url: baseUrl })
    });
    if (!isCurrentModelDiscovery(sectionId, fieldKey, generation)) return;

    const models = result.models || [];
    state.modelsCache.set(modelCacheKey(sectionId, fieldKey), models);
    state.customInputMode.delete(modelCacheKey(sectionId, fieldKey));

    const listId = `list-${sectionId}_${fieldKey}`;
    const datalist = $(listId);
    if (datalist) {
      datalist.innerHTML = models.map((m) => `<option value="${esc(m)}">${esc(m)}</option>`).join("");
    }

    render();

    if (models.length) {
      toast(`✨ ${models.length} modelos encontrados em ${baseUrl}!`);
    } else {
      toast(`Nenhum modelo retornado por ${baseUrl}. Verifique se o servidor está ativo.`, "bad");
    }
  } catch (e) {
    if (isCurrentModelDiscovery(sectionId, fieldKey, generation)) {
      toast(`Falha ao listar modelos de ${baseUrl}: ${e.message}`, "bad");
    }
  } finally {
    if (btnEl && isCurrentModelDiscovery(sectionId, fieldKey, generation)) {
      btnEl.classList.remove("loading");
      btnEl.textContent = state.modelsCache.get(modelCacheKey(sectionId, fieldKey))?.length ? "🔄 Atualizar" : "🔍 Listar Modelos";
    }
  }
}

function formatUptime(secs) {
  if (secs === undefined || secs === null) return "–";
  const s = Number(secs);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const remS = s % 60;
  if (m < 60) return `${m}m ${remS}s`;
  const h = Math.floor(m / 60);
  const remM = m % 60;
  return `${h}h ${remM}m`;
}

function formatBytes(bytes) {
  if (!bytes) return "0 B";
  const n = Number(bytes);
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

function highlightLogs(text) {
  if (!text) return '<span class="log-dim">(Nenhum log retornado pelo journalctl)</span>';
  return text.split("\n").map((line) => {
    if (!line.trim()) return "";
    const escaped = esc(line);
    let cls = "log-line";
    if (/error|fatal|falha|panic|refused/i.test(line)) cls += " log-error";
    else if (/warn|warning|aviso|degraded/i.test(line)) cls += " log-warn";
    else if (/wake phrase matched|listening for command/i.test(line)) cls += " log-wake";
    else if (/heard transcript|executing command/i.test(line)) cls += " log-transcript";
    else if (/voice answer/i.test(line)) cls += " log-answer";
    else if (/started local STT|faster-whisper model ready|kitt voice enabled/i.test(line)) cls += " log-info";
    return `<span class="${cls}">${escaped}</span>`;
  }).join("\n");
}

async function fetchServiceStatus() {
  if (document.hidden || state.statusRequestInFlight) return state.serviceStatus;
  state.statusRequestInFlight = true;
  try {
    const status = await api("/api/v1/service/status");
    state.serviceStatus = status;
    const dot = $("daemon-dot");
    const statusText = $("daemon-status");
    if (dot) dot.style.background = status.daemon?.active ? "var(--good)" : "var(--bad)";
    if (statusText) statusText.textContent = status.daemon?.active ? `kittd (PID ${status.daemon.pid})` : "kittd offline";
    return status;
  } catch (e) {
    const dot = $("daemon-dot");
    const statusText = $("daemon-status");
    if (dot) dot.style.background = "var(--bad)";
    if (statusText) statusText.textContent = "kittd desconectado";
    return null;
  } finally {
    state.statusRequestInFlight = false;
  }
}

async function fetchServiceLogs() {
  if (document.hidden || state.logsRequestInFlight || state.view !== "monitor") return state.serviceLogs;
  state.logsRequestInFlight = true;
  try {
    const payload = await api("/api/v1/service/logs");
    state.serviceLogs = payload.logs || "";
    const pre = $("service-logs");
    if (pre) pre.innerHTML = highlightLogs(state.serviceLogs || "Nenhum log disponível.");
    return state.serviceLogs;
  } catch (error) {
    console.warn("KITT service logs refresh failed:", error);
    return state.serviceLogs;
  } finally {
    state.logsRequestInFlight = false;
  }
}

function agentWebUrl() {
  const section = state.catalog?.sections.find((s) => s.id === "agent.remote");
  const values = state.snapshot?.values?.["agent.remote"] || {};
  const fieldDefault = (key, fallback) => section?.fields.find((f) => f.key === key)?.default ?? fallback;
  let host = values.host ?? fieldDefault("host", "127.0.0.1");
  if (host === "0.0.0.0" || host === "::") host = "127.0.0.1";
  const port = values.port ?? fieldDefault("port", 7337);
  const tlsCert = values.tls_cert ?? fieldDefault("tls_cert", "");
  const tlsKey = values.tls_key ?? fieldDefault("tls_key", "");
  const scheme = tlsCert && tlsKey ? "https" : "http";
  const normalizedHost = host === "::1" ? "[::1]" : host;
  return `${scheme}://${normalizedHost}:${port}/`;
}

function openAgentWeb() {
  window.open(agentWebUrl(), "_blank", "noopener,noreferrer");
}

async function pingService() {
  const start = performance.now();
  try {
    const res = await api("/api/v1/service/ping", { method: "POST" });
    const elapsed = Math.round(performance.now() - start);
    if (res.pong) {
      toast(`📡 Ping OK! Latência: ${elapsed}ms`);
    } else {
      toast(`Resposta inesperada: ${JSON.stringify(res)}`, "bad");
    }
  } catch (e) {
    toast(`Falha no ping: ${e.message}`, "bad");
  }
}

async function restartService() {
  if (!confirm("Deseja realmente reiniciar o serviço kitt-assistant.service?")) return;
  try {
    toast("Enviando comando de reinício para o systemd...", "good");
    const res = await api("/api/v1/service/restart", { method: "POST" });
    toast(res.message || "Serviço reiniciado com sucesso!");
    setTimeout(async () => {
      await fetchServiceStatus();
      render();
    }, 2000);
  } catch (e) {
    toast(`Erro ao reiniciar serviço: ${e.message}`, "bad");
  }
}

function render() {
  renderNav();
  renderTopbar();

  if (state.view === "monitor") {
    renderMonitorView();
    return;
  }

  renderConfigView();
}

const COMPONENT_LABELS = {
  "kitt-assistant": "Assistant",
  "kitt-agent-cli": "Agent",
  "kitt-memory": "Memory",
  "kitt-ai-workers": "AI Workers",
  "kitt-toolbox": "Toolbox",
  "kitt-reverse-proxy": "Reverse Proxy",
  "kitt-protocol": "Protocol",
  "kitt": "Ecosystem"
};

function componentLabel(component) {
  return COMPONENT_LABELS[component]
    || String(component || "Outros")
      .replace(/^kitt-/, "")
      .split(/[-_.]/)
      .filter(Boolean)
      .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
      .join(" ");
}

function groupSectionsByComponent(sections) {
  const grouped = new Map();
  for (const section of sections) {
    const component = section.component || "other";
    if (!grouped.has(component)) grouped.set(component, []);
    grouped.get(component).push(section);
  }
  return Array.from(grouped, ([component, items]) => ({
    component,
    label: componentLabel(component),
    sections: items
  }));
}

function shouldPollMonitorStatus() {
  return !document.hidden && state.view === "monitor";
}

function shouldPollMonitorLogs() {
  return shouldPollMonitorStatus()
    && state.monitorAutoRefresh
    && state.monitorTick > 0
    && state.monitorTick % 3 === 0;
}

async function navigateTo(target) {
  const searchEl = $("search");
  if (searchEl) searchEl.value = "";

  if (target === "__monitor__") {
    state.view = "monitor";
    state.monitorTick = 0;
    render();
    await Promise.all([fetchServiceStatus(), fetchServiceLogs()]);
    if (state.view === "monitor") render();
    return;
  }

  state.view = "config";
  state.section = target;
  render();
}

function renderNav() {
  const sections = state.catalog?.sections || [];
  if (state.view === "config" && !state.section && sections.length) {
    state.section = sections[0].id;
  }
  const daemonOnline = state.serviceStatus?.daemon?.active ?? (state.snapshot !== null);
  const groups = groupSectionsByComponent(sections);

  const monitorBtn = `
    <div class="nav-category">Sistema</div>
    <button class="nav-item nav-monitor ${state.view === "monitor" ? "active" : ""}" data-view="monitor">
      <div class="nav-title-group">
        <span class="nav-icon">📊</span>
        <span>Monitor de Serviço</span>
      </div>
      <span class="dot-indicator ${daemonOnline ? "good" : "bad"}"></span>
    </button>
  `;

  const sectionBtns = groups.map((group) => `
    <div class="nav-category">${esc(group.label)}</div>
    ${group.sections.map((s) => `
      <button class="nav-item ${state.view === "config" && s.id === state.section ? "active" : ""}" data-nav="${esc(s.id)}">
        <span>${esc(s.title)}</span>
        <span class="count">${s.fields.length}</span>
      </button>
    `).join("")}
  `).join("");

  const navEl = $("nav");
  if (navEl) navEl.innerHTML = monitorBtn + sectionBtns;

  const mobileNav = $("mobile-nav-select");
  if (mobileNav) {
    mobileNav.innerHTML = `
      <option value="__monitor__">Monitor de Serviço</option>
      ${groups.map((group) => `
        <optgroup label="${esc(group.label)}">
          ${group.sections.map((s) => `<option value="${esc(s.id)}">${esc(s.title)}</option>`).join("")}
        </optgroup>
      `).join("")}
    `;
    mobileNav.value = state.view === "monitor"
      ? "__monitor__"
      : (state.section || sections[0]?.id || "__monitor__");
    mobileNav.onchange = () => { void navigateTo(mobileNav.value); };
  }

  bindNav();
}

function renderTopbar() {
  const eyebrowEl = $("top-eyebrow");
  const titleEl = $("top-title");
  const actionsEl = $("top-actions");

  if (state.view === "monitor") {
    if (eyebrowEl) eyebrowEl.textContent = "TELEMETRIA & STATUS EM TEMPO REAL";
    if (titleEl) titleEl.textContent = "Monitor do Ecossistema KITT";
    if (actionsEl) {
      actionsEl.innerHTML = `
        <button id="btn-agent-web" class="ghost" title="Abrir KITT Agent Web (use 'kitt web' se estiver offline)">↗ Agent Web</button>
        <button id="btn-ping" class="ghost" title="Testar comunicação IPC/HTTP com o daemon">📡 Testar Ping</button>
        <button id="btn-restart" class="ghost btn-restart-action" title="Reiniciar kitt-assistant.service">⚡ Reiniciar KITT</button>
        <button id="btn-refresh" class="primary" title="Atualizar dados e logs agora">🔄 Atualizar</button>
      `;
      const btnAgentWeb = $("btn-agent-web");
      if (btnAgentWeb) btnAgentWeb.addEventListener("click", openAgentWeb);
      const btnPing = $("btn-ping");
      if (btnPing) btnPing.addEventListener("click", pingService);
      const btnRestart = $("btn-restart");
      if (btnRestart) btnRestart.addEventListener("click", restartService);
      const btnRefresh = $("btn-refresh");
      if (btnRefresh) btnRefresh.addEventListener("click", async () => {
        await Promise.all([fetchServiceStatus(), fetchServiceLogs()]);
        render();
        toast("Status atualizado!");
      });
    }
  } else {
    if (eyebrowEl) eyebrowEl.textContent = "LOCAL • LOOPBACK ONLY";
    if (titleEl) titleEl.textContent = "Configurações do ecossistema";
    if (actionsEl) {
      actionsEl.innerHTML = `
        <button id="btn-agent-web" class="ghost" title="Abrir KITT Agent Web (use 'kitt web' se estiver offline)">↗ Agent Web</button>
        <button id="btn-advanced" class="ghost">${state.showAdvanced ? "Ocultar avançado" : "Avançado"}</button>
        <button id="reset-all" class="ghost">Descartar</button>
        <button id="apply-all" class="primary" disabled>Aplicar alterações</button>
      `;
      const btnAgentWeb = $("btn-agent-web");
      if (btnAgentWeb) btnAgentWeb.addEventListener("click", openAgentWeb);
      const btnAdvanced = $("btn-advanced");
      if (btnAdvanced) btnAdvanced.addEventListener("click", () => {
        state.showAdvanced = !state.showAdvanced;
        render();
      });
      const btnReset = $("reset-all");
      if (btnReset) btnReset.addEventListener("click", () => {
        state.pending.clear();
        render();
      });
      const btnApply = $("apply-all");
      if (btnApply) btnApply.addEventListener("click", preview);
      btnReset.disabled = state.pending.size === 0;
      btnApply.disabled = state.pending.size === 0;
    }
  }
}

function renderMonitorView() {
  const status = state.serviceStatus;
  const overviewEl = $("overview");
  const contentEl = $("content");

  if (!status) {
    if (contentEl) {
      contentEl.innerHTML = `<div class="monitor-loading"><span class="loading-spinner"></span> Carregando telemetria do KITT...</div>`;
    }
    fetchServiceStatus().then(() => {
      if (state.view === "monitor") render();
    });
    return;
  }

  const d = status.daemon || {};
  const v = status.voice || {};
  const m = status.models || {};
  const mem = status.memory || {};

  if (overviewEl) {
    overviewEl.innerHTML = [
      ["Daemon", d.active ? `Online (PID ${d.pid})` : "Offline", d.active ? "ok" : "bad"],
      ["Tempo Ativo", formatUptime(d.uptime_seconds), "ok"],
      ["Worker STT", v.stt_worker_online ? "127.0.0.1:8000 Ativo" : (v.stt_worker_model ? "Offline" : "Sem modelo"), v.stt_worker_online ? "ok" : "warn"],
      ["Memória SQLite", mem.exists ? formatBytes(mem.size_bytes) : "Não criado", ""]
    ].map(([l, val, c]) => `<div class="metric"><small>${esc(l)}</small><strong class="${c}">${esc(val)}</strong></div>`).join("");
  }

  const voiceModelDisplay = v.stt_worker_model
    ? `<span class="tag good">${esc(v.stt_worker_model)}</span>`
    : `<span class="tag warn">⚠️ Vazio (Defina na aba Voz)</span>`;

  const voiceWorkerDisplay = v.stt_worker_online
    ? `<span class="status-pill good">● Respondendo (Porta 8000)</span>`
    : `<span class="status-pill bad">● Indisponível</span>`;

  const wakewordDisplay = v.wakeword_model_exists
    ? `<span class="status-pill good">● Presente (${esc(v.wakeword_model_path)})</span>`
    : `<span class="status-pill neutral">○ Não encontrado (Usando fallback transcrição)</span>`;

  if (contentEl) {
    contentEl.innerHTML = `
      <div class="monitor-grid">
        <!-- Daemon Card -->
        <article class="section-card monitor-card">
          <header class="section-head">
            <div>
              <h2>Daemon KITT (<code>kittd</code>)</h2>
              <p>Processo principal residente e roteador de serviços</p>
            </div>
            <span class="badge ${d.active ? "badge-good" : "badge-bad"}">${d.active ? "● Em Execução" : "● Parado"}</span>
          </header>
          <div class="monitor-details">
            <div class="detail-item"><span class="detail-label">PID</span><strong class="detail-value">${esc(d.pid || "–")}</strong></div>
            <div class="detail-item"><span class="detail-label">Tempo de Atividade</span><strong class="detail-value">${esc(formatUptime(d.uptime_seconds))}</strong></div>
            <div class="detail-item"><span class="detail-label">Socket IPC</span><strong class="detail-value">${esc(d.listen || "127.0.0.1:41827")}</strong></div>
            <div class="detail-item"><span class="detail-label">Painel Web</span><strong class="detail-value">http://${esc(d.bind || "127.0.0.1:41828")}/</strong></div>
            <div class="detail-item"><span class="detail-label">Versão</span><strong class="detail-value">v${esc(d.version || "0.1.0")}</strong></div>
            <div class="detail-item"><span class="detail-label">Segurança</span><strong class="detail-value text-good">Loopback isolado (127.0.0.1)</strong></div>
          </div>
        </article>

        <!-- Voice Card -->
        <article class="section-card monitor-card">
          <header class="section-head">
            <div>
              <h2>Reconhecimento de Voz & STT</h2>
              <p>Captura de microfone, wake words e transcrição Whisper</p>
            </div>
            <span class="badge ${v.enabled ? "badge-good" : "badge-neutral"}">${v.enabled ? "Ativo" : "Desativado"}</span>
          </header>
          <div class="monitor-details">
            <div class="detail-item"><span class="detail-label">Modo de Ativação</span><strong class="detail-value">${esc(v.activation_mode || "auto")}</strong></div>
            <div class="detail-item"><span class="detail-label">Modelo STT (Whisper)</span><div class="detail-value">${voiceModelDisplay}</div></div>
            <div class="detail-item"><span class="detail-label">Worker STT Local</span><div class="detail-value">${voiceWorkerDisplay}</div></div>
            <div class="detail-item"><span class="detail-label">Modelo Wakeword (.rpw)</span><div class="detail-value">${wakewordDisplay}</div></div>
            <div class="detail-item" style="grid-column: 1 / -1;"><span class="detail-label">Frases Ativadoras Ativas</span><strong class="detail-value">${esc((v.wake_phrases || []).join(", ") || "–")}</strong></div>
          </div>
        </article>

        <!-- Models Card -->
        <article class="section-card monitor-card">
          <header class="section-head">
            <div>
              <h2>Modelos de Linguagem & Provedor</h2>
              <p>Configuração de inferência LLM rápida e pesada</p>
            </div>
            <span class="badge">LLM</span>
          </header>
          <div class="monitor-details">
            <div class="detail-item" style="grid-column: 1 / -1;"><span class="detail-label">URL do Provedor</span><strong class="detail-value"><code>${esc(m.base_url || "Não configurado")}</code></strong></div>
            <div class="detail-item"><span class="detail-label">Modelo Rápido (Fast)</span><strong class="detail-value">${esc(m.fast_model || m.model || "–")}</strong></div>
            <div class="detail-item"><span class="detail-label">Modelo Pesado (Heavy)</span><strong class="detail-value">${esc(m.heavy_model || "–")}</strong></div>
          </div>
        </article>

        <!-- Storage Card -->
        <article class="section-card monitor-card">
          <header class="section-head">
            <div>
              <h2>Memória & Armazenamento</h2>
              <p>Base de conhecimento SQLite e arquivos temporários</p>
            </div>
            <span class="badge">SQLite</span>
          </header>
          <div class="monitor-details">
            <div class="detail-item"><span class="detail-label">Banco de Memória</span><strong class="detail-value"><code>memory.db</code> (${mem.exists ? formatBytes(mem.size_bytes) : "Não criado"})</strong></div>
            <div class="detail-item"><span class="detail-label">Cache de Áudio</span><strong class="detail-value"><code>voice-cache/</code> (Transitório)</strong></div>
          </div>
        </article>
      </div>

      <!-- Live Logs Card -->
      <article class="section-card logs-card">
        <header class="section-head logs-head">
          <div class="logs-head-title">
            <span class="scanner-mini"></span>
            <div>
              <h2>Logs em Tempo Real do Serviço (<code>journalctl</code>)</h2>
              <p>Últimos eventos do daemon <code>kitt-assistant.service</code></p>
            </div>
          </div>
          <div class="logs-controls">
            <label class="toggle-auto"><input type="checkbox" id="chk-auto-refresh" ${state.monitorAutoRefresh ? "checked" : ""}><span>Auto-atualizar logs (15s)</span></label>
            <button type="button" id="btn-scroll-bottom" class="ghost" style="padding: 4px 10px; font-size: 11px;">⬇ Rolar ao fim</button>
          </div>
        </header>
        <div class="logs-body">
          <pre id="service-logs" class="service-logs">${highlightLogs(state.serviceLogs || "Nenhum log disponível.")}</pre>
        </div>
      </article>
    `;

    const chkAuto = $("chk-auto-refresh");
    if (chkAuto) {
      chkAuto.addEventListener("change", () => {
        state.monitorAutoRefresh = chkAuto.checked;
      });
    }

    const btnScroll = $("btn-scroll-bottom");
    const preLogs = $("service-logs");
    if (btnScroll && preLogs) {
      btnScroll.addEventListener("click", () => {
        preLogs.scrollTop = preLogs.scrollHeight;
      });
    }
  }
}

function renderConfigView() {
  const sections = state.catalog?.sections || [];
  if (!state.section && sections.length) state.section = sections[0].id;

  const filter = $("search")?.value.trim().toLowerCase() || "";
  const visible = sections
    .filter((s) => !filter || s.title.toLowerCase().includes(filter) || s.component.toLowerCase().includes(filter) || s.fields.some((f) => (f.label + " " + (f.description || "") + " " + f.key).toLowerCase().includes(filter)))
    .filter((s) => filter || s.id === state.section);

  const overviewEl = $("overview");
  if (overviewEl && state.catalog) {
    overviewEl.innerHTML = [
      ["Daemon", state.serviceStatus?.daemon?.active ? "Online" : "Conectado", "ok"],
      ["Componentes", String(new Set(state.catalog.sections.map((s) => s.component)).size), ""],
      ["Seções", String(state.catalog.sections.length), ""],
      ["Modo", state.serviceStatus?.daemon?.bind || "loopback", "ok"]
    ].map(([l, v, c]) => `<div class="metric"><small>${esc(l)}</small><strong class="${c}">${esc(v)}</strong></div>`).join("");
  }

  const contentEl = $("content");
  if (contentEl) {
    contentEl.innerHTML = visible.map((section) => {
      const fields = section.fields.filter((f) => {
        const matches = !filter || (f.label + " " + (f.description || "") + " " + f.key + " " + section.title).toLowerCase().includes(filter);
        return matches && (filter || state.showAdvanced || !f.advanced);
      });
      const changed = fields.some((f) => isChanged(section, f));

      return `
        <article class="section-card">
          <header class="section-head">
            <div>
              <h2>${esc(section.title)}</h2>
              <p>${esc(section.description || section.component)}</p>
            </div>
            <span class="badge ${changed ? "changed" : ""}">${changed ? "Modificado" : esc(section.component)}</span>
          </header>
          <div class="fields">
            ${fields.map((field) => `
              <div class="field ${field.advanced ? "advanced" : ""}">
                <div class="field-label">
                  <span>${esc(field.label)}</span>
                  ${field.apply_mode !== "live" ? `<span class="restart">${field.apply_mode === "daemon_restart" ? "REINICIA KITT" : "RESTART"}</span>` : ""}
                </div>
                <div class="control">${control(section, field)}</div>
                ${field.description ? `<p>${esc(field.description)}</p>` : ""}
              </div>
            `).join("")}
          </div>
        </article>
      `;
    }).join("") || `<div class="alert">Nenhuma configuração encontrada.</div>`;
  }

  const applyBtn = $("apply-all");
  const resetBtn = $("reset-all");
  if (applyBtn) applyBtn.disabled = state.pending.size === 0;
  if (resetBtn) resetBtn.disabled = state.pending.size === 0;
  const revEl = $("revision");
  if (revEl) revEl.textContent = `rev ${state.snapshot?.revision ?? "–"}`;

  bindInputs();
}

function bindInputs() {
  document.querySelectorAll("[data-key]").forEach((el) => {
    const handleValue = () => {
      const section = state.catalog.sections.find((s) => s.id === el.dataset.section);
      const field = section.fields.find((f) => f.key === el.dataset.field);
      let value = el.type === "checkbox" ? el.checked : el.value;

      if (value === "__custom_mode__") {
        state.customInputMode.add(modelCacheKey(section.id, field.key));
        render();
        const inputEl = document.querySelector(`[data-section="${section.id}"][data-field="${field.key}"]`);
        if (inputEl) inputEl.focus();
        return;
      }

      if (field.type === "integer") value = Number.parseInt(value, 10);
      if (field.type === "number") value = Number(value);
      if (field.type === "string_list") value = value.split(",").map((v) => v.trim()).filter(Boolean);

      state.pending.set(el.dataset.key, value);
      invalidateModelCachesForChangedField(section, field.key);
      const applyBtn = $("apply-all");
      const resetBtn = $("reset-all");
      if (applyBtn) applyBtn.disabled = state.pending.size === 0;
      if (resetBtn) resetBtn.disabled = state.pending.size === 0;
    };

    el.addEventListener("input", handleValue);
    el.addEventListener("change", () => {
      handleValue();
      render();
    });
  });

  document.querySelectorAll(".btn-toggle-select").forEach((btn) => btn.addEventListener("click", () => {
    const sec = btn.dataset.section;
    const fld = btn.dataset.field;
    state.customInputMode.delete(modelCacheKey(sec, fld));
    render();
  }));

  document.querySelectorAll("[data-discover-section]").forEach((btn) => btn.addEventListener("click", () => {
    const sec = btn.dataset.discoverSection;
    const fld = btn.dataset.discoverField;
    discoverModels(sec, fld, btn);
  }));
}

function bindNav() {
  document.querySelectorAll("[data-nav]").forEach((el) => el.addEventListener("click", () => {
    void navigateTo(el.dataset.nav);
  }));

  document.querySelectorAll("[data-view='monitor']").forEach((el) => el.addEventListener("click", () => {
    void navigateTo("__monitor__");
  }));
}

function toast(message, type = "good") {
  const alertsEl = $("alerts");
  if (!alertsEl) return;
  const node = document.createElement("div");
  node.className = `alert ${type}`;
  node.textContent = message;
  alertsEl.replaceChildren(node);
  setTimeout(() => node.remove(), 4500);
}

function changesObject() {
  const out = {};
  for (const [compound, value] of state.pending) {
    const split = compound.indexOf("::");
    const section = compound.slice(0, split);
    const field = compound.slice(split + 2);
    (out[section] ??= {})[field] = value;
  }
  return out;
}

async function preview() {
  try {
    const result = await api("/api/v1/validate", {
      method: "POST",
      body: JSON.stringify({
        expected_revision: state.snapshot.revision,
        changes: changesObject()
      })
    });
    const diffEl = $("diff");
    if (diffEl) diffEl.textContent = JSON.stringify(result.diff || changesObject(), null, 2);
    const dialogEl = $("diff-dialog");
    if (dialogEl) dialogEl.showModal();
  } catch (e) {
    toast(e.message, "bad");
  }
}

async function apply() {
  try {
    const result = await api("/api/v1/config", {
      method: "PUT",
      body: JSON.stringify({
        expected_revision: state.snapshot.revision,
        changes: changesObject()
      })
    });
    state.pending.clear();
    state.snapshot = result.snapshot || await api("/api/v1/config");
    render();
    toast(result.restart_required?.length ? `Aplicado com sucesso. Reinício necessário: ${result.restart_required.join(", ")}` : "Configuração aplicada com sucesso!");
  } catch (e) {
    toast(e.message, "bad");
  }
}

async function boot() {
  try {
    const [health, catalog, snapshot] = await Promise.all([
      api("/api/v1/health"),
      api("/api/v1/catalog"),
      api("/api/v1/config")
    ]);
    state.catalog = catalog;
    state.snapshot = snapshot;
    state.csrf = health.csrf_token || state.csrf;
    const daemonText = $("daemon-status");
    if (daemonText) daemonText.textContent = health.status === "ok" ? "kittd online" : "kittd degradado";
    fetchServiceStatus();
    render();

    // Status stays fresh every 5 s while the monitor is visible. Logs are
    // intentionally slower (15 s) and controlled independently by the toggle.
    if (!state.monitorTimer) {
      state.monitorTimer = setInterval(async () => {
        if (!shouldPollMonitorStatus()) return;

        state.monitorTick += 1;
        const previousLogs = $("service-logs");
        const wasAtBottom = previousLogs
          ? (previousLogs.scrollHeight - previousLogs.scrollTop <= previousLogs.clientHeight + 40)
          : false;

        await fetchServiceStatus();
        if (!shouldPollMonitorStatus()) return;
        renderMonitorView();

        if (shouldPollMonitorLogs()) {
          await fetchServiceLogs();
        }

        const currentLogs = $("service-logs");
        if (wasAtBottom && currentLogs) {
          currentLogs.scrollTop = currentLogs.scrollHeight;
        }
      }, 5000);
    }
  } catch (e) {
    const dot = $("daemon-dot");
    if (dot) dot.style.background = "var(--bad)";
    toast(`Falha ao carregar Control Center: ${e.message}`, "bad");
  }
}

const searchInput = $("search");
if (searchInput) {
  searchInput.addEventListener("input", () => {
    if (state.searchTimer) clearTimeout(state.searchTimer);
    state.searchTimer = setTimeout(() => {
      state.searchTimer = null;
      render();
    }, 120);
  });
}

const btnConfirm = $("confirm-apply");
if (btnConfirm) {
  btnConfirm.addEventListener("click", (e) => {
    e.preventDefault();
    const dialogEl = $("diff-dialog");
    if (dialogEl) dialogEl.close();
    apply();
  });
}

if (typeof window !== "undefined") {
  boot();
}
