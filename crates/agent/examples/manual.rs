//! Gera ou melhora o manual de uma sessão com o Claude Code real, como o app fará (plano 06):
//!   cargo build -p screenmanual-cli --release
//!   cargo run -p screenmanual-agent --example manual --release -- <pasta-da-sessão> gerar [id-da-coleção]
//!   cargo run -p screenmanual-agent --example manual --release -- <pasta-da-sessão> melhoria "<pedido>" [--sobrescrever]
//!   cargo run -p screenmanual-agent --example manual --release -- <qualquer-pasta> colecoes
//!   cargo run -p screenmanual-agent --example manual --release -- <qualquer-pasta> token   (lê o token do stdin)
//! A URL e a coleção padrão vêm do config.json (%LOCALAPPDATA%\screenManual). O token vem de
//! OUTLINE_API_TOKEN ou do Credential Manager, e OUTLINE_URL sobrepõe a URL do config.
use std::io::BufRead;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use screenmanual_agent::{check_claude, find_claude, install_skill, ClaudeAgent};
use screenmanual_core::commands::{generate_manual, improve_manual};
use screenmanual_core::ports::Wiki;
use screenmanual_outline::Outline;
use screenmanual_settings::{load_token, save_token, AppConfig, Paths};
use screenmanual_store::FsStore;

const USAGE: &str =
    "uso: manual <pasta-da-sessão> gerar [coleção] | melhoria \"<pedido>\" [--sobrescrever] | colecoes | token";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(dir), Some(cmd)) = (args.first(), args.get(1)) else {
        bail!(USAGE);
    };
    if cmd == "token" {
        println!("cole o token do Outline e tecle Enter:");
        let line = std::io::stdin()
            .lock()
            .lines()
            .next()
            .context("nenhum token lido")??;
        save_token(&line)?;
        println!("token gravado no Credential Manager");
        return Ok(());
    }

    let cfg = AppConfig::load(&Paths::from_env()?.config_file())?;
    let url = std::env::var("OUTLINE_URL").unwrap_or_else(|_| cfg.outline_url.clone());
    if url.is_empty() {
        bail!("defina outline_url no config.json ou a variável OUTLINE_URL");
    }
    let token = match std::env::var("OUTLINE_API_TOKEN") {
        Ok(t) => t,
        Err(_) => {
            load_token()?.context("token do Outline não configurado: rode o comando `token`")?
        }
    };
    if cmd == "colecoes" {
        for c in Outline::new(&url, &token)?.collections()? {
            println!("{}  {}", c.id, c.name);
        }
        return Ok(());
    }

    // ponytail: pasta absoluta sem canonicalize, que no Windows devolve \\?\C:\..., caminho que
    // as regras Read(./**) do Claude não casariam
    let dir = std::path::absolute(dir).with_context(|| format!("pasta inválida: {dir}"))?;
    let root = dir
        .parent()
        .context("a pasta da sessão precisa de uma pasta-mãe")?;
    let id = dir
        .file_name()
        .and_then(|n| n.to_str())
        .context("nome de pasta inválido")?;
    let store = FsStore::new(root);

    let exe = std::env::current_exe()?;
    let cli_dir = exe
        .parent()
        .and_then(|p| p.parent())
        .context("pasta do example inesperada")?
        .to_path_buf();
    if !cli_dir.join("screenmanual-cli.exe").is_file() {
        bail!(
            "screenmanual-cli.exe não está em {}; rode cargo build -p screenmanual-cli --release",
            cli_dir.display()
        );
    }
    let home = PathBuf::from(std::env::var_os("USERPROFILE").context("USERPROFILE não definida")?);
    if install_skill(&home)? {
        println!(
            "skill /gerar-manual instalada em {}",
            home.join(".claude\\skills\\gerar-manual").display()
        );
    }
    let claude = find_claude(&std::env::var_os("PATH").unwrap_or_default(), &home)
        .context("Claude Code não encontrado no PATH nem em ~/.local/bin")?;
    println!(
        "Claude Code {} ({})",
        check_claude(&claude)?,
        claude.display()
    );
    let agent = ClaudeAgent::new(claude, cli_dir, &url, &token, &cfg.modelo_claude);

    let started = Instant::now();
    let mut progress = |p: &str| println!("[{:>4} s] {p}", started.elapsed().as_secs());
    let result = match cmd.as_str() {
        "gerar" => {
            let collection = args
                .get(2)
                .cloned()
                .or(cfg.colecao_padrao.clone())
                .context("informe a coleção ou defina colecao_padrao no config.json")?;
            generate_manual(&store, &agent, id, &collection, None, &mut progress)?
        }
        "melhoria" => {
            let text = args.get(2).context(USAGE)?;
            let overwrite = args.iter().any(|a| a == "--sobrescrever");
            let wiki = Outline::new(&url, &token)?;
            improve_manual(&store, &wiki, &agent, id, text, overwrite, &mut progress)?
        }
        _ => bail!(USAGE),
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    println!("em {} s", started.elapsed().as_secs());
    Ok(())
}
