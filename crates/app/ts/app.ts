// UI do screenManual (adendo 2026-10-05): TS puro, sem framework; o Tauri vem em window.__TAURI__.
import type { Status, Modelo, Desempenho, Resumo, Detalhe, Inicio, PassoUi, ImagemUi, Colecao, DocNode, Config, ApiError, Perguntas, AgentResult, Evento } from "./tipos.js";
import { icone, type NomeIcone } from "./icones.js";
import { esc } from "./texto.js";
import { montarDestino } from "./destino.js";
import { htmlEscolha, ligarEscolhas, sincronizarEscolhas, type OpcaoEscolha } from "./escolha.js";
import { ampliar, bloquearSoltarFora, ligarSoltar, ligarZoom } from "./galeria.js";

const invoke = __TAURI__.core.invoke;

const STATUS: Record<Status, string> = {
  recording: "gravando", interrupted: "interrompida", stopped: "parada", processing: "processando…",
  ready: "pronta", generating: "gerando…", awaiting: "aguardando respostas", draft: "rascunho", published: "publicado", error: "erro",
};
const MODELOS: [Modelo, string][] = [
  ["minimo", "Mínimo"],
  ["rapido", "Rápido"],
  ["equilibrado", "Equilibrado"],
  ["preciso", "Preciso (padrão)"],
];
const TAMANHO: Record<Modelo, string> = { minimo: "~75 MB", rapido: "~490 MB", equilibrado: "~540 MB", preciso: "~575 MB" };
const DESEMPENHOS: OpcaoEscolha[] = [
  { valor: "baixo", rotulo: "Baixo", detalhe: "2 núcleos" },
  { valor: "medio", rotulo: "Médio (padrão)", detalhe: "metade dos núcleos" },
  { valor: "alto", rotulo: "Alto", detalhe: "todos os núcleos" },
];

const st = {
  inicio: null as Inicio | null,
  sessoes: [] as Resumo[],
  sel: null as string | null,
  det: null as Detalhe | null,
  tela: "sessao" as "sessao" | "config",
  progresso: {} as Record<string, string>,
  download: {} as Partial<Record<Modelo, string>>,
  colecoes: [] as Colecao[],
  destGerar: null as { colecao: string; pai: string | null } | null,
  destConfig: null as { colecao: string; pai: string | null } | null,
  config: null as Config | null,
  aviso: "",
  login: false,
  /** "Tentar de novo": a última ação disparada para cada sessão (A9). */
  ultima: {} as Record<string, () => Promise<void>>,
  passos: [] as PassoUi[],
  alterado: {} as Record<string, boolean>,
  urls: {} as Record<string, string>,
  passoImg: 0,
  menu: null as string | null,
  menuExcluir: false,
  menuNovo: false,
  /** Sessões cuja transcrição mudou depois do manual (retentar o áudio). */
  avisoTranscricao: {} as Record<string, boolean>,
};

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector(sel) as T;

/** Troca cada `<span data-icone="x">` estático do index.html pelo SVG. */
function iconesEstaticos(): void {
  document.querySelectorAll<HTMLElement>("[data-icone]").forEach((e) => {
    e.outerHTML = icone(e.dataset.icone as NomeIcone);
  });
}
iconesEstaticos();

const ICONE_STATUS: Record<Status, NomeIcone> = {
  recording: "circle-dot", interrupted: "file-video", stopped: "file-video", processing: "loader",
  ready: "file-video", generating: "loader", awaiting: "message-circle-question", draft: "file-pen-line",
  published: "book-check", error: "circle-alert",
};

