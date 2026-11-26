// Local server for Rune WebGPU. Proxies only allowlisted read projections and
// explicit operator runtime actions via existing Control Center CSRF boundaries.
import http from "node:http";
import { createReadStream } from "node:fs";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_DIST = path.resolve(HERE, "../dist");
const MIME = Object.freeze({
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".wasm": "application/wasm",
  ".css": "text/css; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".ttf": "font/ttf",
  ".woff2": "font/woff2",
});
const API = Object.freeze({
  "/api/v1/health": "health",
  "/api/v1/service/status": "service",
  "/api/v1/reverse-proxy/status": "proxy",
  "/api/v1/runtime-worker/status": "worker",
  "/api/v1/runtime/workers": "workers",
  "/api/v1/runtime/operations": "operations",
  "/api/v1/runtime/snapshots": "snapshots",
  "/api/v1/runtime/mounts": "mounts",
});
const MAX_API_BYTES = 262_144;
const MAX_ACTION_BODY = 16_384;
const MAX_ASSET_BYTES = 64 * 1024 * 1024;
const ALLOWED_ACTIONS = new Set([
  "provision", "pause", "resume", "hibernate", "terminate", "health",
  "reconnect", "leases_reconcile", "leases_force_release", "migrate",
  "snapshot_export", "snapshot_import", "cas_gc",
]);

