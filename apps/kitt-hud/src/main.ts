import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import type { HudEvent } from "@kitt/protocol";
import "./style.css";
const content=document.querySelector<HTMLElement>("#content")!; let timer:number|undefined;
function arm(ttl:number){if(timer)window.clearTimeout(timer);timer=window.setTimeout(()=>invoke("exit_hud"),Math.max(500,ttl));}
function render(e:HudEvent){
 if(e.type==="hide"){void invoke("exit_hud");return}
 if(e.type==="status"){content.innerHTML=`<div class="status">${escapeHtml(e.message ?? e.state)}</div>`;return}
 if(e.type==="text"){content.innerHTML=`<div class="text">${escapeHtml(e.content).replaceAll("\n","<br>")}</div>`;arm(e.ttl_ms);return}
 if(e.type==="image"){const img=document.createElement("img");img.src=e.src;img.alt=e.alt??"KITT image";content.replaceChildren(img);arm(e.ttl_ms)}
}
function escapeHtml(v:string){const d=document.createElement("div");d.textContent=v;return d.innerHTML}
listen<HudEvent>("hud-event",event=>render(event.payload));
