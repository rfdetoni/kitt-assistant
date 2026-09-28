const FALLBACK_COMPONENTS = new Set([
  "Text", "Markdown", "Heading", "Badge", "Status", "Metric", "Progress",
  "List", "Table", "Tree", "Code", "Diff", "Log", "Alert", "Tabs",
  "Input", "Select", "Toggle", "Button", "ButtonGroup", "Artifact", "Timeline",
]);

function node(tag, className = "", text) {
  const el = document.createElement(tag);
  if (className) el.className = className;
  if (text !== undefined) el.textContent = String(text);
  return el;
}

function boundedText(value, max = 12000) {
  let text;
  if (typeof value === "string") text = value;
  else {
    try { text = JSON.stringify(value ?? "", null, 2); }
    catch (_) { text = String(value ?? ""); }
  }
  return String(text || "").slice(0, max);
}

export class SurfaceRenderer {
  constructor({container, request, notify, getSessionId}) {
    this.container = container;
    this.request = request;
    this.notify = notify;
    this.getSessionId = getSessionId;
    this.capabilities = new Set();
    this.revisions = new Map();
  }

  setCapabilities(components) {
    this.capabilities = new Set((Array.isArray(components) ? components : []).map(String));
  }

  reset() {
    this.revisions.clear();
  }

  componentAllowed(kind) {
    return this.capabilities.size
      ? this.capabilities.has(kind)
      : FALLBACK_COMPONENTS.has(kind);
  }

  async invoke(surface, component, button) {
    const action = String(component?.props?.action || "").trim();
    const sessionId = String(this.getSessionId() || "").trim();
    if (!action || !sessionId) return;
    button.disabled = true;
    try {
      await this.request(
        "/api/surfaces/" + encodeURIComponent(surface.id) + "/actions/" + encodeURIComponent(action),
        {
          method: "POST",
          body: JSON.stringify({
            session_id: sessionId,
            component_id: String(component.id || ""),
            context: {surface_revision: Number(surface.revision || 0)},
          }),
        },
      );
      this.notify("Ação " + action + " registrada", "good");
    } catch (err) {
      this.notify(err?.message || "Falha na ação semântica", "bad");
    } finally {
      button.disabled = false;
    }
  }

