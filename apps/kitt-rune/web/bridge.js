// Same-origin, read-only bridge. No secrets, browser credentials, or tool execution.
import init, { update_snapshot } from "./app.js";

const SOURCES = Object.freeze({
  health: "/api/v1/health",
  service: "/api/v1/service/status",
  proxy: "/api/v1/reverse-proxy/status",
  worker: "/api/v1/runtime-worker/status",
});
const LIMIT = 262_144;
let active = false;

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
  if (document.hidden || !active) return;
  await Promise.all(Object.entries(SOURCES).map(([k,v]) => request(k,v)));
}

async function boot() {
  try {
    await init(); // invokes the #[wasm_bindgen(start)] Rune app
    active = true;
    document.getElementById("fallback")?.remove();
    await refresh();
    setInterval(refresh, 15000);
    document.addEventListener("visibilitychange", () => { if (!document.hidden) void refresh(); });
  } catch (error) {
    const fallback = document.getElementById("fallback");
    if (fallback) fallback.textContent =
      "WebGPU indisponível ou Rune não inicializado. Use o Control Center clássico. " +
      String(error).slice(0, 160);
  }
}
void boot();
