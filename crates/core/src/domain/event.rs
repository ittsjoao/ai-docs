use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
    pub fn center(&self) -> (i32, i32) {
        ((self.left + self.right) / 2, (self.top + self.bottom) / 2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    Uia,
    Generic,
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub name: String,
    pub role: String,
    #[serde(default)]
    pub automation_id: String,
    #[serde(default)]
    pub class_name: String,
    pub rect: Option<Rect>,
    #[serde(default)]
    pub is_password: bool,
    #[serde(default)]
    pub ancestors: Vec<String>,
    pub quality: Quality,
}

impl Element {
    pub fn same_as(&self, other: &Element) -> bool {
        if !self.automation_id.is_empty() && self.automation_id == other.automation_id {
            return true;
        }
        self.name == other.name && self.role == other.role && self.rect == other.rect
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PauseReason {
    Manual,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    SessionStart {
        t: u64,
    },
    Window {
        t: u64,
        app: String,
        title: String,
        #[serde(default)]
        url: Option<String>,
    },
    Click {
        t: u64,
        button: MouseButton,
        x: i32,
        y: i32,
        up_x: i32,
        up_y: i32,
        shot: Option<String>,
        dhash: Option<u64>,
        el: Option<Element>,
        monitor: Rect,
    },
    Type {
        t: u64,
        el: Option<Element>,
        chars: u32,
        #[serde(default)]
        password: bool,
    },
    Key {
        t: u64,
        combo: String,
    },
    Marker {
        t: u64,
    },
    Pause {
        t: u64,
        reason: PauseReason,
    },
    Resume {
        t: u64,
    },
    AudioLost {
        t: u64,
    },
    SessionEnd {
        t: u64,
    },
}

impl Event {
    pub fn t(&self) -> u64 {
        match self {
            Event::SessionStart { t }
            | Event::Window { t, .. }
            | Event::Click { t, .. }
            | Event::Type { t, .. }
            | Event::Key { t, .. }
            | Event::Marker { t }
            | Event::Pause { t, .. }
            | Event::Resume { t }
            | Event::AudioLost { t }
            | Event::SessionEnd { t } => *t,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_line_roundtrips() {
        let line = r#"{"type":"click","t":5210,"button":"left","x":812,"y":344,"up_x":812,"up_y":344,"shot":"shots/00005210.png","dhash":42,"el":{"name":"Nova nota","role":"Button","automation_id":"btnNova","rect":{"left":760,"top":328,"right":880,"bottom":360},"quality":"uia"},"monitor":{"left":0,"top":0,"right":1920,"bottom":1080}}"#;
        let ev: Event = serde_json::from_str(line).unwrap();
        assert_eq!(ev.t(), 5210);
        let back = serde_json::to_string(&ev).unwrap();
        assert_eq!(serde_json::from_str::<Event>(&back).unwrap(), ev);
    }

    #[test]
    fn window_without_url_parses() {
        let ev: Event =
            serde_json::from_str(r#"{"type":"window","t":10,"app":"erp.exe","title":"ERP"}"#)
                .unwrap();
        assert_eq!(
            ev,
            Event::Window {
                t: 10,
                app: "erp.exe".into(),
                title: "ERP".into(),
                url: None
            }
        );
    }

    #[test]
    fn same_element_by_automation_id_or_name_role_rect() {
        let r = Some(Rect {
            left: 0,
            top: 0,
            right: 10,
            bottom: 10,
        });
        let a = Element {
            name: "CNPJ".into(),
            role: "Edit".into(),
            automation_id: "txtCnpj".into(),
            class_name: String::new(),
            rect: r,
            is_password: false,
            ancestors: vec![],
            quality: Quality::Uia,
        };
        let mut b = a.clone();
        b.name = "outro".into();
        assert!(a.same_as(&b), "mesmo automation_id");
        b.automation_id.clear();
        let mut c = a.clone();
        c.automation_id.clear();
        assert!(!c.same_as(&b), "nome diferente sem automation_id");
        b.name = "CNPJ".into();
        assert!(c.same_as(&b), "mesmo nome, papel e rect");
    }
}
