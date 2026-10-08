//! Composition root do app (spec §4, adendo 2026-10-05 §3).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod commands;
mod real;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use screenmanual_app::{texto_notificacao, App, EstadoGravacao, Evento};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;

/// Atalhos de gravação registrados?
static ATALHOS: AtomicBool = AtomicBool::new(false);

pub type AppState = Arc<App<real::RealDeps>>;

fn atalho_pausar() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyP)
}

fn atalho_marcar() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyM)
}

fn estado(app: &AppHandle) -> AppState {
    app.state::<AppState>().inner().clone()
}

/// Círculo de 32×32 na cor do estado (spec §5: vermelho gravando, amarelo pausado).
fn icone(e: EstadoGravacao) -> Image<'static> {
    let [r, g, b] = match e {
        EstadoGravacao::Gravando => [220, 38, 38],
        EstadoGravacao::Pausado => [234, 179, 8],
        EstadoGravacao::Parado => [100, 116, 139],
    };
    let mut px = Vec::with_capacity(32 * 32 * 4);
    for y in 0..32 {
        for x in 0..32 {
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
            let alfa = if dx * dx + dy * dy <= 14.0 * 14.0 {
                255
            } else {
                0
            };
            px.extend_from_slice(&[r, g, b, alfa]);
        }
    }
    Image::new_owned(px, 32, 32)
}

fn mostrar(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// "Sair" da bandeja: com gravação ou geração em andamento, a UI confirma antes.
fn sair(app: &AppHandle) {
    if estado(app).ocupado() {
        mostrar(app);
        let _ = app.emit("app", serde_json::json!({"evento": "confirmar_sair"}));
    } else {
        app.exit(0);
    }
}

/// Repassa o evento à UI. Antes disso, atualiza a bandeja e os atalhos e notifica o fim.
fn ao_evento(app: &AppHandle, ev: Evento) {
    match &ev {
        Evento::Gravacao { estado, .. } => {
            if let Some(tray) = app.tray_by_id("main") {
                let _ = tray.set_icon(Some(icone(*estado)));
            }
            let atalhos = app.global_shortcut();
            // Sem `is_registered`/`unregister_all`: o plugin chama o handler com o lock dos atalhos
            // preso, e `unregister_all` segura esse lock esperando a thread principal.
            if *estado == EstadoGravacao::Parado {
                if ATALHOS.swap(false, Ordering::SeqCst) {
                    let _ = atalhos.unregister_multiple([atalho_pausar(), atalho_marcar()]);
                }
                mostrar(app);
            } else if !ATALHOS.swap(true, Ordering::SeqCst) {
                let _ = atalhos.register(atalho_pausar());
                let _ = atalhos.register(atalho_marcar());
            }
        }
        Evento::Fim {
            titulo,
            acao,
            url,
            erro,
            ..
        } => {
            let (titulo, corpo) = texto_notificacao(*acao, titulo, url.as_deref(), erro.as_deref());
            let _ = app
                .notification()
                .builder()
                .title(titulo)
                .body(corpo)
                .show();
        }
        _ => {}
    }
    let _ = app.emit("app", &ev);
}

fn criar_bandeja(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "gravar", "▶ Gravar", true, None::<&str>)?,
            &MenuItem::with_id(app, "pausar", "⏸ Pausar / retomar", true, None::<&str>)?,
            &MenuItem::with_id(app, "parar", "■ Parar", true, None::<&str>)?,
            &MenuItem::with_id(app, "abrir", "Abrir", true, None::<&str>)?,
            &MenuItem::with_id(app, "sair", "Sair", true, None::<&str>)?,
        ],
    )?;
    TrayIconBuilder::with_id("main")
        .icon(icone(EstadoGravacao::Parado))
        .tooltip("screenManual")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "gravar" => {
                mostrar(app);
                let _ = app.emit("app", serde_json::json!({"evento": "pedir_titulo"}));
            }
            "pausar" => {
                let st = estado(app);
                std::thread::spawn(move || {
                    let _ = st.pausar();
                });
            }
            "parar" => {
                let st = estado(app);
                std::thread::spawn(move || {
                    let _ = st.parar();
                });
            }
            "abrir" => mostrar(app),
            "sair" => sair(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = ev
            {
                mostrar(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn main() {
    screenmanual_capture::set_dpi_awareness();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            mostrar(app)
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, atalho, ev| {
                    if ev.state() != ShortcutState::Pressed {
                        return;
                    }
                    // Em outra thread: o plugin chama este handler com o lock dos atalhos preso.
                    let st = estado(app);
                    if atalho == &atalho_pausar() {
                        std::thread::spawn(move || {
                            let _ = st.pausar();
                        });
                    } else if atalho == &atalho_marcar() {
                        std::thread::spawn(move || {
                            let _ = st.marcar();
                        });
                    }
                })
                .build(),
        )
        .setup(|app| {
            let deps = real::RealDeps::new()?;
            let config_file = deps.paths.config_file();
            let handle = app.handle().clone();
            let state: AppState = Arc::new(App::new(
                deps,
                config_file,
                Box::new(move |ev| ao_evento(&handle, ev)),
            ));
            app.manage(state);
            criar_bandeja(app.handle())?;
            Ok(())
        })
        .on_window_event(|w, ev| {
            if let WindowEvent::CloseRequested { api, .. } = ev {
                api.prevent_close();
                let _ = w.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::inicio,
            commands::sessoes,
            commands::detalhe,
            commands::config,
            commands::salvar_config,
            commands::conectar_outline,
            commands::colecoes,
            commands::documentos,
            commands::baixar_modelo,
            commands::gravar,
            commands::pausar,
            commands::marcar,
            commands::parar,
            commands::processar,
            commands::gerar,
            commands::melhorar,
            commands::aprovar,
            commands::passos,
            commands::imagens,
            commands::miniatura,
            commands::definir_imagem,
            commands::adicionar_imagem,
            commands::republicar,
            commands::cancelar,
            commands::abrir_link,
            commands::abrir_pasta,
            commands::fazer_login,
            commands::sair,
        ])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o screenManual");
}
