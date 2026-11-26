// Same-origin bridge for Rune WebGPU. No secrets or browser credentials.
import init, { update_snapshot } from "./app.js";

const SOURCES = Object.freeze({
  health: "/api/v1/health",
  service: "/api/v1/service/status",
  proxy: "/api/v1/reverse-proxy/status",
  worker: "/api/v1/runtime-worker/status",
  workers: "/api/v1/runtime/workers",
  operations: "/api/v1/runtime/operations",
  snapshots: "/api/v1/runtime/snapshots",
  mounts: "/api/v1/runtime/mounts",
});
const LIMIT = 262_144;
let active = false;
let pollMs = 15000;
let timerId = null;

window.__kittRuneAction = async function dispatchRuntimeAction(workerId, action) {
  try {
    const resp = await fetch("/api/v1/runtime/workers/action", {
      method: "POST",
      mode: "same-origin",
      credentials: "same-origin",
      cache: "no-store",
      redirect: "error",
      headers: {
        "Content-Type": "application/json",
        "X-Kitt-Rune-Action": "1",
      },
      body: JSON.stringify({ worker_id: String(workerId), action: String(action) }),
    });
    await refresh();
    return resp.ok;
  } catch {
    return false;
  }
};

async function request(name, path) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 4000);
  try {
    const response = await fetch(path, {
      method: "GET",
      mode: "same-origin",
      credentials: "same-origin",
      cache: "no-store",
      redirect: "error",
      signal: controller.signal,
      headers: { Accept: "application/json" },
    });
    if (!response.ok) throw new Error("HTTP " + response.status);
    const length = Number(response.headers.get("content-length") || 0);
    if (length > LIMIT) throw new Error("Response too large");
    const reader = response.body?.getReader();
    if (!reader) throw new Error("Missing body");
    let size = 0;
    const chunks = [];
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > LIMIT) { await reader.cancel(); throw new Error("Response too large"); }
      chunks.push(value);
    }
    const data = new Uint8Array(size);
    let offset = 0;
    for (const part of chunks) { data.set(part, offset); offset += part.length; }
    update_snapshot(name, new TextDecoder().decode(data));
  } catch (_) {
    update_snapshot(name, '{"available":false}');
  } finally {
    clearTimeout(timer);
  }
}

async function refresh() {
  if (document.hidden || !active || !navigator.onLine) return;
  let hadError = false;
  await Promise.all(Object.entries(SOURCES).map(async ([k, v]) => {
    try { await request(k, v); } catch { hadError = true; }
  }));
  pollMs = hadError ? Math.min(60000, pollMs * 2) : 15000;
  scheduleNext();
}

function scheduleNext() {
  if (timerId) clearTimeout(timerId);
  if (!document.hidden && active) {
    timerId = setTimeout(refresh, pollMs);
  }
}

async function boot() {
  try {
    await init(); // invokes the #[wasm_bindgen(start)] Rune app
    active = true;
    document.getElementById("fallback")?.remove();
    await refresh();
    document.addEventListener("visibilitychange", () => { if (!document.hidden) void refresh(); });
    window.addEventListener("online", () => { pollMs = 15000; void refresh(); });
  } catch (error) {
    const fallback = document.getElementById("fallback");
    if (fallback) fallback.textContent =
      "WebGPU indisponível ou Rune não inicializado. Use o Control Center clássico. " +
      String(error).slice(0, 160);
  }
}
void boot();
