//! Binário chamado pela skill: `screenmanual-cli render | publish | fetch | redact <crop> <x,y,w,h>`.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            println!("erro: pasta atual inacessível: {e}");
            std::process::exit(1);
        }
    };
    let (code, out) = screenmanual_cli::run(&args, &dir, screenmanual_outline::Outline::from_env);
    println!("{out}");
    std::process::exit(code);
}