  renderComponent(surface, component, byId, depth, visited) {
    if (!component || depth > 16) return null;
    const componentId = String(component.id || "");
    if (!componentId || visited.has(componentId)) return null;
    visited.add(componentId);

    const kind = String(component.component || "Text");
    if (!this.componentAllowed(kind)) return null;
    const props = component.props && typeof component.props === "object" ? component.props : {};
    const wrap = node("div", "surface-node surface-" + kind.toLowerCase());
    wrap.dataset.componentId = componentId;

    if (kind === "Heading") {
      wrap.append(node("h4", "surface-heading", boundedText(props.text, 1200)));
    } else if (kind === "Text" || kind === "Markdown") {
      wrap.append(node("div", "surface-text", boundedText(props.text ?? props.content)));
    } else if (kind === "Badge" || kind === "Status") {
      wrap.append(node("span", "surface-badge", boundedText(props.text, 500)));
    } else if (kind === "Metric") {
      const metric = node("div", "surface-metric");
      metric.append(
        node("span", "surface-metric-label", boundedText(props.label, 500)),
        node("strong", "surface-metric-value", boundedText(props.value, 1000)),
      );
      wrap.append(metric);
    } else if (kind === "Progress") {
      const progress = node("progress", "surface-progress");
      progress.max = Math.max(1, Number(props.max || 100));
      progress.value = Math.max(0, Math.min(progress.max, Number(props.value || 0)));
      wrap.append(progress, node("span", "muted", String(progress.value) + "/" + String(progress.max)));
    } else if (kind === "Button") {
      const button = node("button", "surface-button", boundedText(props.label, 300));
      button.type = "button";
      button.addEventListener("click", () => this.invoke(surface, component, button));
      wrap.append(button);
    } else if (kind === "Input") {
      const input = node("input", "surface-input");
      input.name = boundedText(props.name, 128);
      input.placeholder = boundedText(props.placeholder, 500);
      input.value = boundedText(props.value, 2000);
      wrap.append(input);
    } else if (kind === "Select") {
      const select = node("select", "surface-input");
      select.name = boundedText(props.name, 128);
      const values = Array.isArray(props.options) ? props.options.slice(0, 100) : [];
      for (const item of values) {
        const value = typeof item === "object" && item
          ? String(item.value ?? item.label ?? "")
          : String(item);
        const label = typeof item === "object" && item
          ? String(item.label ?? item.value ?? "")
          : String(item);
        const option = node("option", "", label.slice(0, 500));
        option.value = value.slice(0, 500);
        select.append(option);
      }
      wrap.append(select);
    } else if (kind === "Toggle") {
      const label = node("label", "surface-toggle");
      const input = node("input");
      input.type = "checkbox";
      input.name = boundedText(props.name, 128);
      input.checked = Boolean(props.value);
      label.append(input, document.createTextNode(" " + boundedText(props.label ?? props.name, 500)));
      wrap.append(label);
    } else if (kind === "Alert") {
      wrap.append(node("div", "surface-alert", boundedText(props.text)));
    } else if (kind === "Code" || kind === "Diff" || kind === "Log") {
      wrap.append(node("pre", "surface-code", boundedText(props.code ?? props.text ?? props.content, 64000)));
    } else if (kind === "Artifact") {
      wrap.append(node("div", "surface-artifact", "artifact:" + boundedText(props.artifact_id, 180)));
    } else if (kind === "List") {
      const list = node("ul", "surface-list");
      for (const item of (Array.isArray(props.items) ? props.items.slice(0, 200) : [])) {
        list.append(node("li", "", boundedText(item, 2000)));
      }
      wrap.append(list);
    } else if (kind === "Table" || kind === "Tree" || kind === "Timeline") {
      wrap.append(node("pre", "surface-code", boundedText(props.rows ?? props.items ?? props.data ?? props)));
    } else if (kind !== "Tabs" && kind !== "ButtonGroup") {
      wrap.append(node("div", "surface-text", boundedText(props.text ?? props.label ?? kind)));
    }

    const children = Array.isArray(component.children) ? component.children.slice(0, 128) : [];
    for (const childId of children) {
      const child = byId.get(String(childId));
      const rendered = this.renderComponent(surface, child, byId, depth + 1, visited);
      if (rendered) wrap.append(rendered);
    }
    return wrap;
  }

  render(surface, {prepend = false} = {}) {
    if (!surface || typeof surface !== "object") return false;
    const id = String(surface.id || "").trim();
    const revision = Math.max(0, Number(surface.revision || 0));
    if (!id || id.length > 128 || !Array.isArray(surface.components) || surface.components.length > 256) {
      return false;
    }
    const previous = Number(this.revisions.get(id) || 0);
    if (revision && previous >= revision) return false;

    const byId = new Map();
    for (const item of surface.components) {
      if (!item || typeof item !== "object") continue;
      const componentId = String(item.id || "");
      if (componentId && componentId.length <= 128) byId.set(componentId, item);
    }
    const root = byId.get(String(surface.root || ""));
    if (!root) return false;
    const rendered = this.renderComponent(surface, root, byId, 1, new Set());
    if (!rendered) return false;

    const current = [...this.container.querySelectorAll("[data-surface-id]")]
      .find((item) => item.dataset.surfaceId === id);
    current?.remove();

    const card = node("article", "message assistant surface-card");
    card.dataset.surfaceId = id;
    card.dataset.surfaceRevision = String(revision || 1);
    const head = node("div", "message-head");
    head.append(
      node("span", "message-role", "K.I.T.T. SURFACE"),
      node("span", "message-time", "rev " + String(revision || 1)),
    );
    const body = node("div", "surface-body");
    body.append(rendered);
    card.append(head, body);
    if (prepend) this.container.prepend(card);
    else this.container.append(card);
    this.revisions.set(id, revision || 1);
    return true;
  }
}
