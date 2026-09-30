use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use std::collections::VecDeque;

use screenmanual_core::domain::{Element, Rect};
use uiautomation::patterns::UIValuePattern;
use uiautomation::types::{ControlType, Handle, Point};
use uiautomation::{UIAutomation, UIElement, UITreeWalker};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND, POINT, RECT};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, WindowFromPoint, GA_ROOT,
};

use crate::aggregate::{Foreground, Probe, Probed, Shot};
use crate::dhash::dhash;
use crate::privacy::blackout;
use crate::quality::classify;
use crate::sink::PngJob;

const MAX_ANCESTORS: usize = 3;
const MAX_WALK: usize = 8;
/// Limites da busca pela barra de endereço (fica na moldura do navegador, perto da raiz).
const URL_MAX_DEPTH: usize = 12;
const URL_MAX_NODES: usize = 600;

pub(crate) struct WinProbe {
    uia: UIAutomation,
    walker: UITreeWalker,
    dir: PathBuf,
    png: Sender<PngJob>,
}

impl WinProbe {
    /// Cria o cliente UIA (COM) na thread atual; use o probe só nela.
    pub(crate) fn new(dir: &Path, png: Sender<PngJob>) -> anyhow::Result<WinProbe> {
        let uia = UIAutomation::new()?;
        let walker = uia.get_control_view_walker()?;
        Ok(WinProbe {
            uia,
            walker,
            dir: dir.to_path_buf(),
            png,
        })
    }

    fn describe(&self, e: &UIElement, root: RootInfo) -> Probed {
        let RootInfo {
            class: root_class,
            rect: window,
            pid: root_pid,
            title,
        } = root;
        let name = e.get_name().unwrap_or_default();
        let role = e
            .get_control_type()
            .map(|c| format!("{c:?}"))
            .unwrap_or_default();
        let class_name = e.get_classname().unwrap_or_default();
        let rect = e
            .get_bounding_rectangle()
            .ok()
            .map(|r| Rect {
                left: r.get_left(),
                top: r.get_top(),
                right: r.get_right(),
                bottom: r.get_bottom(),
            })
            .filter(|r| r.width() > 0 && r.height() > 0);
        let mut ancestors = Vec::new();
        let mut current = self.walker.get_parent(e).ok();
        for _ in 0..MAX_WALK {
            let Some(parent) = current else { break };
            if ancestors.len() == MAX_ANCESTORS {
                break;
            }
            let n = parent.get_name().unwrap_or_default();
            if !n.trim().is_empty() {
                ancestors.push(n);
            }
            current = self.walker.get_parent(&parent).ok();
        }
        let quality = classify(&name, &role, &class_name, &root_class, rect, window);
        let el = Element {
            name,
            role,
            automation_id: e.get_automation_id().unwrap_or_default(),
            class_name,
            rect,
            is_password: e.is_password().unwrap_or(false),
            ancestors,
            quality,
        };
        let pid = e.get_process_id().unwrap_or(0);
        let app = root_pid
            .filter(|p| *p != 0)
            .and_then(exe_name)
            .or_else(|| exe_name(pid))
            .unwrap_or_default();
        Probed {
            el: Some(el),
            pid,
            root_class,
            app,
            title,
        }
    }
}

fn utf16(buf: &[u16], len: i32) -> String {
    String::from_utf16_lossy(&buf[..len.clamp(0, buf.len() as i32) as usize])
}

fn window_text(h: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(h, &mut buf) };
    utf16(&buf, n)
}

fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    utf16(&buf, n)
}

#[derive(Default)]
struct RootInfo {
    class: String,
    rect: Option<Rect>,
    pid: Option<u32>,
    title: String,
}

/// Classe, retângulo, pid e título da janela raiz sob o ponto (barra de tarefas, RemoteApp, Java…).
fn root_at(x: i32, y: i32) -> RootInfo {
    let h = unsafe { WindowFromPoint(POINT { x, y }) };
    if h.is_invalid() {
        return RootInfo::default();
    }
    let root = unsafe { GetAncestor(h, GA_ROOT) };
    let mut r = RECT::default();
    let rect = unsafe { GetWindowRect(root, &mut r) }.ok().map(|_| Rect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    });
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(root, Some(&mut pid)) };
    RootInfo {
        class: class_name(root),
        rect,
        pid: Some(pid),
        title: window_text(root),
    }
}

