// Tipos compartilhados da UI e o global do Tauri.
export interface TauriGlobal {
  core: { invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> };
  event: { listen<T>(nome: string, cb: (e: { payload: T }) => void): Promise<() => void> };
  window: { getCurrentWindow(): { hide(): Promise<void> } };
}
declare global {
  const __TAURI__: TauriGlobal;
}

export type Status = "recording" | "interrupted" | "stopped" | "processing" | "ready" | "generating" | "awaiting" | "draft" | "published" | "error";
export type Modelo = "minimo" | "rapido" | "equilibrado" | "preciso";
export type Desempenho = "baixo" | "medio" | "alto";
export type Gravacao = "gravando" | "pausado" | "parado";
export interface SkillStatus { personalizada: boolean; editada_em: number | null; desatualizada: boolean; }

export interface Resumo { id: string; title: string; started_at: string; duration_ms: number | null; status: Status; url: string | null; error: string | null; }
export interface Validacao { tipo: string; detalhe: string; }
export interface Detalhe extends Resumo { candidatos: number | null; colecao: string | null; pai: string | null; validacao: Validacao[]; pasta: string; }
export interface Inicio {
  claude_versao: string | null; claude_erro: string | null; configurado: boolean;
  gravacao: Gravacao; gravando: string | null; gerando: string | null;
  modelo_atual: Modelo; modelos: { modelo: Modelo; baixado: boolean }[]; skill: SkillStatus;
}
export interface PassoUi { secao: string; n: number; texto: string; imagem: string | null; }
export interface ImagemUi { id: string; origem: "gravacao" | "operador"; }
export interface Colecao { id: string; name: string; }
export interface DocNode { id: string; title: string; children: DocNode[]; }
export interface Config { outline_url: string; colecao_padrao: string | null; documento_padrao: string | null; transcricao: { modelo: Modelo; vocabulario: boolean; desempenho: Desempenho }; [k: string]: unknown; }
export interface ApiError { kind: string; mensagem: string; local?: number; remote?: number; }
export interface Pergunta { id: string; pergunta: string; opcoes: string[]; multipla: boolean; }
export interface Perguntas { perguntas: Pergunta[]; }
export interface AgentResult { tipo: "pronto" | "perguntas"; url?: string; validacao?: Validacao[]; }
export type Evento =
  | { evento: "sessao"; id: string }
  | { evento: "progresso"; id: string; texto: string }
  | { evento: "modelo"; modelo: Modelo; baixado: number; total: number }
  | { evento: "modelo_fim"; modelo: Modelo; erro: string | null }
  | { evento: "gravacao"; estado: Gravacao; id: string }
  | { evento: "fim"; id: string; titulo: string; url: string | null; erro: string | null }
  | { evento: "perguntas"; id: string; titulo: string }
  | { evento: "pedir_titulo" }
  | { evento: "confirmar_sair" };
