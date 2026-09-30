use crate::domain::*;

#[derive(Debug, Clone)]
pub(crate) struct Draft {
    pub cand: Candidate,
    pub shot: Option<String>,
    pub dhash: Option<u64>,
    pub point: (i32, i32),
    pub monitor: Option<Rect>,
    pub first_in_window: bool,
}

pub(crate) fn add_flag(c: &mut Candidate, f: Flag) {
    if !c.flags.contains(&f) {
        c.flags.push(f);
    }
}

fn dist(a: (i32, i32), b: (i32, i32)) -> i32 {
    let (dx, dy) = ((a.0 - b.0) as f64, (a.1 - b.1) as f64);
    (dx * dx + dy * dy).sqrt() as i32
}

struct Ctx {
    app: String,
    window: String,
    url: Option<String>,
}

fn new_draft(t: u64, kind: CandidateKind, ctx: &Ctx, el: Option<Element>) -> Draft {
    Draft {
        cand: Candidate {
            id: String::new(),
            t,
            kind,
            app: ctx.app.clone(),
            window: ctx.window.clone(),
            url: ctx.url.clone(),
            el,
            input: None,
            keys: vec![],
            speech: vec![],
            flags: vec![],
            crop: None,
            context_shot: None,
        },
        shot: None,
        dhash: None,
        point: (0, 0),
        monitor: None,
        first_in_window: false,
    }
}

fn push(drafts: &mut Vec<Draft>, mut d: Draft, after_auto_pause: &mut bool) {
    if *after_auto_pause {
        add_flag(&mut d.cand, Flag::AfterAutoPause);
        *after_auto_pause = false;
    }
    drafts.push(d);
}

pub(crate) fn group(events: &[Event], cfg: &BuildConfig) -> Vec<Draft> {
    let mut drafts: Vec<Draft> = Vec::new();
    let mut ctx = Ctx { app: String::new(), window: String::new(), url: None };
    let mut window_has_shot = false;
    let mut after_auto_pause = false;

    for ev in events {
        match ev {
            Event::Window { t, app, title, url } => {
                ctx = Ctx { app: app.clone(), window: title.clone(), url: url.clone() };
                window_has_shot = false;
                let caused_by_click = drafts.last().is_some_and(|d| {
                    matches!(d.cand.kind, CandidateKind::Click | CandidateKind::DoubleClick)
                        && t.saturating_sub(d.cand.t) <= cfg.switch_after_click_ms
                });
                if !caused_by_click {
                    push(&mut drafts, new_draft(*t, CandidateKind::Switch, &ctx, None), &mut after_auto_pause);
                }
            }
            Event::Click { t, x, y, up_x, up_y, shot, dhash, el, monitor, .. } => {
                if let Some(prev) = drafts.last_mut() {
                    if prev.cand.kind == CandidateKind::Click
                        && t.saturating_sub(prev.cand.t) <= cfg.double_click_ms
                        && dist(prev.point, (*x, *y)) <= cfg.drag_px
                    {
                        prev.cand.kind = CandidateKind::DoubleClick;
                        continue;
                    }
                }
                let kind = if dist((*x, *y), (*up_x, *up_y)) > cfg.drag_px { CandidateKind::Drag } else { CandidateKind::Click };
                let mut d = new_draft(*t, kind, &ctx, el.clone());
                d.shot = shot.clone();
                d.dhash = *dhash;
                d.point = (*x, *y);
                d.monitor = Some(*monitor);
                d.first_in_window = shot.is_some() && !window_has_shot;
                window_has_shot |= d.first_in_window;
                push(&mut drafts, d, &mut after_auto_pause);
            }
            Event::Type { t, el, chars, password } => {
                if let Some(prev) = drafts.last_mut() {
                    let same = matches!((&prev.cand.el, el), (Some(a), Some(b)) if a.same_as(b));
                    let mergeable = matches!(prev.cand.kind, CandidateKind::Click | CandidateKind::DoubleClick | CandidateKind::Fill);
                    if same && mergeable {
                        prev.cand.kind = CandidateKind::Fill;
                        let input = prev.cand.input.get_or_insert(Input { chars: 0, password: false });
                        input.chars += *chars;
                        input.password |= *password;
                        continue;
                    }
                }
                let mut d = new_draft(*t, CandidateKind::Fill, &ctx, el.clone());
                d.cand.input = Some(Input { chars: *chars, password: *password });
                push(&mut drafts, d, &mut after_auto_pause);
            }
            Event::Key { t, combo } => {
                if let Some(prev) = drafts.last_mut() {
                    if prev.cand.kind == CandidateKind::Fill && prev.cand.keys.is_empty() && (combo == "Enter" || combo == "Tab") {
                        prev.cand.keys.push(combo.clone());
                        continue;
                    }
                }
                let mut d = new_draft(*t, CandidateKind::Key, &ctx, None);
                d.cand.keys.push(combo.clone());
                push(&mut drafts, d, &mut after_auto_pause);
            }
            Event::Marker { .. } => {
                if let Some(prev) = drafts.last_mut() {
                    add_flag(&mut prev.cand, Flag::Important);
                }
            }
            Event::Pause { reason: PauseReason::Auto, .. } => after_auto_pause = true,
            _ => {}
        }
    }
    for (i, d) in drafts.iter_mut().enumerate() {
        d.cand.id = format!("c{:03}", i + 1);
    }
    drafts
}

