import { test } from "node:test";
import assert from "node:assert/strict";
import { createServer, request } from "node:http";
import { mkdtemp, writeFile, symlink, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { createRuneServer } from "./serve.mjs";

async function listen(server) {
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  return server.address().port;
}
async function close(server) {
  await new Promise((resolve) => server.close(resolve));
}

test("Rune WebGPU bridge is read only and strips server secrets", async () => {
  const folder = await mkdtemp(path.join(tmpdir(), "kitt-rune-test-"));
  const control = createServer((req, res) => {
    res.setHeader("content-type", "application/json");
    if (req.url === "/api/v1/health") return res.end('{"status":"ok","csrf_token":"PRIVATE"}');
    if (req.url === "/api/v1/service/status")
      return res.end('{"daemon":{"active":true,"version":"v1"},"api_key_env":"PRIVATE"}');
    res.end('{"api_online":true,"phase":"ready","token":"PRIVATE"}');
  });
  const controlPort = await listen(control);
  const server = createRuneServer({ controlPort, dist: folder });
  const port = await listen(server);
  const base = "http://127.0.0.1:" + port;
  try {
    await writeFile(path.join(folder, "index.html"), "<h1>Rune</h1>");
    const page = await fetch(base);
    assert.equal(page.status, 200);
    assert.match(await page.text(), /Rune/);
    assert.match(page.headers.get("content-security-policy"), /wasm-unsafe-eval/);
    const health = await fetch(base + "/api/v1/health");
    assert.deepEqual(await health.json(), {status:"ok"});
    const service = await (await fetch(base + "/api/v1/service/status")).json();
    assert.equal(service.daemon.active, true);
    assert.equal(JSON.stringify(service).includes("PRIVATE"), false);
    const mut = await fetch(base + "/api/v1/config", {method:"POST"});
    assert.equal(mut.status, 405);
    assert.equal((await fetch(base + "/api/v1/config")).status, 404);
    const invalidHostCode = await new Promise((resolve, reject) => {
      const req = request(base, {headers: {Host:"evil.example"}}, res => {
        res.resume();
        resolve(res.statusCode);
      });
      req.on("error", reject);
      req.end();
    });
    assert.equal(invalidHostCode, 400);
    await symlink(path.join(folder, "..", "escaped.js"), path.join(folder, "leak.js"));
    assert.notEqual((await fetch(base + "/leak.js")).status, 200);

    // Runtime fleet & operations projections + guarded action dispatch
    const workersResp = await fetch(base + "/api/v1/runtime/workers");
    assert.equal(workersResp.status, 200);
    assert.equal(JSON.stringify(await workersResp.json()).includes("PRIVATE"), false);
    const badAction = await fetch(base + "/api/v1/runtime/workers/action", {
      method: "POST",
      headers: { Origin: base, "Content-Type": "application/json" },
      body: JSON.stringify({ worker_id: "w1", action: "pause" }),
    });
    assert.equal(badAction.status, 403);
    const okAction = await fetch(base + "/api/v1/runtime/workers/action", {
      method: "POST",
      headers: { Origin: base, "X-Kitt-Rune-Action": "1", "Content-Type": "application/json" },
      body: JSON.stringify({ worker_id: "w1", action: "pause" }),
    });
    assert.equal(okAction.status, 200);
  } finally {
    await close(server); await close(control); await rm(folder, {recursive:true,force:true});
  }
});
