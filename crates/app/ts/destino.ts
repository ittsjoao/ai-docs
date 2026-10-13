import type { Colecao, DocNode } from "./tipos.js";
import { icone } from "./icones.js";
import { esc, normalizar } from "./texto.js";

/** Nós cujo título contém `termo` (sem acento/caixa), com os ancestrais. Termo vazio: tudo. */
export function filtrarArvore(nos: DocNode[], termo: string): DocNode[] {
  const t = normalizar(termo.trim());
  if (!t) return nos;
  const out: DocNode[] = [];
  for (const n of nos) {
    const filhos = filtrarArvore(n.children, termo);
    if (normalizar(n.title).includes(t) || filhos.length) out.push({ ...n, children: filhos });
  }
  return out;
}

export interface ValorDestino { colecao: string | null; pai: string | null; }
export interface OpcoesDestino {
  colecoes: Colecao[];
  arvore: (colecao: string) => Promise<DocNode[]>;
  valor: ValorDestino;
  aoMudar: (v: { colecao: string; pai: string | null }) => void;
}

/** Cache das árvores por coleção durante a execução do app ("erro" = não carregou). */
const arvores = new Map<string, DocNode[] | "erro">();
/** Coleções com carga em andamento (evita pedir duas vezes quando o painel re-renderiza). */
const carregando = new Set<string>();
/** Popover aberto + termo, por raiz: sobrevive ao re-render do painel. */
/** Redesenho da montagem atual de cada raiz: a carga em andamento chama as montagens vigentes, não as antigas. */
const redesenhar = new Map<string, () => void>();
const estado = new Map<string, { aberto: boolean; termo: string }>();

let escutando = false;
const aoClicarFora = (e: MouseEvent) => {
  document.querySelectorAll<HTMLElement>(".destino").forEach((r) => {
    if (r.contains(e.target as Node)) return;
    r.querySelector(".destino-pop")?.classList.remove("aberto");
    const x = estado.get(r.id);
    if (x) x.aberto = false;
  });
};

function caminho(op: OpcoesDestino): string {
  const col = op.colecoes.find((c) => c.id === op.valor.colecao);
  if (!col) return `<span class="dica">Escolha onde publicar</span>`;
  const partes = [esc(col.name)];
  const arv = arvores.get(col.id);
  const achar = (nos: DocNode[], trilha: string[]): string[] | null => {
    for (const n of nos) {
      const t = [...trilha, n.title];
      if (n.id === op.valor.pai) return t;
      const r = achar(n.children, t);
      if (r) return r;
    }
    return null;
  };
  if (op.valor.pai && Array.isArray(arv)) partes.push(...(achar(arv, []) ?? []).map(esc));
  const ult = partes.pop()!;
  return [...partes, `<b>${ult}</b>`].join(" › ");
}

function marcar(titulo: string, termo: string): string {
  const t = normalizar(termo.trim());
  const i = t ? normalizar(titulo).indexOf(t) : -1;
  if (i < 0) return esc(titulo);
  return esc(titulo.slice(0, i)) + `<mark>${esc(titulo.slice(i, i + t.length))}</mark>` + esc(titulo.slice(i + t.length));
}

