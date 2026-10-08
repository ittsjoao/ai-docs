// UI do screenManual (adendo 2026-10-05): TS puro, sem framework; o Tauri vem em window.__TAURI__.
interface TauriGlobal {
  core: { invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> };
  event: { listen<T>(nome: string, cb: (e: { payload: T }) => void): Promise<() => void> };
  window: { getCurrentWindow(): { hide(): Promise<void> } };
}
declare const __TAURI__: TauriGlobal;

type Status = "recording" | "interrupted" | "stopped" | "processing" | "ready" | "generating" | "awaiting" | "draft" | "published" | "error";
type Modelo = "rapido" | "equilibrado" | "preciso";
type Gravacao = "gravando" | "pausado" | "parado";

interface Resumo { id: string; title: string; started_at: string; duration_ms: number | null; status: Status; url: string | null; error: string | null; }
interface Validacao { tipo: string; detalhe: string; }
interface Detalhe extends Resumo { candidatos: number | null; colecao: string | null; pai: string | null; validacao: Validacao[]; pasta: string; }
interface Inicio {
  claude_versao: string | null; claude_erro: string | null; configurado: boolean;
  gravacao: Gravacao; gravando: string | null; gerando: string | null;
  modelo_atual: Modelo; modelos: { modelo: Modelo; baixado: boolean }[];
}
interface PassoUi { secao: string; n: number; texto: string; imagem: string | null; }
interface ImagemUi { id: string; origem: "gravacao" | "operador"; }
interface Colecao { id: string; name: string; }
interface DocNode { id: string; title: string; children: DocNode[]; }
interface Config { outline_url: string; colecao_padrao: string | null; documento_padrao: string | null; transcricao: { modelo: Modelo; vocabulario: boolean }; [k: string]: unknown; }
interface ApiError { kind: string; mensagem: string; local?: number; remote?: number; }
interface Pergunta { id: string; pergunta: string; opcoes: string[]; multipla: boolean; }
interface Perguntas { perguntas: Pergunta[]; }
interface AgentResult { tipo: "pronto" | "perguntas"; url?: string; validacao?: Validacao[]; }
type Evento =
  | { evento: "sessao"; id: string }
  | { evento: "progresso"; id: string; texto: string }
  | { evento: "modelo"; modelo: Modelo; baixado: number; total: number }
  | { evento: "modelo_fim"; modelo: Modelo; erro: string | null }
  | { evento: "gravacao"; estado: Gravacao; id: string }
  | { evento: "fim"; id: string; titulo: string; url: string | null; erro: string | null }
  | { evento: "perguntas"; id: string; titulo: string }
  | { evento: "pedir_titulo" }
  | { evento: "confirmar_sair" };

const invoke = __TAURI__.core.invoke;

const STATUS: Record<Status, string> = {
  recording: "gravando", interrupted: "interrompida", stopped: "parada", processing: "processando…",
  ready: "pronta", generating: "gerando…", awaiting: "aguardando respostas", draft: "rascunho", published: "publicado", error: "erro",
};
const MODELOS: [Modelo, string][] = [
  ["rapido", "Rápido — ~1 min de processamento por minuto de fala"],
  ["equilibrado", "Equilibrado — ~2 min por minuto de fala"],
  ["preciso", "Preciso — ~3,5 min por minuto de fala (padrão)"],
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
  arvores: {} as Record<string, DocNode[] | "carregando">,
  colGerar: null as string | null,
  colConfig: null as string | null,
  config: null as Config | null,
  aviso: "",
  login: false,
  /** "Tentar de novo": a última ação disparada para cada sessão (A9). */
  ultima: {} as Record<string, () => Promise<void>>,
  passos: [] as PassoUi[],
  alterado: {} as Record<string, boolean>,
  urls: {} as Record<string, string>,
  passoImg: 0,
};

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector(sel) as T;
const esc = (s: string) => s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

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
  st.colGerar = null;
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
    ? `<button data-acao="nova">▶ Gravar</button>`
    : `<button data-acao="pausar" class="sec">${i.gravacao === "pausado" ? "▶ Retomar" : "⏸ Pausar"}</button><button data-acao="parar">■ Parar</button>`;
  const faixa: string[] = [];
  if (i.claude_erro) faixa.push(`Claude Code não instalado: ${esc(i.claude_erro)}`);
  if (st.login) faixa.push(`O Claude Code não está logado. <button data-acao="login">Fazer login</button>`);
  if (st.aviso) faixa.push(`${esc(st.aviso)} <button data-acao="fechar-aviso" class="sec">×</button>`);
  $("#faixa").innerHTML = faixa.map((f) => `<p>${f}</p>`).join("");
}

