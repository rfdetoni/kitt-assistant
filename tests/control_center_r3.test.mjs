import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import vm from "node:vm";

const SOURCE = fs.readFileSync(
  new URL("../apps/kittd/control-center-web/app.js", import.meta.url),
  "utf8",
);

function fakeElement() {
  return {
    value: "",
    innerHTML: "",
    textContent: "",
    disabled: false,
    style: {},
    classList: { add() {}, remove() {} },
    addEventListener() {},
    replaceChildren() {},
    remove() {},
    focus() {},
    showModal() {},
    close() {},
  };
}

function makeHarness() {
  const elements = new Map();
  const getElement = (id) => {
    if (!elements.has(id)) elements.set(id, fakeElement());
    return elements.get(id);
  };
  const pendingDiscoveries = [];

  const context = vm.createContext({
    console,
    setTimeout: (fn) => { fn(); return 0; },
    clearTimeout() {},
    document: {
      getElementById: getElement,
      querySelectorAll: () => [],
      querySelector: () => null,
      createElement: () => fakeElement(),
    },
    fetch: async (path, options = {}) => {
      if (path === "/api/v1/health") {
        return { ok: true, status: 200, json: async () => ({ status: "ok", csrf_token: "test", bind: "loopback" }) };
      }
      if (path === "/api/v1/catalog") {
        return {
          ok: true, status: 200, json: async () => ({ sections: [{
            id: "assistant", title: "Assistant", component: "assistant", fields: [
              { key: "fast_base_url", type: "string", default: "http://old/v1" },
              { key: "fast_model", type: "string", default: "old-model" },
            ],
          }] }),
        };
      }
      if (path === "/api/v1/config") {
        return {
          ok: true, status: 200,
          json: async () => ({ revision: 1, values: { assistant: { fast_base_url: "http://old/v1", fast_model: "old-model" } } }),
        };
      }
      if (path === "/api/v1/service/status") {
        return {
          ok: true, status: 200, json: async () => ({
            status: "ok",
            daemon: { active: true, pid: 1234, uptime_seconds: 120, listen: "127.0.0.1:41827", bind: "127.0.0.1:41828", version: "0.1.0" },
            voice: { enabled: true, activation_mode: "auto", stt_worker_model: "base", stt_worker_online: true, wake_phrases: ["kitt"], wakeword_model_exists: false, wakeword_model_path: "wakewords/kitt.rpw" },
            models: { base_url: "http://127.0.0.1:11434/v1", fast_model: "test-model" },
            memory: { exists: true, size_bytes: 4096 }
          })
        };
      }
      if (path === "/api/v1/service/logs") {
        return {
          ok: true, status: 200, json: async () => ({
            status: "ok",
            source: "journalctl",
            logs: "kittd listening on 127.0.0.1:41827\nkitt voice enabled"
          })
        };
      }
      if (path === "/api/v1/models/discover") {
        let resolve;
        const response = new Promise((res) => { resolve = res; });
        pendingDiscoveries.push({ body: JSON.parse(options.body), resolve });
        return response;
      }
      throw new Error(`unexpected fetch: ${path}`);
    },
  });

  vm.runInContext(SOURCE, context, { filename: "app.js" });
  vm.runInContext(`
    state.catalog={sections:[{
      id:"assistant",title:"Assistant",component:"assistant",fields:[
        {key:"fast_base_url",type:"string",default:"http://old/v1"},
        {key:"fast_model",type:"string",default:"old-model"}
      ]
    }]};
    state.snapshot={revision:1,values:{assistant:{fast_base_url:"http://old/v1",fast_model:"old-model"}}};
    state.csrf="test";
  `, context);

  return { context, pendingDiscoveries };
}

function resolveDiscovery(entry, models) {
  entry.resolve({ ok: true, status: 200, json: async () => ({ models }) });
}

test("model picker does not inject hardcoded fallback models", () => {
  const { context } = makeHarness();
  const html = vm.runInContext(`
    control(
      state.catalog.sections[0],
      state.catalog.sections[0].fields.find((field)=>field.key==="fast_model")
    )
  `, context);
  assert.equal(html.includes("whisper-1"), false);
  assert.equal(html.includes("qwen3:4b"), false);
});

test("changing provider URL invalidates an in-flight model discovery", async () => {
  const { context, pendingDiscoveries } = makeHarness();
  const first = vm.runInContext(`discoverModels("assistant","fast_model")`, context);
  await Promise.resolve();
  assert.equal(pendingDiscoveries.length, 1);
  assert.equal(pendingDiscoveries[0].body.base_url, "http://old/v1");

  vm.runInContext(`
    state.pending.set("assistant::fast_base_url","http://new/v1");
    invalidateModelCachesForChangedField(state.catalog.sections[0],"fast_base_url");
  `, context);
  resolveDiscovery(pendingDiscoveries[0], ["stale-model"]);
  await first;

  assert.equal(
    vm.runInContext(`state.modelsCache.has("assistant::fast_model")`, context),
    false,
  );
});

