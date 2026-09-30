use screenmanual_core::domain::{Quality, Rect};

/// Classes de janela cujo conteúdo o UIA não enxerga (RemoteApp RDP, Java AWT/Swing, JavaFX).
const PIXEL_ONLY_CLASSES: &[&str] = &["RAIL_WINDOW", "SunAwt", "GlassWndClass"];
const CONTAINER_ROLES: &[&str] = &["Pane", "Custom", "Window", "Document", "Group"];

pub(crate) fn classify(
    name: &str,
    role: &str,
    class_name: &str,
    root_class: &str,
    rect: Option<Rect>,
    window: Option<Rect>,
) -> Quality {
    if PIXEL_ONLY_CLASSES
        .iter()
        .any(|c| class_name.starts_with(c) || root_class.starts_with(c))
    {
        return Quality::None;
    }
    let unnamed = name.trim().is_empty();
    if unnamed && role.is_empty() {
        return Quality::None;
    }
    if unnamed && CONTAINER_ROLES.contains(&role) {
        return Quality::Generic;
    }
    let area = |r: Rect| r.width() as i64 * r.height() as i64;
    if let (Some(r), Some(w)) = (rect, window) {
        if area(w) > 0 && area(r) * 2 > area(w) {
            return Quality::Generic;
        }
    }
    Quality::Uia
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN: Option<Rect> = Some(Rect {
        left: 0,
        top: 0,
        right: 1366,
        bottom: 728,
    });

    fn r(l: i32, t: i32, rr: i32, b: i32) -> Option<Rect> {
        Some(Rect {
            left: l,
            top: t,
            right: rr,
            bottom: b,
        })
    }

    #[test]
    fn named_controls_are_uia() {
        assert_eq!(
            classify(
                "COMPRAR",
                "Button",
                "vtex-button",
                "Chrome_WidgetWin_1",
                r(803, 483, 1291, 540),
                WIN
            ),
            Quality::Uia
        );
        assert_eq!(
            classify("dominio", "ListItem", "", "Progman", None, None),
            Quality::Uia
        );
        assert_eq!(
            classify(
                "",
                "Button",
                "",
                "Chrome_WidgetWin_1",
                r(10, 10, 40, 40),
                WIN
            ),
            Quality::Uia
        );
    }

    #[test]
    fn remoteapp_and_java_are_pixel_only() {
        assert_eq!(
            classify(
                "Backup (Work Resources)",
                "Pane",
                "RAIL_WINDOW",
                "RAIL_WINDOW",
                r(499, 312, 873, 466),
                WIN
            ),
            Quality::None
        );
        assert_eq!(
            classify(
                "ZAP",
                "Window",
                "SunAwtDialog",
                "SunAwtDialog",
                r(431, 282, 950, 519),
                WIN
            ),
            Quality::None
        );
        assert_eq!(
            classify(
                "x",
                "Button",
                "",
                "GlassWndClass-GlassWindowClass-2",
                None,
                None
            ),
            Quality::None
        );
    }

    #[test]
    fn unnamed_containers_and_huge_rects_are_generic() {
        assert_eq!(
            classify(
                "",
                "Pane",
                "Windows.UI.Input.InputSite.WindowClass",
                "CASCADIA_HOSTING_WINDOW_CLASS",
                None,
                WIN
            ),
            Quality::Generic
        );
        assert_eq!(
            classify(
                "Google",
                "Document",
                "",
                "Chrome_WidgetWin_1",
                r(0, 0, 1366, 728),
                WIN
            ),
            Quality::Generic
        );
    }

    #[test]
    fn nothing_known_is_none() {
        assert_eq!(classify("", "", "", "", None, None), Quality::None);
    }
}