function renderLista(): void {
  $("#lista").innerHTML = st.sessoes.length === 0
    ? `<li class="vazio">Nenhuma gravação ainda. Clique em ▶ Gravar.</li>`
    : st.sessoes.map((s) =>
      `<li data-id="${esc(s.id)}" class="${s.id === st.sel ? "sel" : ""}"><strong>${esc(s.title)}</strong><span class="st-${s.status}">${STATUS[s.status]}</span></li>`).join("");
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

function renderPainel(): void {
  const p = $("#painel");
  if (st.tela === "config") return trocar(p, htmlConfig());
  const d = st.det;
  if (!d) return trocar(p, `<p class="vazio">Selecione uma sessão ou clique em ▶ Gravar.</p>`);
  const dur = d.duration_ms ? ` · ${Math.max(1, Math.round(d.duration_ms / 60000))} min` : "";
  const cand = d.candidatos !== null ? ` · ${d.candidatos} passos candidatos` : "";
  let h = `<h2>${esc(d.title)}</h2><p class="meta">${STATUS[d.status]}${dur}${cand}</p>`;
  if (d.url) h += `<p><a href="#" data-acao="link" data-url="${esc(d.url)}">🔗 ${esc(d.url)}</a></p>`;
  h += d.validacao.map((v) => `<p class="aviso">⚠ ${esc(v.tipo)}: ${esc(v.detalhe)}</p>`).join("");
  trocar(p, h + htmlAcoes(d));
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
    case "recording": return `<p>Gravando… use ⏸ / ■ no topo ou na bandeja. Ctrl+Alt+P pausa/retoma; Ctrl+Alt+M marca um passo.</p>`;
    case "processing": return prog;
    case "awaiting": return `<p>A IA tem dúvidas antes de escrever o manual.</p><button data-acao="responder">Responder…</button>`;
    case "generating": return prog + `<button data-acao="cancelar" class="sec">Cancelar</button>`;
    case "interrupted":
    case "stopped": return `<p>Gravação ainda não processada.</p><button data-acao="processar">Processar</button>`;
    case "ready": return formGerar(travado) + dica + `<button data-acao="reprocessar" class="sec">Refazer transcrição</button>`;
    case "draft": return `<button data-acao="aprovar">✓ Aprovar e publicar</button>` + htmlPassos(d) + formMelhoria(travado) + dica;
    case "published": return htmlPassos(d) + formMelhoria(travado) + dica;
    case "error": {
      const repetir = st.ultima[d.id] ? `<button data-acao="repetir">Tentar de novo</button> ` : "";
      const fundo = d.url ? formMelhoria(travado)
        : d.candidatos !== null ? formGerar(travado)
        : `<button data-acao="processar">Processar</button>`;
      return `<pre class="erro">${esc(d.error ?? "")}</pre>${repetir}<button data-acao="pasta" class="sec">Abrir pasta</button>${fundo}${dica}`;
    }
  }
}

function opcoesColecao(sel: string | null): string {
  return st.colecoes.map((c) => `<option value="${esc(c.id)}" ${c.id === sel ? "selected" : ""}>${esc(c.name)}</option>`).join("");
}

