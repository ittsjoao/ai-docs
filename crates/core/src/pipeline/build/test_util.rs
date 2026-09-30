#![allow(dead_code)]

use crate::domain::*;

pub fn mon() -> Rect {
    Rect { left: 0, top: 0, right: 1920, bottom: 1080 }
}

pub fn el(name: &str, role: &str, quality: Quality, rect: Option<Rect>) -> Element {
    Element { name: name.into(), role: role.into(), automation_id: String::new(), class_name: String::new(), rect, is_password: false, ancestors: vec![], quality }
}

pub fn field(name: &str) -> Element {
    el(name, "Edit", Quality::Uia, Some(Rect { left: 100, top: 100, right: 300, bottom: 130 }))
}

pub fn click(t: u64, x: i32, y: i32, e: Option<Element>) -> Event {
    Event::Click { t, button: MouseButton::Left, x, y, up_x: x, up_y: y, shot: Some(format!("shots/{t:08}.png")), dhash: None, el: e, monitor: mon() }
}

pub fn click_hash(t: u64, e: Option<Element>, dhash: u64) -> Event {
    match click(t, 500, 500, e) {
        Event::Click { t, button, x, y, up_x, up_y, shot, el, monitor, .. } => Event::Click { t, button, x, y, up_x, up_y, shot, dhash: Some(dhash), el, monitor },
        _ => unreachable!(),
    }
}

pub fn typ(t: u64, e: Element, chars: u32) -> Event {
    Event::Type { t, el: Some(e), chars, password: false }
}

pub fn key(t: u64, combo: &str) -> Event {
    Event::Key { t, combo: combo.into() }
}

pub fn win(t: u64, title: &str) -> Event {
    Event::Window { t, app: "erp.exe".into(), title: title.into(), url: None }
}

pub fn seg(start: u64, end: u64, text: &str) -> Segment {
    Segment { start, end, text: text.into(), words: vec![] }
}
