use crate::domain::{Event, Segment, SessionMeta};

/// Frases que o whisper inventa em silêncio (comparação em minúsculas, após normalizar).
pub const PHANTOMS: &[&str] = &[
    "legendas pela comunidade amara.org",
    "obrigado por assistir",
    "inscreva-se no canal",
    "legenda adriana zanotto",
];

const VOCABULARY_MAX_CHARS: usize = 800; // ~200 tokens do initial_prompt

pub fn align(segments: Vec<Segment>, offset_ms: i64) -> Vec<Segment> {
    let shift = |v: u64| (v as i64 + offset_ms).max(0) as u64;
    segments
        .into_iter()
        .map(|mut s| {
            s.start = shift(s.start);
            s.end = shift(s.end);
            for w in &mut s.words {
                w.s = shift(w.s);
                w.e = shift(w.e);
            }
            s
        })
        .collect()
}

pub fn pause_ranges(events: &[Event]) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut open: Option<u64> = None;
    for e in events {
        match e {
            Event::Pause { t, .. } => {
                open.get_or_insert(*t);
            }
            Event::Resume { t } => {
                if let Some(start) = open.take() {
                    out.push((start, *t));
                }
            }
            _ => {}
        }
    }
    if let Some(start) = open {
        out.push((start, u64::MAX));
    }
    out
}

fn normalize(s: &str) -> String {
    let kept: String = s
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '.' || *c == '-')
        .collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn filter_hallucinations(segments: Vec<Segment>, pauses: &[(u64, u64)]) -> Vec<Segment> {
    segments
        .into_iter()
        .filter(|s| {
            let mid = s.start / 2 + s.end / 2;
            let paused = pauses.iter().any(|(a, b)| mid >= *a && mid < *b);
            let text = normalize(&s.text);
            let phantom = PHANTOMS.iter().any(|p| text.contains(p));
            !paused && !phantom && !text.is_empty()
        })
        .collect()
}

pub fn vocabulary(meta: &SessionMeta, events: &[Event]) -> String {
    let mut terms: Vec<String> = Vec::new();
    let mut add = |s: &str| {
        let s = s.trim();
        if !s.is_empty() && !terms.iter().any(|t| t == s) {
            terms.push(s.to_string());
        }
    };
    add(&meta.title);
    for e in events {
        match e {
            Event::Window { title, .. } => add(title),
            Event::Click { el: Some(el), .. } | Event::Type { el: Some(el), .. } => add(&el.name),
            _ => {}
        }
    }
    terms.join(", ").chars().take(VOCABULARY_MAX_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::*;

    fn seg(start: u64, end: u64, text: &str) -> Segment {
        Segment { start, end, text: text.into(), words: vec![Word { w: "x".into(), s: start, e: end }] }
    }

    #[test]
    fn align_shifts_and_clamps_at_zero() {
        let out = align(vec![seg(1000, 2000, "a"), seg(100, 300, "b")], -200);
        assert_eq!((out[0].start, out[0].end, out[0].words[0].s), (800, 1800, 800));
        let out = align(vec![seg(100, 300, "b")], -500);
        assert_eq!((out[0].start, out[0].end), (0, 0));
    }

    #[test]
    fn pause_ranges_close_on_resume_or_stay_open() {
        let ev = vec![
            Event::Pause { t: 100, reason: PauseReason::Auto },
            Event::Resume { t: 500 },
            Event::Pause { t: 900, reason: PauseReason::Manual },
        ];
        assert_eq!(pause_ranges(&ev), vec![(100, 500), (900, u64::MAX)]);
    }

    #[test]
    fn drops_phantoms_paused_and_empty_segments() {
        let segs = vec![
            seg(0, 1000, "clico em nova nota"),
            seg(1000, 2000, "Legendas pela comunidade Amara.org"),
            seg(2000, 3000, "fala durante pausa"),
            seg(3000, 3500, "  "),
        ];
        let kept = filter_hallucinations(segs, &[(1800, 3200)]);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].text, "clico em nova nota");
    }

    #[test]
    fn vocabulary_dedupes_and_truncates() {
        let meta = SessionMeta { schema_version: 1, id: "s".into(), title: "Emitir NFS-e".into(), started_at: String::new(), audio_offset_ms: None, duration_ms: None };
        let el = Element { name: "CNPJ".into(), role: "Edit".into(), automation_id: String::new(), class_name: String::new(), rect: None, is_password: false, ancestors: vec![], quality: Quality::Uia };
        let ev = vec![
            Event::Window { t: 0, app: "erp.exe".into(), title: "ERP".into(), url: None },
            Event::Window { t: 5, app: "erp.exe".into(), title: "ERP".into(), url: None },
            Event::Type { t: 9, el: Some(el), chars: 14, password: false },
        ];
        assert_eq!(vocabulary(&meta, &ev), "Emitir NFS-e, ERP, CNPJ");
        let long = SessionMeta { title: "x".repeat(2000), ..meta };
        assert_eq!(vocabulary(&long, &[]).chars().count(), 800);
    }
}