/** Carrega a árvore da coleção uma vez por abertura do app e re-renderiza. */
function garantirArvore(colecao: string | null): void {
  if (!colecao || st.arvores[colecao]) return;
  st.arvores[colecao] = "carregando";
  invoke<DocNode[]>("documentos", { colecao })
    .then((a) => { st.arvores[colecao] = a; })
    .catch((e: ApiError) => { st.arvores[colecao] = []; st.aviso = e.mensagem ?? String(e); })
    .finally(render);
}

function opcoesPai(colecao: string | null, sel: string | null): string {
  const arv = colecao ? st.arvores[colecao] : undefined;
  if (arv === "carregando") return `<option value="">carregando…</option>`;
  const nos: string[] = [];
  const andar = (lista: DocNode[], nivel: number) => lista.forEach((n) => {
    nos.push(`<option value="${esc(n.id)}" ${n.id === sel ? "selected" : ""}>${"— ".repeat(nivel)}${esc(n.title)}</option>`);
    andar(n.children, nivel + 1);
  });
  andar(arv ?? [], 1);
  return `<option value="">(raiz da coleção)</option>` + nos.join("");
}

function existeNaArvore(colecao: string | null, id: string | null): boolean {
  const arv = colecao ? st.arvores[colecao] : undefined;
  if (!id || !Array.isArray(arv)) return true;
  const tem = (l: DocNode[]): boolean => l.some((n) => n.id === id || tem(n.children));
  return tem(arv);
}

function formGerar(travado: string): string {
  const colecao = st.colGerar ?? st.det?.colecao ?? st.config?.colecao_padrao ?? st.colecoes[0]?.id ?? null;
  const pai = st.det?.pai ?? (colecao === st.config?.colecao_padrao ? st.config?.documento_padrao : null) ?? null;
  garantirArvore(colecao);
  const sumiu = !existeNaArvore(colecao, pai)
    ? `<p class="aviso">⚠ o documento padrão não existe mais no Outline; o manual vai para a raiz da coleção.</p>` : "";
  return `<form data-form="gerar">
    <label>Coleção do Outline <select name="colecao" required>${opcoesColecao(colecao)}</select></label>
    <label>Dentro de <select name="pai">${opcoesPai(colecao, existeNaArvore(colecao, pai) ? pai : null)}</select></label>
    ${sumiu}
    <button ${travado}>Gerar manual</button></form>`;
}

function formMelhoria(travado: string): string {
  return `<form data-form="melhoria">
    <label>O que melhorar <textarea name="texto" rows="3" required placeholder="ex.: junte os passos 3 e 4"></textarea></label>
    <button ${travado}>Enviar melhoria</button></form>`;
}

