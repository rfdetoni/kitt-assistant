"use strict";

const state={catalog:null,snapshot:null,pending:new Map(),section:null,csrf:null,modelsCache:new Map()};
const $=(id)=>document.getElementById(id);

async function api(path,options={}){
  const headers={"Accept":"application/json",...(options.body?{"Content-Type":"application/json"}:{}),...(options.headers||{})};
  if(options.method && !["GET","HEAD"].includes(options.method) && state.csrf) headers["X-KITT-CSRF"]=state.csrf;
  const response=await fetch(path,{...options,headers,credentials:"same-origin"});
  const payload=await response.json().catch(()=>({}));
  if(!response.ok) throw new Error(payload.error||`HTTP ${response.status}`);
  if(payload.csrf_token) state.csrf=payload.csrf_token;
  return payload;
}

function esc(value){return String(value??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;","\"":"&quot;","'":"&#39;"}[c]));}
function key(section,field){return `${section.id}::${field.key}`;}
function current(section,field){const k=key(section,field);if(state.pending.has(k))return state.pending.get(k);return state.snapshot?.values?.[section.id]?.[field.key]??field.default??null;}
function isChanged(section,field){return state.pending.has(key(section,field));}

function isModelField(field){
  return field.key==="model" || field.key.endsWith("_model") || field.key.includes("model");
}

function getSectionBaseUrl(sectionId){
  const s=state.catalog?.sections.find(sec=>sec.id===sectionId);
  if(!s) return "http://127.0.0.1:11434/v1";
  for(const f of s.fields){
    if(f.key.includes("base_url")||f.key.includes("ollama_url")||f.key.includes("url")){
      const val=current(s,f);
      if(val&&typeof val==="string"&&val.trim()) return val.trim();
    }
  }
  return "http://127.0.0.1:11434/v1";
}

function getSectionApiKeyEnv(sectionId){
  const s=state.catalog?.sections.find(sec=>sec.id===sectionId);
  if(!s) return null;
  for(const f of s.fields){
    if(f.key.includes("api_key_env")){
      const val=current(s,f);
      if(val&&typeof val==="string"&&val.trim()) return val.trim();
    }
  }
  return null;
}

function control(section,field){
  const k=key(section,field),value=current(section,field),attr=`data-key="${esc(k)}" data-section="${esc(section.id)}" data-field="${esc(field.key)}"`;
  if(field.type==="boolean") return `<label class="switch"><input type="checkbox" ${attr} ${value?"checked":""}><span></span></label>`;
  if(field.type==="enum") return `<select ${attr}>${(field.options||[]).map(o=>`<option value="${esc(o)}" ${String(value)===o?"selected":""}>${esc(o)}</option>`).join("")}</select>`;
  const inputType=field.type==="integer"||field.type==="number"?"number":"text";
  const min=field.minimum!==undefined?`min="${field.minimum}"`:"",max=field.maximum!==undefined?`max="${field.maximum}"`:"",step=field.type==="number"?'step="any"':"";
  const shown=field.type==="string_list"&&Array.isArray(value)?value.join(", "):value??"";

  if(isModelField(field)){
    const listId=`list-${esc(k.replace(/::/g,"_"))}`;
    const cachedModels=state.modelsCache.get(section.id)||[];
    const datalistOptions=cachedModels.map(m=>`<option value="${esc(m)}">${esc(m)}</option>`).join("");
    return `<input type="${inputType}" ${attr} list="${listId}" ${min} ${max} ${step} value="${esc(shown)}" placeholder="${esc(field.placeholder||"Selecione ou digite o modelo")}" autocomplete="off"><datalist id="${listId}">${datalistOptions}</datalist><button type="button" class="btn-discover" data-discover-section="${esc(section.id)}" data-discover-field="${esc(field.key)}" title="Listar modelos disponíveis na URL do provider">🔍 Listar Modelos</button>`;
  }

  return `<input type="${inputType}" ${attr} ${min} ${max} ${step} value="${esc(shown)}" placeholder="${esc(field.placeholder||"")}" autocomplete="off">`;
}

async function discoverModels(sectionId, fieldKey, btnEl=null){
  const baseUrl=getSectionBaseUrl(sectionId);
  const apiKeyEnv=getSectionApiKeyEnv(sectionId);
  if(btnEl){btnEl.classList.add("loading");btnEl.textContent="⏳ Buscando...";}
  try{
    const result=await api("/api/v1/models/discover",{
      method:"POST",
      body:JSON.stringify({base_url:baseUrl,api_key_env:apiKeyEnv})
    });
    const models=result.models||[];
    state.modelsCache.set(sectionId, models);
    const listId=`list-${sectionId}_${fieldKey}`;
    const datalist=$(listId);
    if(datalist){
      datalist.innerHTML=models.map(m=>`<option value="${esc(m)}">${esc(m)}</option>`).join("");
    }
    const inputEl=document.querySelector(`[data-section="${sectionId}"][data-field="${fieldKey}"]`);
    if(inputEl){
      inputEl.focus();
    }
    if(models.length){
      toast(`✨ ${models.length} modelos encontrados em ${baseUrl}! Escolha no campo.`);
    }else{
      toast(`Nenhum modelo retornado por ${baseUrl}. Verifique se o servidor está ativo.`, "bad");
    }
  }catch(e){
    toast(`Falha ao listar modelos de ${baseUrl}: ${e.message}`, "bad");
  }finally{
    if(btnEl){btnEl.classList.remove("loading");btnEl.textContent="🔍 Listar Modelos";}
  }
}

