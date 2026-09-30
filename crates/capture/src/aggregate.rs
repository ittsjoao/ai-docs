use screenmanual_core::domain::{CaptureConfig, Element, Event, MouseButton, PauseReason, Rect};

use crate::keys::{classify_key, KeyKind};
use crate::privacy::{denied, sanitize_url};

/// Entrada crua vinda dos hooks, do relógio (Tick) ou do handle (pausa, marcador, stop).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Raw {
    MouseDown {
        t: u64,
        x: i32,
        y: i32,
        button: MouseButton,
    },
    MouseUp {
        t: u64,
        x: i32,
        y: i32,
        button: MouseButton,
    },
    Key {
        t: u64,
        vk: u32,
        ctrl: bool,
        alt: bool,
        shift: bool,
    },
    Tick {
        t: u64,
    },
    Pause {
        t: u64,
    },
    Resume {
        t: u64,
    },
    Marker {
        t: u64,
    },
    AudioLost {
        t: u64,
    },
    Stop {
        t: u64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Foreground {
    pub app: String,
    pub title: String,
    pub pid: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Probed {
    pub el: Element,
    pub pid: u32,
    pub root_class: String,
    /// Exe da janela raiz sob o ponto, em minúsculas.
    pub app: String,
    /// Título da janela raiz sob o ponto.
    pub title: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Shot {
    pub path: String,
    pub dhash: u64,
    pub monitor: Rect,
}

/// O que o agregador precisa perguntar ao sistema. Implementado por `WinProbe`, e por um fake nos testes.
pub(crate) trait Probe {
    fn foreground(&mut self) -> Option<Foreground>;
    fn element_at(&mut self, x: i32, y: i32) -> Option<Probed>;
    fn focused(&mut self) -> Option<Probed>;
    /// Captura o monitor do ponto; `blackout` (coordenadas de tela) é pintado de preto antes de salvar.
    fn screenshot(&mut self, t: u64, x: i32, y: i32, blackout: Option<Rect>) -> Option<Shot>;
    /// Texto cru da barra de endereço da janela ativa (só é chamado para navegadores).
    fn url(&mut self) -> Option<String>;
}

const BROWSERS: &[&str] = &["chrome.exe", "msedge.exe", "brave.exe", "firefox.exe"];

/// VKs de P e M: Ctrl+Alt+P/M são os atalhos do próprio app (pausar / marcar passo).
const OWN_HOTKEY_VKS: &[u32] = &[0x50, 0x4D];
const SHELL_CLASSES: &[&str] = &[
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "NotifyIconOverflowWindow",
];
const NO_MONITOR: Rect = Rect {
    left: 0,
    top: 0,
    right: 0,
    bottom: 0,
};

struct Pending {
    t: u64,
    x: i32,
    y: i32,
    button: MouseButton,
    shot: Option<Shot>,
    el: Option<Element>,
}

struct Typing {
    t: u64,
    el: Option<Element>,
    chars: u32,
    password: bool,
}

pub(crate) struct Aggregator<P: Probe> {
    probe: P,
    cfg: CaptureConfig,
    own_pid: u32,
    fg: Option<Foreground>,
    paused_manual: bool,
    paused_auto: bool,
    pending: Option<Pending>,
    ignored_button: Option<MouseButton>,
    typing: Option<Typing>,
}

impl<P: Probe> Aggregator<P> {
    pub(crate) fn new(probe: P, cfg: CaptureConfig, own_pid: u32) -> Self {
        Self {
            probe,
            cfg,
            own_pid,
            fg: None,
            paused_manual: false,
            paused_auto: false,
            pending: None,
            ignored_button: None,
            typing: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn probe_mut(&mut self) -> &mut P {
        &mut self.probe
    }

    pub(crate) fn start(&mut self) -> Vec<Event> {
        let mut out = vec![Event::SessionStart { t: 0 }];
        self.check_foreground(0, &mut out);
        out
    }

    pub(crate) fn feed(&mut self, raw: Raw) -> Vec<Event> {
        let mut out = Vec::new();
        match raw {
            Raw::Tick { t } => self.check_foreground(t, &mut out),
            Raw::MouseDown { t, x, y, button } => {
                self.check_foreground(t, &mut out);
                if !self.paused() {
                    self.flush_typing(&mut out);
                    self.mouse_down(t, x, y, button);
                }
            }
            Raw::MouseUp { x, y, button, .. } => self.mouse_up(x, y, button, &mut out),
            Raw::Key {
                t,
                vk,
                ctrl,
                alt,
                shift,
            } => {
                if !self.paused() {
                    self.key(t, vk, ctrl, alt, shift, &mut out);
                }
            }
            Raw::Pause { t } => {
                if !self.paused_manual {
                    self.flush_typing(&mut out);
                    self.paused_manual = true;
                    self.pending = None;
                    self.ignored_button = None;
                    if !self.paused_auto {
                        out.push(Event::Pause {
                            t,
                            reason: PauseReason::Manual,
                        });
                    }
                }
            }
            Raw::Resume { t } => {
                if self.paused_manual {
                    self.paused_manual = false;
                    if !self.paused_auto {
                        out.push(Event::Resume { t });
                    }
                    self.fg = None; // reanuncia a janela atual
                    self.check_foreground(t, &mut out);
                }
            }
            Raw::Marker { t } => out.push(Event::Marker { t }),
            Raw::AudioLost { t } => out.push(Event::AudioLost { t }),
            Raw::Stop { t } => {
                self.flush_typing(&mut out);
                out.push(Event::SessionEnd { t });
            }
        }
        out
    }

    fn paused(&self) -> bool {
        self.paused_manual || self.paused_auto
    }

    fn check_foreground(&mut self, t: u64, out: &mut Vec<Event>) {
        if self.paused_manual {
            return;
        }
        let Some(fg) = self.probe.foreground() else {
            return;
        };
        if fg.pid == self.own_pid || self.fg.as_ref() == Some(&fg) {
            return;
        }
        self.flush_typing(out);
        let deny = denied(&fg.app, &fg.title, &self.cfg);
        if deny && !self.paused_auto {
            self.paused_auto = true;
            self.pending = None;
            self.ignored_button = None;
            out.push(Event::Pause {
                t,
                reason: PauseReason::Auto,
            });
        } else if !deny && self.paused_auto {
            self.paused_auto = false;
            out.push(Event::Resume { t });
        }
        if !deny {
            let url = if BROWSERS.contains(&fg.app.as_str()) {
                self.probe.url().as_deref().and_then(sanitize_url)
            } else {
                None
            };
            out.push(Event::Window {
                t,
                app: fg.app.clone(),
                title: fg.title.clone(),
                url,
            });
        }
        self.fg = Some(fg);
    }

    fn mouse_down(&mut self, t: u64, x: i32, y: i32, button: MouseButton) {
        let probed = self.probe.element_at(x, y);
        if probed.as_ref().is_some_and(|p| {
            p.pid == self.own_pid || SHELL_CLASSES.contains(&p.root_class.as_str())
        }) {
            self.ignored_button = Some(button);
            return;
        }
        // O hook do mouse dispara antes de o foco mudar: confere a janela sob o clique.
        if probed
            .as_ref()
            .is_some_and(|p| denied(&p.app, &p.title, &self.cfg))
        {
            self.ignored_button = Some(button);
            return;
        }
        let el = probed.map(|p| p.el);
        let blackout = el.as_ref().filter(|e| e.is_password).and_then(|e| e.rect);
        let shot = self.probe.screenshot(t, x, y, blackout);
        self.pending = Some(Pending {
            t,
            x,
            y,
            button,
            shot,
            el,
        });
    }

    fn mouse_up(&mut self, x: i32, y: i32, button: MouseButton, out: &mut Vec<Event>) {
        if self.ignored_button == Some(button) {
            self.ignored_button = None;
            return;
        }
        let Some(p) = self.pending.take_if(|p| p.button == button) else {
            return;
        };
        let (shot, dhash, monitor) = match p.shot {
            Some(s) => (Some(s.path), Some(s.dhash), s.monitor),
            None => (None, None, NO_MONITOR),
        };
        out.push(Event::Click {
            t: p.t,
            button,
            x: p.x,
            y: p.y,
            up_x: x,
            up_y: y,
            shot,
            dhash,
            el: p.el,
            monitor,
        });
    }

    fn key(&mut self, t: u64, vk: u32, ctrl: bool, alt: bool, shift: bool, out: &mut Vec<Event>) {
        if ctrl && alt && OWN_HOTKEY_VKS.contains(&vk) {
            return;
        }
        match classify_key(vk, ctrl, alt, shift) {
            KeyKind::Ignore => {}
            KeyKind::Typing => {
                if self.typing.is_none() {
                    // Início de uma digitação: o Alt+Tab pode ter levado a uma janela da denylist antes do próximo Tick.
                    self.check_foreground(t, out);
                    if self.paused() {
                        return;
                    }
                    let focused = self.probe.focused();
                    let password = focused.as_ref().is_some_and(|p| p.el.is_password);
                    self.typing = Some(Typing {
                        t,
                        el: focused.map(|p| p.el),
                        chars: 0,
                        password,
                    });
                }
                if let Some(typing) = &mut self.typing {
                    typing.chars += 1;
                }
            }
            KeyKind::Combo(combo) => {
                self.flush_typing(out);
                out.push(Event::Key { t, combo });
            }
        }
    }

    fn flush_typing(&mut self, out: &mut Vec<Event>) {
        if let Some(ty) = self.typing.take().filter(|ty| ty.chars > 0) {
            out.push(Event::Type {
                t: ty.t,
                el: ty.el,
                chars: ty.chars,
                password: ty.password,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenmanual_core::domain::Quality;

    const MON: Rect = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };
    const OWN: u32 = 99;

    #[derive(Default)]
    struct FakeProbe {
        fg: Option<Foreground>,
        at: Option<Probed>,
        focused: Option<Probed>,
        url: Option<String>,
        url_calls: u32,
        shots: Vec<(u64, Option<Rect>)>,
    }

    impl Probe for FakeProbe {
        fn foreground(&mut self) -> Option<Foreground> {
            self.fg.clone()
        }
        fn element_at(&mut self, _x: i32, _y: i32) -> Option<Probed> {
            self.at.clone()
        }
        fn focused(&mut self) -> Option<Probed> {
            self.focused.clone()
        }
        fn screenshot(&mut self, t: u64, _x: i32, _y: i32, blackout: Option<Rect>) -> Option<Shot> {
            self.shots.push((t, blackout));
            Some(Shot {
                path: format!("shots/{t:08}.png"),
                dhash: 7,
                monitor: MON,
            })
        }
        fn url(&mut self) -> Option<String> {
            self.url_calls += 1;
            self.url.clone()
        }
    }

    fn fg(app: &str, title: &str, pid: u32) -> Option<Foreground> {
        Some(Foreground {
            app: app.into(),
            title: title.into(),
            pid,
        })
    }

    fn probed(name: &str, pid: u32, root_class: &str, password: bool) -> Option<Probed> {
        let rect = Some(Rect {
            left: 100,
            top: 100,
            right: 300,
            bottom: 130,
        });
        let el = Element {
            name: name.into(),
            role: "Edit".into(),
            automation_id: String::new(),
            class_name: String::new(),
            rect,
            is_password: password,
            ancestors: vec![],
            quality: Quality::Uia,
        };
        Some(Probed {
            el,
            pid,
            root_class: root_class.into(),
            app: "erp.exe".into(),
            title: "ERP".into(),
        })
    }

    fn agg() -> Aggregator<FakeProbe> {
        let probe = FakeProbe {
            fg: fg("erp.exe", "ERP", 10),
            at: probed("CNPJ", 10, "ErpMain", false),
            focused: probed("CNPJ", 10, "", false),
            ..Default::default()
        };
        Aggregator::new(probe, CaptureConfig::default(), OWN)
    }

    fn click(a: &mut Aggregator<FakeProbe>, t: u64) -> Vec<Event> {
        let mut out = a.feed(Raw::MouseDown {
            t,
            x: 200,
            y: 115,
            button: MouseButton::Left,
        });
        out.extend(a.feed(Raw::MouseUp {
            t: t + 80,
            x: 205,
            y: 115,
            button: MouseButton::Left,
        }));
        out
    }

    fn key(a: &mut Aggregator<FakeProbe>, t: u64, vk: u32, ctrl: bool, alt: bool) -> Vec<Event> {
        a.feed(Raw::Key {
            t,
            vk,
            ctrl,
            alt,
            shift: false,
        })
    }

    #[test]
    fn start_emits_session_start_and_window() {
        let mut a = agg();
        assert_eq!(
            a.start(),
            vec![
                Event::SessionStart { t: 0 },
                Event::Window {
                    t: 0,
                    app: "erp.exe".into(),
                    title: "ERP".into(),
                    url: None
                }
            ]
        );
    }

    #[test]
    fn click_becomes_event_with_shot_and_up_position() {
        let mut a = agg();
        a.start();
        let out = click(&mut a, 1000);
        let Event::Click {
            t,
            x,
            up_x,
            shot,
            dhash,
            el,
            monitor,
            ..
        } = &out[0]
        else {
            panic!("{out:?}")
        };
        assert_eq!((*t, *x, *up_x), (1000, 200, 205));
        assert_eq!(shot.as_deref(), Some("shots/00001000.png"));
        assert_eq!((*dhash, *monitor), (Some(7), MON));
        assert_eq!(el.as_ref().unwrap().name, "CNPJ");
        assert_eq!(a.probe_mut().shots, vec![(1000, None)]);
    }

    #[test]
    fn password_field_is_blacked_out() {
        let mut a = agg();
        a.probe_mut().at = probed("Senha", 10, "ErpMain", true);
        click(&mut a, 1000);
        assert_eq!(
            a.probe_mut().shots,
            vec![(
                1000,
                Some(Rect {
                    left: 100,
                    top: 100,
                    right: 300,
                    bottom: 130
                })
            )]
        );
    }

    #[test]
    fn own_window_and_taskbar_clicks_are_ignored() {
        let mut a = agg();
        a.start();
        a.probe_mut().at = probed("Parar", OWN, "Tauri", false);
        assert!(click(&mut a, 1000).is_empty());
        a.probe_mut().at = probed("Chrome", 5, "Shell_TrayWnd", false);
        assert!(click(&mut a, 2000).is_empty());
        assert!(a.probe_mut().shots.is_empty());
    }

    #[test]
    fn typing_is_counted_and_flushed_before_keys_and_clicks() {
        let mut a = agg();
        a.start();
        assert!(key(&mut a, 100, 0x41, false, false).is_empty());
        key(&mut a, 110, 0x42, false, false);
        key(&mut a, 120, 0x51, true, true); // AltGr+Q conta como caractere
        let out = key(&mut a, 130, 0x0D, false, false);
        let focused = probed("CNPJ", 10, "", false).unwrap().el;
        assert_eq!(
            out,
            vec![
                Event::Type {
                    t: 100,
                    el: Some(focused.clone()),
                    chars: 3,
                    password: false
                },
                Event::Key {
                    t: 130,
                    combo: "Enter".into()
                }
            ]
        );
        key(&mut a, 200, 0x43, false, false);
        let out = click(&mut a, 300);
        assert_eq!(
            out[0],
            Event::Type {
                t: 200,
                el: Some(focused),
                chars: 1,
                password: false
            }
        );
        assert!(matches!(out[1], Event::Click { .. }));
    }

    #[test]
    fn shortcuts_are_logged_but_app_hotkeys_are_not() {
        let mut a = agg();
        assert_eq!(
            key(&mut a, 100, 0x53, true, false),
            vec![Event::Key {
                t: 100,
                combo: "Ctrl+S".into()
            }]
        );
        assert!(key(&mut a, 200, 0x50, true, true).is_empty()); // Ctrl+Alt+P
        assert!(key(&mut a, 300, 0x4D, true, true).is_empty()); // Ctrl+Alt+M
    }

    #[test]
    fn denylisted_window_auto_pauses_without_logging_its_title() {
        let mut a = agg();
        a.start();
        a.probe_mut().fg = fg("keepass.exe", "Cofre", 20);
        assert_eq!(
            a.feed(Raw::Tick { t: 500 }),
            vec![Event::Pause {
                t: 500,
                reason: PauseReason::Auto
            }]
        );
        assert!(click(&mut a, 600).is_empty());
        assert!(key(&mut a, 700, 0x41, false, false).is_empty());
        a.probe_mut().fg = fg("erp.exe", "ERP", 10);
        assert_eq!(
            a.feed(Raw::Tick { t: 900 }),
            vec![
                Event::Resume { t: 900 },
                Event::Window {
                    t: 900,
                    app: "erp.exe".into(),
                    title: "ERP".into(),
                    url: None
                }
            ]
        );
        assert!(a.probe_mut().shots.is_empty());
    }

    #[test]
    fn typing_right_after_focusing_denied_window_is_not_recorded() {
        let mut a = agg();
        a.start();
        a.probe_mut().fg = fg("keepass.exe", "Cofre", 20);
        assert_eq!(
            key(&mut a, 100, 0x41, false, false),
            vec![Event::Pause {
                t: 100,
                reason: PauseReason::Auto
            }]
        );
        assert_eq!(
            a.feed(Raw::Stop { t: 200 }),
            vec![Event::SessionEnd { t: 200 }]
        );
    }

    #[test]
    fn first_click_on_denied_window_still_in_background_is_not_recorded() {
        let mut a = agg();
        a.start();
        let mut p = probed("Entrada", 20, "KPWnd", false).unwrap();
        p.app = "keepass.exe".into();
        p.title = "Cofre".into();
        a.probe_mut().at = Some(p);
        assert!(click(&mut a, 1000).is_empty());
        assert!(a.probe_mut().shots.is_empty());
    }

    #[test]
    fn pause_between_down_and_up_drops_the_click() {
        let mut a = agg();
        a.start();
        assert!(a
            .feed(Raw::MouseDown {
                t: 100,
                x: 200,
                y: 115,
                button: MouseButton::Left
            })
            .is_empty());
        assert_eq!(
            a.feed(Raw::Pause { t: 150 }),
            vec![Event::Pause {
                t: 150,
                reason: PauseReason::Manual
            }]
        );
        assert!(a
            .feed(Raw::MouseUp {
                t: 180,
                x: 205,
                y: 115,
                button: MouseButton::Left
            })
            .is_empty());
    }

    #[test]
    fn overlapping_manual_and_auto_pause() {
        let mut a = agg();
        a.start();
        a.probe_mut().fg = fg("keepass.exe", "Cofre", 20);
        assert_eq!(
            a.feed(Raw::Tick { t: 100 }),
            vec![Event::Pause {
                t: 100,
                reason: PauseReason::Auto
            }]
        );
        assert!(a.feed(Raw::Pause { t: 200 }).is_empty());
        assert!(a.feed(Raw::Resume { t: 300 }).is_empty());
        a.probe_mut().fg = fg("erp.exe", "ERP", 10);
        assert_eq!(
            a.feed(Raw::Tick { t: 400 }),
            vec![
                Event::Resume { t: 400 },
                Event::Window {
                    t: 400,
                    app: "erp.exe".into(),
                    title: "ERP".into(),
                    url: None
                }
            ]
        );
    }

    #[test]
    fn manual_pause_ignores_input_and_resume_reannounces_window() {
        let mut a = agg();
        a.start();
        assert_eq!(
            a.feed(Raw::Pause { t: 100 }),
            vec![Event::Pause {
                t: 100,
                reason: PauseReason::Manual
            }]
        );
        assert!(click(&mut a, 200).is_empty());
        a.probe_mut().fg = fg("chrome.exe", "Banco XPTO", 30);
        assert!(
            a.feed(Raw::Tick { t: 300 }).is_empty(),
            "janela durante pausa não é registrada"
        );
        assert_eq!(
            a.feed(Raw::Resume { t: 400 }),
            vec![
                Event::Resume { t: 400 },
                Event::Window {
                    t: 400,
                    app: "chrome.exe".into(),
                    title: "Banco XPTO".into(),
                    url: None
                }
            ]
        );
    }

    #[test]
    fn browser_window_carries_sanitized_url_read_once_per_change() {
        let mut a = agg();
        a.start();
        assert_eq!(a.probe_mut().url_calls, 0, "erp.exe não é navegador");
        a.probe_mut().fg = fg("chrome.exe", "NFS-e - Emitir", 30);
        a.probe_mut().url = Some("nfse.prefeitura.sp.gov.br/emitir?cnpj=123".into());
        assert_eq!(
            a.feed(Raw::Tick { t: 500 }),
            vec![Event::Window {
                t: 500,
                app: "chrome.exe".into(),
                title: "NFS-e - Emitir".into(),
                url: Some("https://nfse.prefeitura.sp.gov.br/emitir".into())
            }]
        );
        a.feed(Raw::Tick { t: 750 });
        assert_eq!(a.probe_mut().url_calls, 1, "mesma janela: não relê a URL");
    }

    #[test]
    fn foreground_change_before_click_is_announced_first() {
        let mut a = agg();
        a.start();
        a.probe_mut().fg = fg("chrome.exe", "Portal", 30);
        let out = click(&mut a, 1000);
        assert!(matches!(&out[0], Event::Window { title, .. } if title == "Portal"));
        assert!(matches!(out[1], Event::Click { .. }));
    }

    #[test]
    fn stop_flushes_typing_and_ends_session() {
        let mut a = agg();
        key(&mut a, 100, 0x41, false, false);
        let out = a.feed(Raw::Stop { t: 500 });
        assert!(matches!(out[0], Event::Type { chars: 1, .. }));
        assert_eq!(out[1], Event::SessionEnd { t: 500 });
        assert_eq!(
            a.feed(Raw::AudioLost { t: 600 }),
            vec![Event::AudioLost { t: 600 }]
        );
        assert_eq!(
            a.feed(Raw::Marker { t: 700 }),
            vec![Event::Marker { t: 700 }]
        );
    }
}
