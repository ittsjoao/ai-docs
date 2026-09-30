use std::sync::mpsc::Sender;
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::Instant;

use anyhow::bail;
use screenmanual_core::domain::MouseButton;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_MENU, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, WH_KEYBOARD_LL,
    WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_QUIT,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_USER,
};

use crate::aggregate::Raw;

/// Destino dos callbacks (que não recebem contexto). Só existe durante a gravação.
static TARGET: Mutex<Option<(Sender<Raw>, Instant)>> = Mutex::new(None);

fn send(make: impl FnOnce(u64) -> Raw) {
    if let Ok(guard) = TARGET.lock() {
        if let Some((tx, t0)) = guard.as_ref() {
            let _ = tx.send(make(t0.elapsed().as_millis() as u64));
        }
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let (x, y) = (info.pt.x, info.pt.y);
        let action = match wparam.0 as u32 {
            WM_LBUTTONDOWN => Some((true, MouseButton::Left)),
            WM_LBUTTONUP => Some((false, MouseButton::Left)),
            WM_RBUTTONDOWN => Some((true, MouseButton::Right)),
            WM_RBUTTONUP => Some((false, MouseButton::Right)),
            WM_MBUTTONDOWN => Some((true, MouseButton::Middle)),
            WM_MBUTTONUP => Some((false, MouseButton::Middle)),
            _ => None,
        };
        if let Some((down, button)) = action {
            send(|t| {
                if down {
                    Raw::MouseDown { t, x, y, button }
                } else {
                    Raw::MouseUp { t, x, y, button }
                }
            });
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN) {
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let held = |vk: VIRTUAL_KEY| unsafe { GetAsyncKeyState(vk.0 as i32) } < 0;
        let (vk, ctrl, alt, shift) = (info.vkCode, held(VK_CONTROL), held(VK_MENU), held(VK_SHIFT));
        send(|t| Raw::Key {
            t,
            vk,
            ctrl,
            alt,
            shift,
        });
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

pub(crate) struct Hooks {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

/// Instala os hooks numa thread com loop de mensagens. Os callbacks só enviam `Raw` e retornam.
pub(crate) fn start(tx: Sender<Raw>, t0: Instant) -> anyhow::Result<Hooks> {
    *TARGET.lock().unwrap_or_else(|e| e.into_inner()) = Some((tx, t0));
    let (id_tx, id_rx) = std::sync::mpsc::channel::<Option<u32>>();
    let thread = std::thread::spawn(move || unsafe {
        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0);
        let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0);
        let ok = mouse.is_ok() && keyboard.is_ok();
        let mut msg = MSG::default();
        // Força a criação da fila de mensagens antes de publicar o id, para o WM_QUIT não se perder.
        let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
        let _ = id_tx.send(ok.then(|| GetCurrentThreadId()));
        if ok {
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
        }
        if let Ok(h) = mouse {
            let _ = UnhookWindowsHookEx(h);
        }
        if let Ok(h) = keyboard {
            let _ = UnhookWindowsHookEx(h);
        }
    });
    match id_rx.recv()? {
        Some(thread_id) => Ok(Hooks {
            thread_id,
            thread: Some(thread),
        }),
        None => {
            *TARGET.lock().unwrap_or_else(|e| e.into_inner()) = None;
            let _ = thread.join();
            bail!("não foi possível instalar os hooks de mouse e teclado")
        }
    }
}

impl Hooks {
    pub(crate) fn stop(mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        *TARGET.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}
