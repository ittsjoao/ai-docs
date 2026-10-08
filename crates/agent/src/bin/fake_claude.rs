//! `claude` falso para os testes do `ClaudeAgent`. O modo vem de `fake-mode.txt` na pasta atual:
//! `ok` (padrão), `sleep`, `login`, `noresult`, `orphan` (deixa um neto vivo segurando o stdout)
//! `okmentionslogin`, `perguntas` (pergunta na 1ª rodada; com `--resume` ou `respostas.json`
//! publica), `resume_falha` (o `--resume` falha; do zero publica), `resume_stderr` (o
//! `--resume` sai com 1 só com stderr; do zero publica) ou `pergunta_de_novo` (sempre
//! grava `perguntas.json`). Grava os argumentos e o ambiente em `fake-args.json`.
// ponytail: binário de teste no pacote; o instalador (plano 06) só leva o screenmanual-cli
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--version") {
        println!("9.9.9 (Claude Code)");
        return;
    }
    if args.first().map(String::as_str) == Some("--hold") {
        std::thread::sleep(Duration::from_secs(30));
        return;
    }
    let env = |k: &str| std::env::var(k).ok();
    let seen = serde_json::json!({
        "args": args,
        "outline_url": env("OUTLINE_URL"),
        "token": env("OUTLINE_API_TOKEN"),
        "path": env("PATH"),
        "cwd": std::env::current_dir().unwrap().to_string_lossy(),
    });
    std::fs::write("fake-args.json", seen.to_string()).unwrap();
    match std::fs::read_to_string("fake-mode.txt")
        .unwrap_or_default()
        .trim()
    {
        "sleep" => std::thread::sleep(Duration::from_secs(60)),
        "login" => println!(
            r#"{{"type":"result","subtype":"success","is_error":true,"result":"Not logged in · Please run /login"}}"#
        ),
        "noresult" => println!(
            r#"{{"type":"result","subtype":"success","is_error":false,"result":"Parei: OUTLINE_API_TOKEN inválido (401)."}}"#
        ),
        "orphan" => {
            // neto desligado que herda o stdout (spawn sem redirecionar) e segura o pipe aberto
            let hold = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("--hold")
                .spawn()
                .unwrap();
            std::mem::forget(hold); // não esperamos: o neto deve sobreviver ao pai
            ok_stream("Publicado.");
        }
        "okmentionslogin" => ok_stream("use /login se precisar"),
        "perguntas" => {
            let resumido = args.iter().any(|a| a == "--resume");
            if resumido || std::path::Path::new("respostas.json").exists() {
                ok_stream("Publicado.");
            } else {
                println!(r#"{{"type":"system","subtype":"init","session_id":"sess-1"}}"#);
                std::fs::write(
                    "perguntas.json",
                    r#"{"perguntas":[{"id":"q1","pergunta":"Qual sistema?","opcoes":["ERP","CRM"]}]}"#,
                )
                .unwrap();
                println!(
                    r#"{{"type":"result","subtype":"success","is_error":false,"result":"Aguardando respostas.","session_id":"sess-1"}}"#
                );
            }
        }
        "resume_falha" => {
            if args.iter().any(|a| a == "--resume") {
                println!(
                    r#"{{"type":"result","subtype":"error_during_execution","is_error":true,"result":"No conversation found with session ID: sess-1"}}"#
                );
            } else {
                ok_stream("Publicado.");
            }
        }
        "resume_stderr" => {
            if args.iter().any(|a| a == "--resume") {
                eprintln!("No conversation found with session ID: sess-1");
                std::process::exit(1);
            }
            ok_stream("Publicado.");
        }
        "pergunta_de_novo" => {
            std::fs::write(
                "perguntas.json",
                r#"{"perguntas":[{"id":"q2","pergunta":"E agora?","opcoes":["a","b"]}]}"#,
            )
            .unwrap();
            println!(
                r#"{{"type":"result","subtype":"success","is_error":false,"result":"Mais dúvidas.","session_id":"sess-2"}}"#
            );
        }
        _ => ok_stream("Publicado."),
    }
}

fn ok_stream(text: &str) {
    println!(r#"{{"type":"system","subtype":"init","cwd":"x","session_id":"sess-ok"}}"#);
    println!(
        r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","name":"Read","input":{{"file_path":"C:\\s\\crops\\c001.png"}}}}]}}}}"#
    );
    println!(
        r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","name":"Read","input":{{"file_path":"C:\\s\\crops\\c002.png"}}}}]}}}}"#
    );
    println!(
        r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","name":"Bash","input":{{"command":"screenmanual-cli publish"}}}}]}}}}"#
    );
    std::fs::write(
        "result.json",
        r#"{"url":"https://wiki.x/doc/a","revision":2,"rodadas":0,"validacao":[]}"#,
    )
    .unwrap();
    println!(
        r#"{{"type":"result","subtype":"success","is_error":false,"result":"{text}","total_cost_usd":0.5}}"#
    );
}