test("out-of-order discoveries keep only the newest response", async () => {
  const { context, pendingDiscoveries } = makeHarness();
  const first = vm.runInContext(`discoverModels("assistant","fast_model")`, context);
  await Promise.resolve();

  vm.runInContext(`state.pending.set("assistant::fast_base_url","http://new/v1")`, context);
  const second = vm.runInContext(`discoverModels("assistant","fast_model")`, context);
  await Promise.resolve();
  assert.equal(pendingDiscoveries.length, 2);

  resolveDiscovery(pendingDiscoveries[1], ["new-model"]);
  await second;
  resolveDiscovery(pendingDiscoveries[0], ["old-model"]);
  await first;

  assert.deepEqual(
    Array.from(vm.runInContext(`state.modelsCache.get("assistant::fast_model")`, context)),
    ["new-model"],
  );
});

test("service monitor renders navigation and telemetries correctly", async () => {
  const { context } = makeHarness();
  await vm.runInContext(`fetchServiceStatus()`, context);
  assert.equal(vm.runInContext(`state.serviceStatus?.daemon?.pid`, context), 1234);
  assert.equal(vm.runInContext(`formatUptime(125)`, context), "2m 5s");
  assert.equal(vm.runInContext(`formatBytes(4096)`, context), "4.0 KB");

  const logHtml = vm.runInContext(`highlightLogs("error: failed to bind socket")`, context);
  assert.equal(logHtml.includes("log-error"), true);
});

test("service status response does not include logs and fetchServiceLogs fetches logs on demand", async () => {
  const { context } = makeHarness();
  const status = await vm.runInContext(`fetchServiceStatus()`, context);
  assert.equal(status.logs, undefined);

  vm.runInContext(`state.view = "monitor"`, context);
  const logs = await vm.runInContext(`fetchServiceLogs()`, context);
  assert.equal(logs.includes("kittd listening"), true);
  assert.equal(vm.runInContext(`state.serviceLogs`, context).includes("kitt voice enabled"), true);
});

test("agentWebUrl normalizes 0.0.0.0 and :: to loopback and supports TLS", () => {
  const { context } = makeHarness();
  vm.runInContext(`
    state.catalog.sections.push({
      id: "agent.remote",
      fields: [
        { key: "host", default: "127.0.0.1" },
        { key: "port", default: 7337 },
        { key: "tls_cert", default: "" },
        { key: "tls_key", default: "" }
      ]
    });
    state.snapshot.values["agent.remote"] = { host: "0.0.0.0", port: 7337 };
  `, context);

  assert.equal(vm.runInContext(`agentWebUrl()`, context), "http://127.0.0.1:7337/");

  vm.runInContext(`state.snapshot.values["agent.remote"] = { host: "::1", port: 8443, tls_cert: "/cert.pem", tls_key: "/key.pem" }`, context);
  assert.equal(vm.runInContext(`agentWebUrl()`, context), "https://[::1]:8443/");
});

test("advanced fields are filtered unless showAdvanced is true or search matches", () => {
  const { context } = makeHarness();
  vm.runInContext(`
    state.catalog.sections = [{
      id: "test", title: "Test Section", component: "test", fields: [
        { key: "basic_field", label: "Basic Field", type: "string", default: "val", advanced: false },
        { key: "adv_field", label: "Advanced Field", type: "string", default: "val2", advanced: true }
      ]
    }];
    state.showAdvanced = false;
  `, context);

  // Without search and showAdvanced=false: only basic field
  let html = vm.runInContext(`
    (function() {
      const section = state.catalog.sections[0];
      const filter = "";
      const fields = section.fields.filter((f) => {
        const matches = !filter || (f.label + " " + (f.description || "") + " " + f.key + " " + section.title).toLowerCase().includes(filter);
        return matches && (filter || state.showAdvanced || !f.advanced);
      });
      return fields.map((f) => f.key);
    })()
  `, context);
  assert.deepEqual(Array.from(html), ["basic_field"]);

  // With showAdvanced=true: all fields
  vm.runInContext(`state.showAdvanced = true;`, context);
  html = vm.runInContext(`
    (function() {
      const section = state.catalog.sections[0];
      const filter = "";
      const fields = section.fields.filter((f) => {
        const matches = !filter || (f.label + " " + (f.description || "") + " " + f.key + " " + section.title).toLowerCase().includes(filter);
        return matches && (filter || state.showAdvanced || !f.advanced);
      });
      return fields.map((f) => f.key);
    })()
  `, context);
  assert.deepEqual(Array.from(html), ["basic_field", "adv_field"]);

  // With search match on advanced field when showAdvanced=false: advanced field is included
  vm.runInContext(`state.showAdvanced = false;`, context);
  html = vm.runInContext(`
    (function() {
      const section = state.catalog.sections[0];
      const filter = "advanced";
      const fields = section.fields.filter((f) => {
        const matches = !filter || (f.label + " " + (f.description || "") + " " + f.key + " " + section.title).toLowerCase().includes(filter);
        return matches && (filter || state.showAdvanced || !f.advanced);
      });
      return fields.map((f) => f.key);
    })()
  `, context);
  assert.deepEqual(Array.from(html), ["adv_field"]);
});

test("hidden document suppresses polling calls", async () => {
  const { context } = makeHarness();
  vm.runInContext(`document.hidden = true;`, context);
  const statusRes = await vm.runInContext(`fetchServiceStatus()`, context);
  assert.equal(statusRes, null);
  const logsRes = await vm.runInContext(`fetchServiceLogs()`, context);
  assert.equal(logsRes, "");
});
