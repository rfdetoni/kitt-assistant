import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import type { HudEvent } from "@kitt/protocol";
import "./style.css";

const content = document.querySelector<HTMLElement>("#content")!;
let timer: number | undefined;

function clearTimer() {
  if (timer !== undefined) {
    window.clearTimeout(timer);
    timer = undefined;
  }
}

function arm(ttl: number) {
  clearTimer();
  timer = window.setTimeout(() => invoke("exit_hud"), Math.max(500, ttl));
}

function render(event: HudEvent) {
  // A new event supersedes any previous TTL. Without this, a listening/text TTL
  // can terminate the HUD while a later thinking/responding state is still active.
  clearTimer();

  if (event.type === "hide") {
    void invoke("exit_hud");
    return;
  }
  if (event.type === "status") {
    content.innerHTML = `<div class="status">${escapeHtml(event.message ?? event.state)}</div>`;
    return;
  }
  if (event.type === "text") {
    content.innerHTML = `<div class="text">${escapeHtml(event.content).replaceAll("\n", "<br>")}</div>`;
    arm(event.ttl_ms);
    return;
  }
  if (event.type === "image") {
    const img = document.createElement("img");
    img.src = event.src;
    img.alt = event.alt ?? "KITT image";
    content.replaceChildren(img);
    arm(event.ttl_ms);
  }
}

function escapeHtml(value: string) {
  const element = document.createElement("div");
  element.textContent = value;
  return element.innerHTML;
}

void listen<HudEvent>("hud-event", (event) => render(event.payload));
