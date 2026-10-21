# ai-docs

## O que e

screenManual: app Windows (Tauri v2 + Rust) que grava um procedimento e gera com Claude Code um manual passo a passo no Outline.

## Como rodar

- UI: `crates/app/ts` compila com `npm run build` (tsc) para `crates/app/ui/*.js`, que e gitignored
- instalador: `scripts/build-instalador.ps1` -> `target/release/bundle/nsis/screenManual_<versao>_x64-setup.exe`
- CI: `.github/workflows/build.yml` (windows-latest) roda o mesmo script em push no master; tag `v*` publica o setup na Release

## Armadilhas

- `screenmanual-cli.exe` precisa estar ao lado do `screenManual.exe` (`real.rs` procura em `current_exe().parent()`); o bundle o leva como resource, entao o CLI tem que ser compilado em release antes do `cargo tauri build`
- whisper-rs-sys precisa de cmake e LLVM; fora do Developer Prompt o cmake nao esta no PATH (o script acha o do VS pelo vswhere). Flags de build em `.cargo/config.toml`
- `cargo tauri build` extrai o NSIS em `%LOCALAPPDATA%\tauri`; no sandbox do Claude Code isso da "Acesso negado (os error 5)"

## Tarefa, branch e commit

- Branch: `feat|fix|chore|docs/<assunto-em-kebab-case>`
- Nao commitar direto em: main, master — master e a branch publicada (nao existe main)
- Commit: `tipo(escopo): assunto`, so assunto, sem corpo

## Wiki

Conhecimento deste repo vive em `C:\Users\joao.silva\Documents\wiki\pessoal-wiki`, dominio `projects`.
Consulte pelo `llms.txt` antes de perguntar; escreva la o que a
sessao apurou, seguindo o AGENTS.md da wiki.
