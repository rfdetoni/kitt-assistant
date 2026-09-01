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
  `, context);

  return { context, pendingDiscoveries };
}

function resolveDiscovery(entry, models) {
  entry.resolve({ ok: true, status: 200, json: async () => ({ models }) });
}

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