fn exe_name(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(handle);
        if !ok {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        Some(path.rsplit('\\').next().unwrap_or(&path).to_lowercase())
    }
}

impl Probe for WinProbe {
    fn foreground(&mut self) -> Option<Foreground> {
        let h = unsafe { GetForegroundWindow() };
        if h.is_invalid() {
            return None;
        }
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
        Some(Foreground {
            app: exe_name(pid).unwrap_or_default(),
            title: window_text(h),
            pid,
        })
    }

    fn element_at(&mut self, x: i32, y: i32) -> Option<Probed> {
        let root = root_at(x, y);
        match self.uia.element_from_point(Point::new(x, y)) {
            Ok(e) => Some(self.describe(&e, root)),
            // Sem UIA, a janela raiz (Win32) ainda basta para denylist e classes do shell.
            Err(_) => {
                let pid = root.pid?;
                Some(Probed {
                    el: None,
                    pid,
                    root_class: root.class,
                    app: exe_name(pid).unwrap_or_default(),
                    title: root.title,
                })
            }
        }
    }

    fn focused(&mut self) -> Option<Probed> {
        let e = self.uia.get_focused_element().ok()?;
        Some(self.describe(&e, RootInfo::default()))
    }

    fn screenshot(&mut self, t: u64, x: i32, y: i32, black: Option<Rect>) -> Option<Shot> {
        let monitor = xcap::Monitor::from_point(x, y).ok()?;
        let (mx, my) = (monitor.x().ok()?, monitor.y().ok()?);
        let mut img = monitor.capture_image().ok()?;
        if let Some(r) = black {
            blackout(
                &mut img,
                Rect {
                    left: r.left - mx,
                    top: r.top - my,
                    right: r.right - mx,
                    bottom: r.bottom - my,
                },
            );
        }
        let hash = dhash(&img);
        let rel = format!("shots/{t:08}.png");
        let (w, h) = (img.width() as i32, img.height() as i32);
        let _ = self.png.send((self.dir.join(&rel), img));
        Some(Shot {
            path: rel,
            dhash: hash,
            monitor: Rect {
                left: mx,
                top: my,
                right: mx + w,
                bottom: my + h,
            },
        })
    }

    /// Busca em largura pelo primeiro `Edit` com valor, pulando o conteúdo da página (`Document`).
    /// O `uiautomation` usa outra versão do crate `windows`, por isso o HWND vai como `isize`.
    fn url(&mut self) -> Option<String> {
        let h = unsafe { GetForegroundWindow() };
        if h.is_invalid() {
            return None;
        }
        let root = self
            .uia
            .element_from_handle(Handle::from(h.0 as isize))
            .ok()?;
        let mut queue = VecDeque::from([(root, 0usize)]);
        let mut visited = 0;
        while let Some((el, depth)) = queue.pop_front() {
            visited += 1;
            if visited > URL_MAX_NODES {
                break;
            }
            let control_type = el.get_control_type().ok();
            if matches!(
                control_type,
                Some(ControlType::Edit | ControlType::ComboBox)
            ) {
                // Ignora a barra enquanto tem foco porque seu valor é o texto digitado, não a URL.
                if el.has_keyboard_focus().unwrap_or(false) {
                    return None;
                }
                if let Ok(value) = el
                    .get_pattern::<UIValuePattern>()
                    .and_then(|p| p.get_value())
                {
                    if !value.trim().is_empty() {
                        return Some(value);
                    }
                }
            }
            if matches!(control_type, Some(ControlType::Document)) || depth >= URL_MAX_DEPTH {
                continue;
            }
            let mut child = self.walker.get_first_child(&el).ok();
            while let Some(c) = child {
                child = self.walker.get_next_sibling(&c).ok();
                queue.push_back((c, depth + 1));
            }
        }
        None
    }
}
