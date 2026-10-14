// Lista de escolha no mesmo visual do campo de destino (botão-campo + popover), no lugar do <select> nativo.
import { icone, type NomeIcone } from "./icones.js";
import { esc } from "./texto.js";

export interface OpcaoEscolha { valor: string; rotulo: string; detalhe?: string; }

/**
 * Campo de escolha. O valor vive num `<input type="hidden" name=…>`: entra no FormData do form e é
 * preservado pelo `trocar()` do painel; ao escolher, o input dispara `change` como um select faria.
 */
export function htmlEscolha(nome: string, ic: NomeIcone, opcoes: OpcaoEscolha[], atual: string): string {
  const itens = opcoes.map((o) => `<button type="button" class="destino-item escolha-item" data-valor="${esc(o.valor)}">
      <span class="escolha-rotulo">${esc(o.rotulo)}</span>${o.detalhe ? `<span class="dica">${esc(o.detalhe)}</span>` : ""}${icone("check", "ic escolha-check")}</button>`).join("");
  return `<div class="destino escolha">
    <input type="hidden" name="${esc(nome)}" value="${esc(atual)}">
    <button type="button" class="campo destino-campo">${icone(ic)}<span class="destino-caminho"></span>${icone("chevron-down")}</button>
    <div class="popover destino-pop"><div class="destino-lista">${itens}</div></div>
  </div>`;
}

/** Mostra no botão a opção do input escondido (depois de um re-render que restaurou o valor). */
export function sincronizarEscolhas(raiz: HTMLElement): void {
  raiz.querySelectorAll<HTMLElement>(".escolha").forEach((e) => {
    const v = e.querySelector<HTMLInputElement>("input[type=hidden]")!.value;
    let rotulo = "";
    e.querySelectorAll<HTMLElement>(".escolha-item").forEach((i) => {
      const sel = i.dataset.valor === v;
      i.classList.toggle("sel", sel);
      if (sel) rotulo = i.querySelector(".escolha-rotulo")!.textContent ?? "";
    });
    e.querySelector(".destino-caminho")!.textContent = rotulo;
  });
}

const fecharTodas = (exceto?: Element) =>
  document.querySelectorAll(".escolha .destino-pop.aberto").forEach((p) => { if (p.parentElement !== exceto) p.classList.remove("aberto"); });

/** Liga os campos de escolha por delegação no documento; chamar uma vez. */
export function ligarEscolhas(): void {
  document.addEventListener("click", (ev) => {
    const alvo = ev.target as HTMLElement;
    const raiz = alvo.closest<HTMLElement>(".escolha");
    fecharTodas(raiz ?? undefined);
    if (!raiz) return;
    const pop = raiz.querySelector<HTMLElement>(".destino-pop")!;
    const item = alvo.closest<HTMLElement>(".escolha-item");
    if (item) {
      const input = raiz.querySelector<HTMLInputElement>("input[type=hidden]")!;
      pop.classList.remove("aberto");
      raiz.querySelector<HTMLElement>(".destino-campo")!.focus();
      if (input.value === item.dataset.valor) return;
      input.value = item.dataset.valor!;
      sincronizarEscolhas(raiz.parentElement!);
      input.dispatchEvent(new Event("change", { bubbles: true }));
      return;
    }
    if (alvo.closest(".destino-campo")) {
      pop.classList.toggle("aberto");
      pop.querySelector<HTMLElement>(".escolha-item.sel, .escolha-item")?.focus();
    }
  });
  document.addEventListener("keydown", (ev) => {
    const raiz = (ev.target as HTMLElement).closest?.(".escolha");
    if (!raiz) return;
    const pop = raiz.querySelector<HTMLElement>(".destino-pop")!;
    if (ev.key === "Escape" && pop.classList.contains("aberto")) {
      pop.classList.remove("aberto");
      raiz.querySelector<HTMLElement>(".destino-campo")!.focus();
      ev.preventDefault();
      return;
    }
    if (ev.key !== "ArrowDown" && ev.key !== "ArrowUp") return;
    const itens = [...raiz.querySelectorAll<HTMLElement>(".escolha-item")];
    const i = itens.indexOf(document.activeElement as HTMLElement);
    if (!pop.classList.contains("aberto")) { pop.classList.add("aberto"); (itens.find((x) => x.classList.contains("sel")) ?? itens[0])?.focus(); }
    else itens[Math.max(0, Math.min(itens.length - 1, i + (ev.key === "ArrowDown" ? 1 : -1)))]?.focus();
    ev.preventDefault();
  });
}