function plain(res, status, message) {
  res.writeHead(status, { "Content-Type": "text/plain; charset=utf-8",
    "Cache-Control": "no-store", "X-Content-Type-Options": "nosniff" });
  res.end(message);
}
function json(res, status, data) {
  res.writeHead(status, { "Content-Type": "application/json; charset=utf-8",
    "Cache-Control": "no-store", "X-Content-Type-Options": "nosniff" });
  res.end(JSON.stringify(data));
}
function safeProjection(route, source) {
  if (!source || typeof source !== "object" || Array.isArray(source)) return {};
  const s = source;
  if (route === "health") return { status: s.status === "ok" ? "ok" : "offline" };
  if (route === "service") return {
    daemon: { active: s.daemon?.active === true,
      version: String(s.daemon?.version || "").slice(0, 120),
      uptime_seconds: Number(s.daemon?.uptime_seconds) || 0 },
    voice: { enabled: s.voice?.enabled === true,
      stt_worker_online: s.voice?.stt_worker_online === true },
    models: { fast_model: String(s.models?.fast_model || "").slice(0, 120),
      heavy_model: String(s.models?.heavy_model || "").slice(0, 120) },
    memory: { exists: s.memory?.exists === true,
      size_bytes: Number(s.memory?.size_bytes) || 0 },
  };
  if (route === "worker") return { available: s.available === true,
    state: ["UNBOUND","RUNNING","PAUSED","FAILED","BUSY"].includes(s.state) ? s.state : "UNKNOWN",
    ...Object.fromEntries(["executions","execution_failures","cache_bytes","uploaded_bytes","downloaded_bytes"]
      .map(key => [key, Number.isSafeInteger(s[key]) && s[key] >= 0 ? s[key] : 0])) };
  if (route === "workers") return {
    status: "ok",
    os: String(s.os || "").slice(0, 32),
    workers: Array.isArray(s.workers) ? s.workers.slice(0, 16).map(w => ({
      id: String(w?.id || "").slice(0, 64),
      label: String(w?.label || "").slice(0, 64),
      os: String(w?.os || "").slice(0, 32),
      backend: String(w?.backend || "remote").slice(0, 32),
      available: w?.available === true,
      capacity_available: w?.capacity_available === true,
      state: String(w?.state || "UNAVAILABLE").slice(0, 24),
      cpu_millis: Number(w?.cpu_millis) || 0,
      memory_bytes: Number(w?.memory_bytes) || 0,
      gpu_count: Number(w?.gpu_count) || 0,
      gpu_detected_count: Number(w?.gpu_detected_count) || 0,
      gpu_vendor: String(w?.gpu_vendor || "none").slice(0, 24),
      active_leases: Number(w?.active_leases) || 0,
      executions: Number(w?.executions) || 0,
      execution_failures: Number(w?.execution_failures) || 0,
      cache_bytes: Number(w?.cache_bytes) || 0,
    })) : [],
    operations: Array.isArray(s.operations) ? s.operations.slice(0, 16).map(o => ({
      at_ms: Number(o?.at_ms) || 0,
      worker_id: String(o?.worker_id || "").slice(0, 64),
      action: String(o?.action || "").slice(0, 32),
      ok: o?.ok === true,
      summary: String(o?.summary || "").slice(0, 120),
    })) : [],
  };
  if (route === "operations") return {
    status: "ok",
    operations: Array.isArray(s.operations) ? s.operations.slice(0, 64).map(o => ({
      at_ms: Number(o?.at_ms) || 0,
      worker_id: String(o?.worker_id || "").slice(0, 64),
      action: String(o?.action || "").slice(0, 32),
      ok: o?.ok === true,
      summary: String(o?.summary || "").slice(0, 120),
    })) : [],
  };
  if (route === "snapshots") return {
    status: "ok",
    snapshots: Array.isArray(s.snapshots) ? s.snapshots.slice(0, 64).map(x => ({
      snapshot_id: String(x?.snapshot_id || "").slice(0, 80),
      workspace_id: String(x?.workspace_id || "").slice(0, 64),
      version: Number(x?.version) || 1,
      total_bytes: Number(x?.total_bytes) || 0,
    })) : [],
  };
  if (route === "mounts") return {
    status: "ok",
    mounts: Array.isArray(s.mounts) ? s.mounts.slice(0, 32).map(m => ({
      alias: String(m?.alias || "").slice(0, 64),
      kind: String(m?.kind || "").slice(0, 16),
      source: String(m?.source || "").slice(0, 120),
    })) : [],
  };
  return { api_online: s.api_online === true,
    managed: s.managed === true,
    phase: String(s.phase || "").slice(0, 120) };
}
export function createRuneServer({ port = 41931, controlPort = 41828,
  dist = DEFAULT_DIST, fetcher = fetch } = {}) {
  if (!Number.isInteger(port) || port < 0 || port > 65535) throw Error("invalid port");
  if (!Number.isInteger(controlPort) || controlPort < 1 || controlPort > 65535)
    throw Error("invalid Control Center port");
  const root = path.resolve(dist);
  return http.createServer(async (req, res) => {
    const addr = req.socket.localPort;
    if (!["127.0.0.1:" + addr, "localhost:" + addr, "[::1]:" + addr].includes(req.headers.host)) {
      return plain(res, 400, "Invalid Host");
    }
    let route;
    try { route = new URL(req.url || "/", "http://localhost"); }
    catch { return plain(res, 400, "Invalid URL"); }
    if (route.search || route.hash) return plain(res, 400, "Query not allowed");
    if (req.method === "POST" && route.pathname === "/api/v1/runtime/workers/action") {
      const origin = req.headers.origin;
      const expectedOrigins = new Set(["http://127.0.0.1:" + addr, "http://localhost:" + addr, "http://[::1]:" + addr]);
      if (!origin || !expectedOrigins.has(origin) || req.headers["x-kitt-rune-action"] !== "1") {
        return json(res, 403, { error: "cross-origin or missing action header" });
      }
      try {
        const chunks = [];
        let len = 0;
        for await (const ch of req) {
          len += ch.byteLength;
          if (len > MAX_ACTION_BODY) return json(res, 413, { error: "payload too large" });
          chunks.push(ch);
        }
        const payload = JSON.parse(Buffer.concat(chunks).toString("utf8") || "{}");
        if (!ALLOWED_ACTIONS.has(payload.action)) return json(res, 400, { error: "unsupported action" });
        const healthResp = await fetcher("http://127.0.0.1:" + controlPort + "/api/v1/health", { redirect: "error" });
        if (!healthResp.ok) return json(res, 503, { error: "control center unavailable" });
        const healthJson = await healthResp.json();
        const csrf = String(healthJson?.csrf_token || "");
        if (!csrf) return json(res, 503, { error: "csrf unavailable" });
        const fwd = await fetcher("http://127.0.0.1:" + controlPort + "/api/v1/runtime/workers/action", {
          method: "POST",
          redirect: "error",
          headers: {
            "Content-Type": "application/json",
            "Origin": "http://127.0.0.1:" + controlPort,
            "X-KITT-CSRF": csrf,
          },
          body: JSON.stringify(payload),
        });
        const out = await fwd.json().catch(() => ({ error: "invalid upstream json" }));
        return json(res, fwd.status, out);
      } catch {
        return json(res, 502, { error: "action proxy failed" });
      }
    }
    if (req.method !== "GET" && req.method !== "HEAD") return plain(res, 405, "Read only");
    const endpoint = API[route.pathname];
    if (endpoint) {
      if (req.method !== "GET") return plain(res, 405, "Read only");
      try {
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), 3000);
        let upstream;
        try {
          upstream = await fetcher("http://127.0.0.1:" + controlPort + route.pathname, {
            method: "GET", redirect: "error", signal: controller.signal,
            headers: { Accept: "application/json" },
          });
          if (!upstream.ok) return json(res, 503, { available: false });
          if (Number(upstream.headers.get("content-length") || 0) > MAX_API_BYTES)
            return json(res, 502, { available: false });
          let size = 0;
          const chunks = [];
          for await (const chunk of upstream.body) {
            size += chunk.byteLength;
            if (size > MAX_API_BYTES) return json(res, 502, { available: false });
            chunks.push(chunk);
          }
          const source = JSON.parse(Buffer.concat(chunks).toString("utf8"));
          return json(res, 200, safeProjection(endpoint, source));
        } finally { clearTimeout(timer); }
      } catch { return json(res, 503, { available: false }); }
    }
    // Never expose arbitrary Control Center endpoints via this bridge.
    if (route.pathname.startsWith("/api/")) return plain(res, 404, "Not found");
    const relative = route.pathname === "/" ? "index.html" : route.pathname.slice(1);
    if (!relative || relative.includes("..") || relative.includes("\\") || relative.includes("%"))
      return plain(res, 400, "Invalid asset path");
    const ext = path.extname(relative);
    if (!Object.hasOwn(MIME, ext)) return plain(res, 404, "Asset not allowed");
    const candidate = path.resolve(root, relative);
    if (!candidate.startsWith(root + path.sep)) return plain(res, 403, "Asset outside root");
    try {
      const realRoot = await fs.realpath(root);
      const real = await fs.realpath(candidate);
      if (!real.startsWith(realRoot + path.sep)) return plain(res, 403, "Asset symlink escape");
      const info = await fs.stat(real);
      if (!info.isFile() || info.size > MAX_ASSET_BYTES)
        return plain(res, 404, "Asset not available");
      res.writeHead(200, {
        "Content-Type": MIME[ext], "Cache-Control": "no-store",
        "Content-Length": info.size,
        "X-Content-Type-Options": "nosniff",
        "X-Frame-Options": "DENY",
        "Referrer-Policy": "no-referrer",
        "Cross-Origin-Resource-Policy": "same-origin",
        "Content-Security-Policy": "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; font-src 'self'; worker-src 'self'; base-uri 'none'; frame-ancestors 'none'",
      });
      if (req.method === "HEAD") {
        res.end();
      } else {
        createReadStream(real).pipe(res);
      }
    } catch { return plain(res, 404, "Not found"); }
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const port = Number(process.env.KITT_RUNE_WEB_PORT || 41931);
  const controlPort = Number(process.env.KITT_CONTROL_CENTER_PORT || 41828);
  const dist = process.env.KITT_RUNE_DIST || DEFAULT_DIST;
  const server = createRuneServer({ port, controlPort, dist });
  server.listen(port, "127.0.0.1", () =>
    console.log("KITT Rune web: http://127.0.0.1:" + port));
}
