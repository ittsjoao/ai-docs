/** Foto do passo em tamanho grande, num <dialog> nativo. */
export function ampliar(src: string, legenda: string): void {
  const dlg = document.getElementById("zoom") as HTMLDialogElement;
  (document.getElementById("zoom-img") as HTMLImageElement).src = src;
  document.getElementById("zoom-legenda")!.textContent = legenda;
  dlg.showModal();
}

/** Fecha o zoom ao clicar fora da imagem ou no botão. */
export function ligarZoom(): void {
  const dlg = document.getElementById("zoom") as HTMLDialogElement;
  dlg.addEventListener("click", (e) => {
    const t = e.target as HTMLElement;
    if (t === dlg || t.closest("button")) dlg.close();
  });
}

/** Arrastar e soltar um arquivo em `alvo`; o primeiro arquivo vai para `aoArquivo`. */
export function ligarSoltar(alvo: HTMLElement, aoArquivo: (f: File) => void): void {
  alvo.addEventListener("dragover", (e) => { e.preventDefault(); alvo.classList.add("sobre"); });
  alvo.addEventListener("dragleave", () => alvo.classList.remove("sobre"));
  alvo.addEventListener("drop", (e) => {
    e.preventDefault();
    alvo.classList.remove("sobre");
    const f = e.dataTransfer?.files[0];
    if (f) aoArquivo(f);
  });
}