function htmlConfig(): string {
  const c = st.config;
  const i = st.inicio;
  if (!c || !i) return "";
  garantirArvore(st.colConfig ?? c.colecao_padrao);
  const titulo = i.configurado ? `<h2>Configurações</h2>` : `<h2>Bem-vindo ao screenManual</h2><p>Conecte o Outline para começar.</p>`;
  const modelos = MODELOS.map(([m, rotulo]) => {
    const baixado = i.modelos.find((x) => x.modelo === m)?.baixado;
    const pct = st.download[m];
    const estado = pct ? `baixando ${pct} <progress max="100" value="${parseInt(pct, 10) || 0}"></progress>` : baixado ? "baixado" : "não baixado";
    return `<label class="radio"><input type="radio" name="modelo" value="${m}" ${c.transcricao.modelo === m ? "checked" : ""}> ${rotulo} <em>(${estado})</em></label>`;
  }).join("");
  return `${titulo}
  <form data-form="outline">
    <label>URL do Outline <input name="url" value="${esc(c.outline_url)}" placeholder="https://wiki.suaempresa.com" required></label>
    <label>Token de API <input name="token" type="password" placeholder="${i.configurado ? "deixe vazio para manter o atual" : "cole o token"}"></label>
    <button>Testar e salvar</button>
  </form>
  <form data-form="config">
    <label>Destino padrão: coleção <select name="colecao"><option value="">—</option>${opcoesColecao(st.colConfig ?? c.colecao_padrao)}</select></label>
    <label>Dentro de <select name="pai">${opcoesPai(st.colConfig ?? c.colecao_padrao, c.documento_padrao)}</select></label>
    <fieldset><legend>Transcrição</legend>${modelos}
      <label class="radio"><input type="checkbox" name="vocabulario" ${c.transcricao.vocabulario ? "checked" : ""}> Usar vocabulário da sessão (experimental)</label>
    </fieldset>
    <button>Salvar</button>
    ${i.configurado ? `<button type="button" data-acao="voltar" class="sec">Voltar</button>` : ""}
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
    ${p.imagem ? `<img data-mini="${esc(p.imagem)}" alt="">` : `<span class="sem">sem imagem</span>`}
    <p><strong>${p.n}.</strong> ${esc(p.texto.replace(/\*\*/g, ""))}</p>
    <button data-acao="img-passo" data-n="${p.n}" class="sec" title="Imagem do passo">🖼</button></div>`).join("");
  const publicar = st.alterado[d.id] ? `<button data-acao="republicar">Publicar alterações</button>` : "";
  return `<h3>Passos</h3>${linhas}${publicar}`;
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

document.addEventListener("click", async (ev) => {
  const alvo = ev.target as HTMLElement;
  const li = alvo.closest<HTMLElement>("li[data-id]");
  if (li) return selecionar(li.dataset.id!);
  const escolhida = alvo.closest<HTMLElement>("[data-escolher]");
  if (escolhida) return escolherImagem(escolhida.dataset.escolher!);
  const b = alvo.closest<HTMLElement>("[data-acao]");
  if (!b) return;
  ev.preventDefault();
  const id = st.det?.id ?? "";
  switch (b.dataset.acao) {
    case "nova": return abrirTitulo();
    case "cancelar-titulo": $<HTMLDialogElement>("#titulo").close(); return;
    case "pausar": return tentar(() => invoke("pausar"));
    case "parar": return tentar(() => invoke("parar"));
    case "cancelar": return tentar(() => invoke("cancelar"));
    case "processar": return executar(id, () => invoke("processar", { id, refazer: false }));
    case "reprocessar": return executar(id, () => invoke("processar", { id, refazer: true }));
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
    case "config": await abrirConfig(); return render();
    case "voltar": st.tela = "sessao"; return render();
    case "fechar-aviso": st.aviso = ""; return render();
  }
});

document.addEventListener("change", (ev) => {
  const t = ev.target as HTMLInputElement;
  if (t.name === "colecao") {
    const form = t.closest("form")?.dataset.form;
    if (form === "gerar") st.colGerar = t.value;
    if (form === "config") st.colConfig = t.value || null;
    garantirArvore(t.value || null);
    renderPainel();
    return;
  }
  if (t.name === "modelo" && t.type === "radio") {
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
      const colecao = s("colecao");
      const pai = s("pai") || null;
      return travar(f, () => executar(id, (sobrescrever) => invoke("gerar", { id, colecao, pai, sobrescrever })));
    }
    case "melhoria": {
      if (st.det?.status === "published" &&
        !(await perguntar("Este manual já está publicado. A melhoria altera o documento publicado. Continuar?"))) return;
      const texto = s("texto");
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
      c.colecao_padrao = s("colecao") || null;
      c.documento_padrao = (c.colecao_padrao && s("pai")) || null;
      c.transcricao = { modelo: (s("modelo") || c.transcricao.modelo) as Modelo, vocabulario: v.has("vocabulario") };
      st.colConfig = null;
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