function agoraLocal(): string {
  const d = new Date();
  const off = -d.getTimezoneOffset();
  const p = (n: number) => String(Math.trunc(Math.abs(n))).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}` +
    `${off >= 0 ? "+" : "-"}${p(off / 60)}:${p(off % 60)}`;
}

// ---------- carga ----------

async function carregar(): Promise<void> {
  st.inicio = await invoke<Inicio>("inicio");
  st.sessoes = await invoke<Resumo[]>("sessoes");
  st.det = st.sel ? await invoke<Detalhe>("detalhe", { id: st.sel }).catch(() => null) : null;
  if (st.sel && !st.det) st.sel = null; // a sessão selecionada foi excluída
  st.passos = st.det && (st.det.status === "draft" || st.det.status === "published")
    ? await invoke<PassoUi[]>("passos", { id: st.det.id }).catch(() => []) : [];
  if (!st.inicio.configurado && st.tela !== "config") await abrirConfig();
  render();
}

/** Lista as coleções; se falhar (ex.: token revogado), mostra o erro e devolve []. */
async function carregarColecoes(): Promise<Colecao[]> {
  try {
    return await invoke<Colecao[]>("colecoes");
  } catch (e) {
    st.aviso = (e as ApiError).mensagem ?? String(e);
    return [];
  }
}

async function abrirConfig(): Promise<void> {
  st.tela = "config";
  st.destConfig = null;
  st.config = await invoke<Config>("config");
  if (st.inicio?.configurado && st.colecoes.length === 0) {
    st.colecoes = await carregarColecoes();
  }
}

async function abrirDuvidas(id: string): Promise<void> {
  const p = await invoke<Perguntas | null>("perguntas", { id });
  if (!p) return;
  $("#lista-duvidas").innerHTML = p.perguntas.map((q) => `<fieldset data-q="${esc(q.id)}"><legend>${esc(q.pergunta)}</legend>
    ${q.opcoes.map((o) => `<label class="radio"><input type="${q.multipla ? "checkbox" : "radio"}" name="${esc(q.id)}" value="${esc(o)}"> ${esc(o)}</label>`).join("")}
    <label>Outro <input name="${esc(q.id)}__outro" placeholder="escreva outra resposta"></label></fieldset>`).join("");
  const dlg = $<HTMLDialogElement>("#duvidas");
  dlg.dataset.id = id;
  if (!dlg.open) dlg.showModal();
}

function lerRespostas(f: HTMLFormElement): { respostas: { id: string; escolhas: string[]; outro: string | null }[] } {
  const v = new FormData(f);
  return {
    respostas: [...f.querySelectorAll<HTMLElement>("fieldset[data-q]")].map((fs) => {
      const q = fs.dataset.q!;
      const outro = String(v.get(`${q}__outro`) ?? "").trim();
      return { id: q, escolhas: v.getAll(q).map(String), outro: outro || null };
    }),
  };
}

async function selecionar(id: string): Promise<void> {
  st.sel = id;
  st.destGerar = null;
  st.tela = "sessao";
  st.det = await invoke<Detalhe>("detalhe", { id });
  st.passos = st.det && (st.det.status === "draft" || st.det.status === "published")
    ? await invoke<PassoUi[]>("passos", { id: st.det.id }).catch(() => []) : [];
  if (st.inicio?.configurado && st.colecoes.length === 0) {
    st.colecoes = await carregarColecoes();
  }
  render();
  if (st.det?.status === "awaiting") abrirDuvidas(id);
}

function baixar(m: Modelo): void {
  const pronto = st.inicio?.modelos.find((x) => x.modelo === m)?.baixado;
  if (pronto || st.download[m]) return;
  st.download[m] = "0%";
  invoke("baixar_modelo", { modelo: m }).catch((e: ApiError) => { st.aviso = e.mensagem; render(); });
}

// ---------- ações ----------

async function tentar(f: () => Promise<unknown>): Promise<void> {
  try {
    await f();
  } catch (e) {
    const err = e as ApiError;
    st.login = err.kind === "login";
    st.aviso = err.mensagem ?? String(e);
  }
  await carregar();
}

/** Roda uma ação da sessão; no D9 pergunta e repete com sobrescrever=true. */
async function executar(id: string, acao: (sobrescrever: boolean) => Promise<unknown>, sobrescrever = false): Promise<void> {
  st.ultima[id] = () => executar(id, acao);
  st.aviso = "";
  st.login = false;
  try {
    const r = (await acao(sobrescrever)) as AgentResult | null;
    if (!sobrescrever && r?.validacao?.some((v) => v.tipo === "editado_manualmente") &&
      (await perguntar("O documento foi editado no Outline depois da última publicação. Sobrescrever com a versão do app?"))) {
      return executar(id, acao, true);
    }
  } catch (e) {
    const err = e as ApiError;
    if (err.kind === "editado_manualmente" && !sobrescrever &&
      (await perguntar(`O documento foi editado no Outline (revisão ${err.remote}; a última publicada pelo app é a ${err.local}). Sobrescrever?`))) {
      return executar(id, acao, true);
    }
    st.login = err.kind === "login";
    st.aviso = err.mensagem ?? String(e);
  }
  await carregar();
}

/** Desabilita os botões do form enquanto a chamada está em andamento (evita envio duplo). */
async function travar(f: HTMLFormElement, fn: () => Promise<void>): Promise<void> {
  const bs = [...f.querySelectorAll("button")];
  bs.forEach((b) => (b.disabled = true));
  try {
    await fn();
  } finally {
    bs.forEach((b) => (b.disabled = false));
  }
}

/** Esvazia o form (no DOM atual) para o trocar() não restaurar o texto enviado. */
function limparForm(nome: string): void {
  document.querySelector<HTMLFormElement>(`form[data-form="${nome}"]`)?.reset();
}

function perguntar(texto: string): Promise<boolean> {
  const dlg = $<HTMLDialogElement>("#pergunta");
  dlg.querySelector("p")!.textContent = texto;
  dlg.showModal();
  return new Promise((ok) => {
    dlg.onclick = (e) => {
      const v = (e.target as HTMLElement).closest("button")?.value;
      if (!v) return;
      dlg.close();
      ok(v === "sim");
    };
    dlg.oncancel = () => ok(false);
  });
}

function abrirTitulo(): void {
  if (st.inicio?.gravacao !== "parado") return;
  $<HTMLDialogElement>("#titulo").showModal();
}

// ---------- render ----------

function render(): void {
  renderTopo();
  renderLista();
  renderPainel();
}

function renderTopo(): void {
  const i = st.inicio;
  if (!i) return;
  $("#acoes-gravacao").innerHTML = i.gravacao === "parado"
    ? `<button data-acao="nova" class="primario">● Gravar</button>`
    : `<button data-acao="pausar">${icone("pause")} ${i.gravacao === "pausado" ? "Retomar" : "Pausar"}</button>` +
      `<button data-acao="parar" class="primario">${icone("square")} Parar</button>`;
  const faixa: string[] = [];
  if (i.claude_erro) faixa.push(`<span>Claude Code não instalado: ${esc(i.claude_erro)}</span>`);
  if (st.login) faixa.push(`<span>O Claude Code não está logado.</span><button data-acao="login">Fazer login</button>`);
  if (st.aviso) faixa.push(`<span>${esc(st.aviso)}</span><button data-acao="fechar-aviso" class="icone" title="Fechar">${icone("x")}</button>`);
  $("#faixa").innerHTML = faixa.map((f) => `<p>${icone("circle-alert")}${f}</p>`).join("");
}

function renderLista(): void {
  $("#lista").innerHTML = st.sessoes.length === 0
    ? `<li class="vazio">Nenhuma gravação ainda. Clique em ● Gravar.</li>`
    : st.sessoes.map((s) =>
      `<li data-id="${esc(s.id)}" class="${s.id === st.sel ? "sel" : ""}">
        <span class="tile s-${s.status}">${icone(ICONE_STATUS[s.status])}</span>
        <span class="meta-item"><strong>${esc(s.title)}</strong><span class="t-${s.status}">${STATUS[s.status]}</span></span>
        <button class="icone pontos ${st.menu === s.id ? "on" : ""}" data-acao="menu" data-id="${esc(s.id)}" title="Ações">${icone("more-horizontal")}</button>
        ${st.menu === s.id ? htmlMenu(s) : ""}
      </li>`).join("");
  st.menuNovo = false;
}

const OCUPADA: Status[] = ["recording", "processing", "generating"];

function htmlMenu(s: Resumo): string {
  const ocupada = OCUPADA.includes(s.status) || st.inicio?.gerando === s.id;
  const dis = ocupada ? `disabled title="aguarde terminar"` : "";
  const id = esc(s.id);
  const sub = st.menuExcluir ? "aberto" : "";
  return `<div class="popover menu aberto ${st.menuNovo ? "novo" : ""}" data-menu>
    <button class="mi" data-acao="refazer-tudo" data-id="${id}" ${dis}>${icone("refresh-cw")} Refazer tudo</button>
    <button class="mi" data-acao="retentar-audio" data-id="${id}" ${dis}>${icone("audio-lines")} Retentar o áudio</button>
    <div class="sep"></div>
    <button class="mi perigo ${st.menuExcluir ? "aberta" : ""}" data-acao="excluir-abrir" data-id="${id}" ${dis}>${icone("trash-2")} Excluir <span class="fim">${icone("chevron-down")}</span></button>
    <div class="sub ${sub}">
      <button class="mi perigo" data-acao="excluir-local" data-id="${id}" ${dis}>Só a gravação</button>
      ${s.url ? `<button class="mi perigo" data-acao="excluir-outline" data-id="${id}" ${dis}>Gravação + documento no Outline</button>` : ""}
    </div>
  </div>`;
}

/** Troca o HTML do painel preservando o que o operador digitou, se for a mesma tela/sessão. */
function trocar(p: HTMLElement, html: string): void {
  const chave = `${st.tela}:${st.sel}`;
  const campos = (raiz: HTMLElement) =>
    [...raiz.querySelectorAll<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>("[name]")];
  const chaveCampo = (e: HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement) =>
    `${e.closest("form")?.dataset.form}:${e.name}` + (e.type === "radio" ? `:${e.value}` : "");
  const marcavel = (e: HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement) =>
    e.type === "radio" || e.type === "checkbox";
  const antes = new Map<string, string | boolean>();
  if (p.dataset.chave === chave) {
    for (const e of campos(p)) {
      antes.set(chaveCampo(e), marcavel(e) ? (e as HTMLInputElement).checked : e.value);
    }
  }
  p.innerHTML = html;
  p.dataset.chave = chave;
  for (const e of campos(p)) {
    const v = antes.get(chaveCampo(e));
    if (v === undefined) continue;
    if (marcavel(e)) (e as HTMLInputElement).checked = v as boolean;
    else e.value = v as string;
  }
}

const arvore = (colecao: string) => invoke<DocNode[]>("documentos", { colecao });

function valorGerar(): { colecao: string | null; pai: string | null } {
  return st.destGerar ?? {
    colecao: st.det?.colecao ?? st.config?.colecao_padrao ?? null,
    pai: st.det?.colecao ? (st.det.pai ?? null) : (st.config?.documento_padrao ?? null),
  };
}

function montarDestinos(): void {
  const g = document.getElementById("destino-gerar");
  if (g && st.det) {
    montarDestino(g, { colecoes: st.colecoes, arvore, valor: valorGerar(), aoMudar: (x) => (st.destGerar = x) });
  }
  const c = document.getElementById("destino-config");
  if (c && st.config) {
    const v = st.destConfig ?? { colecao: st.config.colecao_padrao, pai: st.config.documento_padrao };
    montarDestino(c, { colecoes: st.colecoes, arvore, valor: v, aoMudar: (x) => (st.destConfig = x) });
  }
}

function renderPainel(): void {
  const p = $("#painel");
  if (st.tela === "config") {
    trocar(p, `<div class="cartao config">${htmlConfig()}</div>`);
    sincronizarEscolhas(p);
    return montarDestinos();
  }
  const d = st.det;
  if (!d) return trocar(p, `<p class="vazio">Selecione uma gravação ou clique em ● Gravar.</p>`);
  const dur = d.duration_ms ? ` · ${Math.max(1, Math.round(d.duration_ms / 60000))} min` : "";
  const cand = d.candidatos !== null ? ` · ${d.candidatos} passos candidatos` : "";
  const link = d.url ? `<button data-acao="link" data-url="${esc(d.url)}">${icone("external-link")} Abrir no Outline</button>` : "";
  const avisos = d.validacao.map((v) => `<p class="aviso">${icone("circle-alert")} ${esc(v.tipo)}: ${esc(v.detalhe)}</p>`).join("");
  const transcricao = st.avisoTranscricao[d.id]
    ? `<p class="aviso">${icone("circle-alert")} A transcrição mudou. Use Refazer tudo ou peça uma melhoria para que o manual use a nova fala.</p>` : "";
  const cab = `<div class="cartao">
    <div class="cabecalho">
      <div class="titulo"><span class="tile grande s-${d.status}">${icone(ICONE_STATUS[d.status])}</span>
        <div><h2>${esc(d.title)}</h2><p class="meta">${STATUS[d.status]}${dur}${cand}</p></div></div>
      ${link}
    </div>${avisos}${transcricao}
    <div class="linha-acoes">${htmlAcoes(d)}</div>
  </div>`;
  trocar(p, cab + htmlPassos(d));
  montarDestinos();
  carregarMiniaturas(p, d.id);
}

function htmlAcoes(d: Detalhe): string {
  const prog = `<p id="progresso" class="progresso">${esc(st.progresso[d.id] ?? "")}</p>`;
  const semClaude = !!st.inicio?.claude_erro;
  const outra = !!st.inicio?.gerando && st.inicio.gerando !== d.id;
  const travado = semClaude || outra ? "disabled" : "";
  const tituloOutra = st.sessoes.find((x) => x.id === st.inicio?.gerando)?.title ?? "outra sessão";
  const dica = outra ? `<p class="dica">Aguarde a geração de ${esc(tituloOutra)} terminar.</p>`
    : semClaude ? `<p class="dica">Instale o Claude Code para gerar manuais.</p>` : "";
  switch (d.status) {
    case "recording": return `<p>Gravando… use Pausar ou Parar na barra lateral ou na bandeja. Ctrl+Alt+P pausa ou retoma, Ctrl+Alt+M marca um passo.</p>`;
    case "processing": return prog;
    case "awaiting": return `<p>A IA tem dúvidas antes de escrever o manual.</p><button data-acao="responder" class="primario">Responder…</button>`;
    case "generating": return prog + `<button data-acao="cancelar">Cancelar</button>`;
    case "interrupted":
    case "stopped": return `<p>Gravação ainda não processada.</p><button data-acao="processar" class="primario">Processar</button>`;
    case "ready": return formGerar(travado) + dica;
    case "draft": return `<button data-acao="aprovar" class="primario">${icone("check")} Aprovar e publicar</button>` + formMelhoria(travado) + dica;
    case "published": return formMelhoria(travado) + dica;
    case "error": {
      const repetir = st.ultima[d.id] ? `<button data-acao="repetir" class="primario">Tentar de novo</button> ` : "";
      const fundo = d.url ? formMelhoria(travado)
        : d.candidatos !== null ? formGerar(travado)
        : `<button data-acao="processar" class="primario">Processar</button>`;
      return `<pre class="erro">${esc(d.error ?? "")}</pre>${repetir}<button data-acao="pasta">Abrir pasta</button>${fundo}${dica}`;
    }
  }
}

function formGerar(travado: string): string {
  return `<form data-form="gerar" class="melhoria">
    <div id="destino-gerar" style="flex:1"></div>
    <button ${travado} class="primario">Gerar manual</button></form>`;
}

function formMelhoria(travado: string): string {
  return `<form data-form="melhoria" class="melhoria">
    <input name="texto" required placeholder="O que melhorar? ex.: junte os passos 3 e 4">
    <button ${travado} class="icone" title="Enviar melhoria">${icone("send-horizontal")}</button></form>`;
}

function htmlConfig(): string {
  const c = st.config;
  const i = st.inicio;
  if (!c || !i) return "";
  const voltar = i.configurado ? `<button type="button" data-acao="voltar">Voltar</button>` : "";
  const cabecalho = i.configurado
    ? `<div class="cabecalho"><h2>Configurações</h2>${voltar}</div>`
    : `<div class="cabecalho"><div><h2>Bem-vindo ao screenManual</h2><p class="meta">Conecte o Outline para começar.</p></div></div>`;
  /** Uma linha da lista: nome e dica à esquerda, controle à direita. */
  const ajuste = (nome: string, dica: string, controle: string) =>
    `<div class="ajuste"><div class="nome">${nome}<p class="dica">${dica}</p></div><div class="ctl">${controle}</div></div>`;
  const modelos = MODELOS.map(([m, nome]): OpcaoEscolha => ({
    valor: m,
    rotulo: nome,
    detalhe: st.download[m] ? "baixando" : i.modelos.find((x) => x.modelo === m)?.baixado ? "baixado" : `${TAMANHO[m]} a baixar`,
  }));
  const baixando = (Object.entries(st.download) as [Modelo, string][]).map(([m, pct]) => {
    const nome = MODELOS.find(([x]) => x === m)?.[1] ?? m;
    return `<p class="progresso">Baixando ${esc(nome)}… <progress max="100" value="${parseInt(pct, 10) || 0}"></progress> ${esc(pct)}</p>`;
  }).join("");
  const sk = i.skill;
  const quando = sk.editada_em
    ? new Date(sk.editada_em).toLocaleString("pt-BR", { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" }).replace(",", "")
    : "";
  const skill = `<span class="valor">${sk.personalizada ? `Personalizada${quando ? ` · editada ${esc(quando)}` : ""}` : "Padrão"}</span>
    ${sk.personalizada && sk.desatualizada ? `<p class="aviso">${icone("circle-alert")} A skill padrão mudou numa atualização. Restaure e refaça suas alterações.</p>` : ""}
    <div class="botoes"><button type="button" data-acao="editar-skill">Editar skill</button>${sk.personalizada ? `<button type="button" data-acao="restaurar-skill">Restaurar padrão</button>` : ""}</div>`;
  return `${cabecalho}
  <form data-form="outline">
    <p class="rotulo">Outline</p>
    ${ajuste("Endereço", "URL da wiki, sem barra no fim.",
      `<input name="url" value="${esc(c.outline_url)}" placeholder="https://wiki.suaempresa.com" required>`)}
    ${ajuste("Token de API", "Fica no Gerenciador de Credenciais.",
      `<input name="token" type="password" placeholder="${i.configurado ? "deixe vazio para manter o atual" : "cole o token"}">
      <div class="botoes"><button ${i.configurado ? "" : `class="primario"`}>Testar e salvar</button></div>`)}
  </form>
  <form data-form="config">
    ${ajuste("Destino padrão", "Onde o manual é publicado.", `<div id="destino-config"></div>`)}
    <p class="rotulo">Transcrição</p>
    ${ajuste("Modelo", "Mínimo é o mais rápido e erra mais; Preciso leva ~3,5 min por minuto de fala.",
      `${htmlEscolha("modelo", "audio-lines", modelos, c.transcricao.modelo)}${baixando}`)}
    ${ajuste("Desempenho do PC", "Baixo deixa o PC livre; Alto transcreve mais rápido, mas pesa.",
      htmlEscolha("desempenho", "gauge", DESEMPENHOS, c.transcricao.desempenho))}
    ${ajuste("Vocabulário da sessão", "Experimental: usa títulos de janela como dica.",
      `<label class="radio valor"><input type="checkbox" name="vocabulario" ${c.transcricao.vocabulario ? "checked" : ""}> Usar</label>`)}
    <p class="rotulo">Skill do manual</p>
    ${ajuste("Em uso", "Vale a partir da próxima geração.", skill)}
    <div class="rodape">${voltar}<button class="primario">Salvar</button></div>
  </form>`;
}

// ---------- imagens do rascunho ----------

/** URL blob da miniatura, com cache por sessão+imagem. */
async function urlImagem(id: string, imagem: string): Promise<string> {
  const k = `${id}/${imagem}`;
  if (!st.urls[k]) {
    const buf = await invoke<ArrayBuffer>("miniatura", { id, imagem });
    st.urls[k] = URL.createObjectURL(new Blob([buf], { type: "image/png" }));
  }
  return st.urls[k];
}

function htmlPassos(d: Detalhe): string {
  if (st.passos.length === 0) return "";
  const linhas = st.passos.map((p) => `<div class="passo">
    ${p.imagem ? `<img data-mini="${esc(p.imagem)}" data-zoom="${p.n}" alt="" title="Ampliar">` : `<span class="sem">${icone("image-off")}</span>`}
    <p><strong>${p.n}.</strong> ${esc(p.texto.replace(/\*\*/g, ""))}</p>
    <button data-acao="img-passo" data-n="${p.n}" class="icone" title="Imagem do passo">${icone(p.imagem ? "image" : "image-plus")}</button></div>`).join("");
  const publicar = st.alterado[d.id] ? `<button data-acao="republicar" class="primario">Publicar alterações</button>` : "";
  return `<div class="cartao"><p class="rotulo" style="margin-top:0">Passos</p>${linhas}${publicar}</div>`;
}

/** Preenche os <img data-mini> depois que o HTML entrou no DOM. */
function carregarMiniaturas(raiz: ParentNode, id: string): void {
  raiz.querySelectorAll<HTMLImageElement>("img[data-mini]").forEach((img) => {
    urlImagem(id, img.dataset.mini!).then((u) => (img.src = u)).catch(() => (img.alt = "?"));
  });
}

async function abrirImagens(n: number): Promise<void> {
  const id = st.det!.id;
  st.passoImg = n;
  $("#imagens-titulo").textContent = `Imagem do passo ${n}`;
  const atual = st.passos.find((p) => p.n === n)?.imagem ?? null;
  const lista = await invoke<ImagemUi[]>("imagens", { id });
  const g = $("#galeria");
  g.innerHTML = lista.map((i) =>
    `<img data-mini="${esc(i.id)}" data-escolher="${esc(i.id)}" class="${i.id === atual ? "sel" : ""}" title="${i.origem === "operador" ? "sua imagem" : i.id}" alt="">`).join("");
  carregarMiniaturas(g, id);
  $<HTMLDialogElement>("#imagens").showModal();
}

$<HTMLDialogElement>("#imagens").addEventListener("click", (e) => {
  const v = (e.target as HTMLElement).closest("button")?.value;
  if (v === "sem") escolherImagem(null);
  if (v === "fechar") $<HTMLDialogElement>("#imagens").close();
});
$<HTMLInputElement>("#arquivo").addEventListener("change", (e) => {
  const f = (e.target as HTMLInputElement).files?.[0];
  (e.target as HTMLInputElement).value = "";
  if (f) enviarArquivo(f);
});
$<HTMLDialogElement>("#imagens").addEventListener("paste", (e) => {
  const item = [...(e.clipboardData?.items ?? [])].find((i) => i.type.startsWith("image/"));
  const f = item?.getAsFile();
  if (f) { e.preventDefault(); enviarArquivo(f); }
});
ligarZoom();
bloquearSoltarFora();
ligarEscolhas();
ligarSoltar($("#soltar"), (f) => enviarArquivo(f), () => {
  st.aviso = "não é uma imagem PNG ou JPEG";
  $<HTMLDialogElement>("#imagens").close();
  render();
});

async function escolherImagem(imagem: string | null): Promise<void> {
  const id = st.det!.id;
  $<HTMLDialogElement>("#imagens").close();
  await tentar(async () => {
    await invoke("definir_imagem", { id, passo: st.passoImg, imagem });
    st.alterado[id] = true;
  });
}

async function enviarArquivo(blob: Blob): Promise<void> {
  const id = st.det!.id;
  const bytes = Array.from(new Uint8Array(await blob.arrayBuffer()));
  try {
    const img = await invoke<string>("adicionar_imagem", { id, bytes });
    await escolherImagem(img);
  } catch (e) {
    $<HTMLDialogElement>("#imagens").close();
    st.aviso = (e as ApiError).mensagem ?? String(e);
    render();
  }
}

async function republicar(id: string, sobrescrever = false): Promise<void> {
  if (!sobrescrever && st.det?.status === "published" &&
    !(await perguntar("Isto altera o documento publicado. Continuar?"))) return;
  try {
    await invoke("republicar", { id, sobrescrever });
    delete st.alterado[id];
  } catch (e) {
    const err = e as ApiError;
    if (err.kind === "editado_manualmente" && !sobrescrever &&
      (await perguntar(`O documento foi editado no Outline (revisão ${err.remote}; a última publicada pelo app é a ${err.local}). Sobrescrever?`))) {
      return republicar(id, true);
    }
    st.aviso = err.mensagem ?? String(e);
  }
  await carregar();
}

// ---------- eventos do DOM ----------

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && st.menu && !document.querySelector("dialog[open]")) {
    st.menu = null; st.menuExcluir = false; renderLista();
  }
});

document.addEventListener("click", async (ev) => {
  const alvo = ev.target as HTMLElement;
  if (st.menu && !alvo.closest("[data-menu]") && !alvo.closest('[data-acao="menu"]')) {
    st.menu = null; st.menuExcluir = false; renderLista();
  }
  const li = alvo.closest<HTMLElement>("li[data-id]");
  if (li && !alvo.closest("[data-acao]") && !alvo.closest("[data-menu]")) return selecionar(li.dataset.id!);
  const escolhida = alvo.closest<HTMLElement>("[data-escolher]");
  if (escolhida) return escolherImagem(escolhida.dataset.escolher!);
  const z = alvo.closest<HTMLImageElement>("img[data-zoom]");
  if (z && z.src) {
    const p = st.passos.find((x) => x.n === Number(z.dataset.zoom));
    return ampliar(z.src, p ? `Passo ${p.n}. ${p.texto.replace(/\*\*/g, "")}` : "");
  }
  const b = alvo.closest<HTMLElement>("[data-acao]");
  if (!b) return;
  ev.preventDefault();
  const id = st.det?.id ?? "";
  const alvoId = b.dataset.id ?? id;
  switch (b.dataset.acao) {
    case "nova": return abrirTitulo();
    case "cancelar-titulo": $<HTMLDialogElement>("#titulo").close(); return;
    case "pausar": return tentar(() => invoke("pausar"));
    case "parar": return tentar(() => invoke("parar"));
    case "cancelar": return tentar(() => invoke("cancelar"));
    case "processar": return executar(id, () => invoke("processar", { id, refazer: false }));
    case "menu":
      st.menu = st.menu === alvoId ? null : alvoId; st.menuExcluir = false; st.menuNovo = st.menu !== null;
      return renderLista();
    case "excluir-abrir":
      st.menuExcluir = !st.menuExcluir;
      b.nextElementSibling?.classList.toggle("aberto", st.menuExcluir);
      b.classList.toggle("aberta", st.menuExcluir);
      return;
    case "refazer-tudo": {
      st.menu = null;
      if (!(await perguntar("Isto transcreve de novo e reescreve o manual no mesmo documento. Trocas de imagem feitas no app se perdem. Continuar?"))) return renderLista();
      delete st.avisoTranscricao[alvoId];
      return executar(alvoId, (sobrescrever) => invoke("refazer_tudo", { id: alvoId, sobrescrever }));
    }
    case "retentar-audio": {
      st.menu = null;
      if (!(await perguntar("Transcrever o áudio de novo?"))) return renderLista();
      const tinhaManual = ["draft", "published"].includes(st.sessoes.find((x) => x.id === alvoId)?.status ?? "");
      await executar(alvoId, () => invoke("processar", { id: alvoId, refazer: true }));
      if (tinhaManual && !st.aviso) { st.avisoTranscricao[alvoId] = true; render(); }
      return;
    }
    case "excluir-local":
    case "excluir-outline": {
      const outline = b.dataset.acao === "excluir-outline";
      const titulo = st.sessoes.find((x) => x.id === alvoId)?.title ?? "";
      st.menu = null; st.menuExcluir = false;
      const texto = outline
        ? `Excluir a gravação "${titulo}" e mandar o documento para a lixeira do Outline?`
        : `Excluir a gravação "${titulo}" deste computador? Prints, áudio e passos são apagados. O documento no Outline fica.`;
      if (!(await perguntar(texto))) return renderLista();
      document.querySelector(`li[data-id="${CSS.escape(alvoId)}"]`)?.classList.add("saindo");
      await new Promise((r) => setTimeout(r, 150));
      return tentar(async () => {
        await invoke("excluir", { id: alvoId, outline });
        if (st.sel === alvoId) { st.sel = null; st.det = null; }
      });
    }
    case "aprovar": {
      (b as HTMLButtonElement).disabled = true;
      try { await executar(id, () => invoke("aprovar", { id })); }
      finally { (b as HTMLButtonElement).disabled = false; }
      return;
    }
    case "img-passo": return abrirImagens(Number(b.dataset.n));
    case "republicar": return republicar(id);
    case "responder": return abrirDuvidas(id);
    case "pular": {
      const sid = $<HTMLDialogElement>("#duvidas").dataset.id!;
      $<HTMLDialogElement>("#duvidas").close();
      return executar(sid, () => invoke("pular", { id: sid }));
    }
    case "cancelar-perguntas": {
      const sid = $<HTMLDialogElement>("#duvidas").dataset.id!;
      $<HTMLDialogElement>("#duvidas").close();
      return tentar(() => invoke("cancelar_perguntas", { id: sid }));
    }
    case "repetir": return st.ultima[id]?.();
    case "link": return tentar(() => invoke("abrir_link", { url: b.dataset.url }));
    case "pasta": return tentar(() => invoke("abrir_pasta", { id }));
    case "login": return tentar(() => invoke("fazer_login"));
    case "editar-skill": return tentar(() => invoke("editar_skill"));
    case "restaurar-skill":
      if (!(await perguntar("Descartar a skill personalizada e voltar à padrão?"))) return;
      return tentar(() => invoke("restaurar_skill"));
    case "config": await abrirConfig(); return render();
    case "voltar": st.tela = "sessao"; st.destConfig = null; return render();
    case "fechar-aviso": st.aviso = ""; return render();
  }
});

document.addEventListener("change", (ev) => {
  const t = ev.target as HTMLInputElement;
  if (t.name === "modelo" && t.type === "hidden") {
    baixar(t.value as Modelo);
    renderPainel();
  }
});

document.addEventListener("submit", async (ev) => {
  ev.preventDefault();
  const f = ev.target as HTMLFormElement;
  const v = new FormData(f);
  const s = (k: string) => String(v.get(k) ?? "");
  const id = st.det?.id ?? "";
  switch (f.dataset.form) {
    case "titulo": {
      $<HTMLDialogElement>("#titulo").close();
      const titulo = s("titulo");
      f.reset();
      return tentar(async () => {
        st.sel = await invoke<string>("gravar", { titulo, quando: agoraLocal() });
        st.tela = "sessao";
        await __TAURI__.window.getCurrentWindow().hide();
      });
    }
    case "duvidas": {
      const dlg = $<HTMLDialogElement>("#duvidas");
      const sid = dlg.dataset.id!;
      const respostas = lerRespostas(f);
      if (respostas.respostas.some((r) => r.escolhas.length === 0 && !r.outro)) {
        st.aviso = "Responda todas as perguntas ou clique em Pular.";
        return render();
      }
      dlg.close();
      return executar(sid, () => invoke("responder", { id: sid, respostas }));
    }
    case "gerar": {
      const v = valorGerar();
      if (!v.colecao) { st.aviso = "Escolha onde publicar."; return render(); }
      delete st.avisoTranscricao[id];
      return travar(f, () => executar(id, (sobrescrever) => invoke("gerar", { id, colecao: v.colecao, pai: v.pai, sobrescrever })));
    }
    case "melhoria": {
      if (st.det?.status === "published" &&
        !(await perguntar("Este manual já está publicado. A melhoria altera o documento publicado. Continuar?"))) return;
      const texto = s("texto");
      delete st.avisoTranscricao[id];
      await travar(f, () => executar(id, (sobrescrever) => invoke("melhorar", { id, texto, sobrescrever })));
      if (!st.aviso && st.det?.id === id) limparForm("melhoria");
      return;
    }
    case "outline":
      return tentar(async () => {
        st.colecoes = await invoke<Colecao[]>("conectar_outline", { url: s("url"), token: s("token") });
        st.config = await invoke<Config>("config");
        st.aviso = "Outline conectado.";
        st.inicio = await invoke<Inicio>("inicio");
        baixar(st.config.transcricao.modelo);
      });
    case "config": {
      const c = st.config!;
      if (st.destConfig) {
        c.colecao_padrao = st.destConfig.colecao;
        c.documento_padrao = st.destConfig.pai;
      }
      st.destConfig = null;
      c.transcricao = {
        ...c.transcricao,
        modelo: (s("modelo") || c.transcricao.modelo) as Modelo,
        vocabulario: v.has("vocabulario"),
        desempenho: (s("desempenho") || c.transcricao.desempenho || "medio") as Desempenho,
      };
      return tentar(async () => {
        await invoke("salvar_config", { cfg: c });
        st.aviso = "Configurações salvas.";
        baixar(c.transcricao.modelo);
      });
    }
  }
});

// ---------- eventos do Rust (canal "app", A8) ----------

__TAURI__.event.listen<Evento>("app", async ({ payload: ev }) => {
  switch (ev.evento) {
    case "progresso": {
      st.progresso[ev.id] = ev.texto;
      const p = document.getElementById("progresso");
      if (ev.id === st.sel && p) p.textContent = ev.texto;
      return;
    }
    case "fim":
      delete st.progresso[ev.id];
      if (ev.erro) st.aviso = ev.erro;
      return carregar();
    case "perguntas":
      await carregar();
      return abrirDuvidas(ev.id);
    case "sessao":
    case "gravacao":
      return carregar();
    case "modelo":
      st.download[ev.modelo] = `${Math.floor((ev.baixado * 100) / Math.max(ev.total, 1))}%`;
      if (st.tela === "config") renderPainel();
      return;
    case "modelo_fim":
      delete st.download[ev.modelo];
      if (ev.erro) st.aviso = `O download do modelo falhou: ${ev.erro}`;
      return carregar();
    case "pedir_titulo":
      await carregar();
      return abrirTitulo();
    case "confirmar_sair":
      if (await perguntar("Há uma gravação ou geração em andamento. Sair mesmo assim? A gravação é encerrada sem processar.")) {
        await invoke("sair");
      }
      return;
  }
});

carregar()
  .then(async () => {
    if (st.inicio?.configurado) {
      st.config = await invoke<Config>("config");
      st.colecoes = await carregarColecoes();
      render();
    }
  })
  .catch((e: ApiError) => { st.aviso = e.mensagem ?? String(e); render(); });