#[cfg(test)]
#[allow(unused_imports)]
mod tests {
    use super::*;
    use crate::pipeline::build::test_util::*;
    use super::*;
    use crate::pipeline::build::test_util::*;

    fn kinds(d: &[Draft]) -> Vec<CandidateKind> {
        d.iter().map(|d| d.cand.kind).collect()
    }

    #[test]
    fn click_type_enter_become_one_fill() {
        let cfg = BuildConfig::default();
        let d = group(&[click(1000, 200, 115, Some(field("CNPJ"))), typ(2000, field("CNPJ"), 14), key(2100, "Enter")], &cfg);
        assert_eq!(kinds(&d), vec![CandidateKind::Fill]);
        assert_eq!(d[0].cand.input, Some(Input { chars: 14, password: false }));
        assert_eq!(d[0].cand.keys, vec!["Enter".to_string()]);
        assert!(d[0].shot.is_some(), "fill herda o print do clique");
    }

    #[test]
    fn typing_in_another_field_after_tab_is_new_fill() {
        let cfg = BuildConfig::default();
        let d = group(&[click(1000, 200, 115, Some(field("CNPJ"))), typ(2000, field("CNPJ"), 14), key(2100, "Tab"), typ(3000, field("Nome"), 20)], &cfg);
        assert_eq!(kinds(&d), vec![CandidateKind::Fill, CandidateKind::Fill]);
        assert_eq!(d[0].cand.keys, vec!["Tab".to_string()]);
        assert!(d[1].shot.is_none());
    }

    #[test]
    fn double_click_and_separate_clicks() {
        let cfg = BuildConfig::default();
        let d = group(&[click(1000, 500, 500, None), click(1300, 502, 501, None)], &cfg);
        assert_eq!(kinds(&d), vec![CandidateKind::DoubleClick]);
        let d = group(&[click(1000, 500, 500, None), click(1700, 500, 500, None)], &cfg);
        assert_eq!(kinds(&d), vec![CandidateKind::Click, CandidateKind::Click]);
    }

    #[test]
    fn far_mouse_up_is_drag() {
        let cfg = BuildConfig::default();
        let ev = match click(1000, 100, 100, None) {
            Event::Click { t, button, x, y, shot, dhash, el, monitor, .. } => Event::Click { t, button, x, y, up_x: 150, up_y: 100, shot, dhash, el, monitor },
            _ => unreachable!(),
        };
        assert_eq!(kinds(&group(&[ev], &cfg)), vec![CandidateKind::Drag]);
    }

    #[test]
    fn window_switch_only_when_not_caused_by_click() {
        let cfg = BuildConfig::default();
        let d = group(&[click(1000, 1, 1, None), win(3000, "Outra")], &cfg);
        assert_eq!(kinds(&d), vec![CandidateKind::Click, CandidateKind::Switch]);
        let d = group(&[click(1000, 1, 1, None), win(1200, "Outra")], &cfg);
        assert_eq!(kinds(&d), vec![CandidateKind::Click]);
        assert_eq!(d[0].cand.window, "", "a janela vale para os candidatos seguintes");
    }

    #[test]
    fn marker_and_auto_pause_flags() {
        let cfg = BuildConfig::default();
        let d = group(
            &[
                click(1000, 1, 1, None),
                Event::Marker { t: 1500 },
                Event::Pause { t: 2000, reason: PauseReason::Auto },
                Event::Resume { t: 5000 },
                click(6000, 900, 900, None),
            ],
            &cfg,
        );
        assert_eq!(d[0].cand.flags, vec![Flag::Important]);
        assert_eq!(d[1].cand.flags, vec![Flag::AfterAutoPause]);
    }

    #[test]
    fn ids_windows_and_first_in_window() {
        let cfg = BuildConfig::default();
        let d = group(&[win(0, "ERP"), click(1000, 1, 1, None), click(3000, 900, 900, None)], &cfg);
        let ids: Vec<&str> = d.iter().map(|d| d.cand.id.as_str()).collect();
        assert_eq!(ids, vec!["c001", "c002", "c003"]);
        assert_eq!(d[1].cand.window, "ERP");
        assert_eq!(d[1].cand.app, "erp.exe");
        assert!(d[1].first_in_window);
        assert!(!d[2].first_in_window);
    }
}