function render(){
  const sections=state.catalog?.sections||[];
  if(!state.section&&sections.length)state.section=sections[0].id;
  $("nav").innerHTML=sections.map(s=>`<button class="nav-item ${s.id===state.section?"active":""}" data-nav="${esc(s.id)}"><span>${esc(s.title)}</span><span class="count">${s.fields.length}</span></button>`).join("");
  const filter=$("search").value.trim().toLowerCase();
  const visible=sections.filter(s=>!filter||s.title.toLowerCase().includes(filter)||s.component.toLowerCase().includes(filter)||s.fields.some(f=>(f.label+" "+(f.description||"")+" "+f.key).toLowerCase().includes(filter))).filter(s=>filter||s.id===state.section);
  $("content").innerHTML=visible.map(section=>{
    const fields=section.fields.filter(f=>!filter||(f.label+" "+(f.description||"")+" "+f.key+" "+section.title).toLowerCase().includes(filter));
    const changed=fields.some(f=>isChanged(section,f));
    return `<article class="section-card"><header class="section-head"><div><h2>${esc(section.title)}</h2><p>${esc(section.description||section.component)}</p></div><span class="badge ${changed?"changed":""}">${changed?"modificado":esc(section.component)}</span></header><div class="fields">${fields.map(field=>`<div class="field ${field.advanced?"advanced":""}"><div class="field-label"><span>${esc(field.label)}</span>${field.apply_mode!=="live"?`<span class="restart">${field.apply_mode==="daemon_restart"?"REINICIA KITT":"RESTART"}</span>`:""}</div><div class="control">${control(section,field)}</div>${field.description?`<p>${esc(field.description)}</p>`:""}</div>`).join("")}</div></article>`;
  }).join("")||`<div class="alert">Nenhuma configuração encontrada.</div>`;
  $("apply-all").disabled=state.pending.size===0;$("reset-all").disabled=state.pending.size===0;
  $("revision").textContent=`rev ${state.snapshot?.revision??"–"}`;
  bindInputs();bindNav();
}

function bindInputs(){
  document.querySelectorAll("[data-key]").forEach(el=>el.addEventListener("change",()=>{
    const section=state.catalog.sections.find(s=>s.id===el.dataset.section),field=section.fields.find(f=>f.key===el.dataset.field);
    let value=el.type==="checkbox"?el.checked:el.value;
    if(field.type==="integer")value=Number.parseInt(value,10);
    if(field.type==="number")value=Number(value);
    if(field.type==="string_list")value=value.split(",").map(v=>v.trim()).filter(Boolean);
    state.pending.set(el.dataset.key,value);

    // If a base_url was changed, automatically discover models for model fields in this section
    if(field.key.includes("base_url")||field.key.includes("ollama_url")){
      const modelField=section.fields.find(f=>isModelField(f));
      if(modelField){
        discoverModels(section.id, modelField.key);
      }
    }
    render();
  }));

  document.querySelectorAll("[data-discover-section]").forEach(btn=>btn.addEventListener("click",()=>{
    const sec=btn.dataset.discoverSection,fld=btn.dataset.discoverField;
    discoverModels(sec, fld, btn);
  }));
}

function bindNav(){document.querySelectorAll("[data-nav]").forEach(el=>el.addEventListener("click",()=>{state.section=el.dataset.nav;$("search").value="";render();}));}
function toast(message,type="good"){const node=document.createElement("div");node.className=`alert ${type}`;node.textContent=message;$("alerts").replaceChildren(node);setTimeout(()=>node.remove(),4500);}
function changesObject(){const out={};for(const [compound,value] of state.pending){const split=compound.indexOf("::");const section=compound.slice(0,split),field=compound.slice(split+2);(out[section]??={})[field]=value;}return out;}

async function preview(){try{const result=await api("/api/v1/validate",{method:"POST",body:JSON.stringify({expected_revision:state.snapshot.revision,changes:changesObject()})});$("diff").textContent=JSON.stringify(result.diff||changesObject(),null,2);$("diff-dialog").showModal();}catch(e){toast(e.message,"bad");}}
async function apply(){try{const result=await api("/api/v1/config",{method:"PUT",body:JSON.stringify({expected_revision:state.snapshot.revision,changes:changesObject()})});state.pending.clear();state.snapshot=result.snapshot||await api("/api/v1/config");render();toast(result.restart_required?.length?`Aplicado. Reinício necessário: ${result.restart_required.join(", ")}`:"Configuração aplicada.");}catch(e){toast(e.message,"bad");}}

async function boot(){
  try{
    const [health,catalog,snapshot]=await Promise.all([api("/api/v1/health"),api("/api/v1/catalog"),api("/api/v1/config")]);
    state.catalog=catalog;state.snapshot=snapshot;state.csrf=health.csrf_token||state.csrf;
    $("daemon-status").textContent=health.status==="ok"?"kittd online":"kittd degradado";
    $("overview").innerHTML=[
      ["Daemon",health.status||"unknown","ok"],
      ["Componentes",String(new Set(catalog.sections.map(s=>s.component)).size),""],
      ["Seções",String(catalog.sections.length),""],
      ["Modo",health.bind||"loopback","ok"]
    ].map(([l,v,c])=>`<div class="metric"><small>${esc(l)}</small><strong class="${c}">${esc(v)}</strong></div>`).join("");
    render();
  }catch(e){$("daemon-dot").style.background="var(--bad)";toast(`Falha ao carregar Control Center: ${e.message}`,"bad");}
}

$("search").addEventListener("input",render);$("reset-all").addEventListener("click",()=>{state.pending.clear();render();});$("apply-all").addEventListener("click",preview);$("confirm-apply").addEventListener("click",e=>{e.preventDefault();$("diff-dialog").close();apply();});boot();