export function montarDestino(raiz: HTMLElement, op: OpcoesDestino): void {
  if (!escutando) { document.addEventListener("click", aoClicarFora); escutando = true; } // uma vez só; fora do import para o node carregar o módulo
  raiz.classList.add("destino");
  raiz.innerHTML = `<button type="button" class="campo destino-campo">${icone("folder")}<span class="destino-caminho">${caminho(op)}</span>${icone("chevron-down")}</button>
    <div class="popover destino-pop"><label class="destino-busca">${icone("search")}<input placeholder="Buscar coleção ou documento…"></label><div class="destino-lista"></div></div>`;
  const campo = raiz.querySelector<HTMLButtonElement>(".destino-campo")!;
  const pop = raiz.querySelector<HTMLElement>(".destino-pop")!;
  const busca = raiz.querySelector<HTMLInputElement>("input")!;
  const lista = raiz.querySelector<HTMLElement>(".destino-lista")!;
  let realce = 0;

  const linhas = (): HTMLElement[] => [...lista.querySelectorAll<HTMLElement>("[data-col]")];
  const pintar = () => {
    const ls = linhas();
    ls.forEach((l, i) => l.classList.toggle("realce", i === realce));
    ls[realce]?.scrollIntoView({ block: "nearest" });
  };
  const desenhar = () => {
    const termo = busca.value;
    const t = normalizar(termo.trim());
    const html: string[] = [];
    for (const c of op.colecoes) {
      const arv = arvores.get(c.id);
      const nos = Array.isArray(arv) ? filtrarArvore(arv, termo) : [];
      const casaColecao = !t || normalizar(c.name).includes(t);
      if (!casaColecao && !nos.length) continue;
      html.push(`<div class="destino-col">${marcar(c.name, termo)}</div>`);
      html.push(`<div class="destino-item" data-col="${esc(c.id)}" data-pai="" style="padding-left:12px">${icone("folder")} (raiz da coleção)</div>`);
      if (arv === undefined) html.push(`<div class="dica" style="padding-left:12px">carregando…</div>`);
      if (arv === "erro") html.push(`<div class="dica" style="padding-left:12px">não foi possível carregar</div>`);
      const andar = (ns: DocNode[], nivel: number) => ns.forEach((n) => {
        html.push(`<div class="destino-item" data-col="${esc(c.id)}" data-pai="${esc(n.id)}" style="padding-left:${12 + nivel * 12}px">${icone("file-text")} ${marcar(n.title, termo)}</div>`);
        andar(n.children, nivel + 1);
      });
      andar(nos, 1);
    }
    lista.innerHTML = html.join("") || `<div class="dica">Nada encontrado.</div>`;
    realce = Math.min(realce, Math.max(linhas().length - 1, 0));
    pintar();
  };
  redesenhar.set(raiz.id, () => {
    if (!raiz.isConnected) return;
    desenhar();
    raiz.querySelector(".destino-caminho")!.innerHTML = caminho(op);
  });
  const carregarTudo = () => {
    for (const c of op.colecoes) {
      if (arvores.has(c.id) || carregando.has(c.id)) continue;
      carregando.add(c.id);
      op.arvore(c.id)
        .then((a) => arvores.set(c.id, a))
        .catch(() => arvores.set(c.id, "erro"))
        .finally(() => { carregando.delete(c.id); redesenhar.forEach((f) => f()); });
    }
  };
  const abrir = (termo = "") => {
    pop.classList.add("aberto"); realce = 0; busca.value = termo;
    estado.set(raiz.id, { aberto: true, termo });
    carregarTudo(); desenhar(); busca.focus(); busca.setSelectionRange(termo.length, termo.length);
  };
  const fechar = () => { pop.classList.remove("aberto"); estado.set(raiz.id, { aberto: false, termo: "" }); };
  const escolher = (l: HTMLElement) => {
    op.valor = { colecao: l.dataset.col!, pai: l.dataset.pai || null };
    raiz.querySelector(".destino-caminho")!.innerHTML = caminho(op);
    fechar();
    op.aoMudar({ colecao: op.valor.colecao!, pai: op.valor.pai });
  };

  campo.addEventListener("click", () => (pop.classList.contains("aberto") ? fechar() : abrir()));
  busca.addEventListener("input", () => { realce = 0; estado.set(raiz.id, { aberto: true, termo: busca.value }); desenhar(); });
  busca.addEventListener("keydown", (e) => {
    const n = linhas().length;
    if (e.key === "ArrowDown") { realce = Math.min(realce + 1, n - 1); pintar(); e.preventDefault(); }
    if (e.key === "ArrowUp") { realce = Math.max(realce - 1, 0); pintar(); e.preventDefault(); }
    if (e.key === "Enter") { const l = linhas()[realce]; if (l) escolher(l); e.preventDefault(); }
    if (e.key === "Escape") { fechar(); campo.focus(); e.preventDefault(); }
  });
  lista.addEventListener("click", (e) => {
    const l = (e.target as HTMLElement).closest<HTMLElement>("[data-col]");
    if (l) escolher(l);
  });
  if (op.valor.colecao) carregarTudo(); // para o caminho mostrar o documento pai salvo
  const antes = estado.get(raiz.id);
  if (antes?.aberto) abrir(antes.termo);
}
