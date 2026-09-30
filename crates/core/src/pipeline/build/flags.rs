use super::{add_flag, Draft};
use crate::domain::{BuildConfig, CandidateKind, Flag, Quality};

const UNDO_KEYS: &[&str] = &["Esc", "Ctrl+Z", "Alt+F4"];
const UNDO_NAMES: &[&str] = &["cancelar", "voltar", "fechar", "cancel", "back", "close"];

fn is_click(kind: CandidateKind) -> bool {
    matches!(kind, CandidateKind::Click | CandidateKind::DoubleClick)
}

/// O código só marca; quem decide descartar é o Claude.
pub(crate) fn apply_flags(drafts: &mut [Draft], cfg: &BuildConfig) {
    let n = drafts.len();
    for i in 0..n {
        let no_change = is_click(drafts[i].cand.kind)
            && match (drafts[i].dhash, drafts[i + 1..].iter().find_map(|d| d.dhash)) {
                (Some(a), Some(b)) => (a ^ b).count_ones() <= cfg.no_change_hamming,
                _ => false,
            };
        if no_change {
            add_flag(&mut drafts[i].cand, Flag::NoChange);
        }

        let undo_next = i + 1 < n && {
            let next = &drafts[i + 1].cand;
            let undo_key = next.keys.iter().any(|k| UNDO_KEYS.contains(&k.as_str()));
            let undo_click = is_click(next.kind)
                && next.el.as_ref().is_some_and(|e| UNDO_NAMES.contains(&e.name.trim().to_lowercase().as_str()));
            (undo_key || undo_click) && next.t.saturating_sub(drafts[i].cand.t) <= cfg.error_window_ms
        };
        if undo_next {
            add_flag(&mut drafts[i].cand, Flag::PossibleError);
        }

        let c = &drafts[i].cand;
        let generic = c.el.as_ref().is_none_or(|e| e.quality != Quality::Uia);
        if generic && c.flags.contains(&Flag::NoChange) && c.speech.is_empty() {
            add_flag(&mut drafts[i].cand, Flag::Noise);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::*;
    use crate::pipeline::build::group::group;
    use crate::pipeline::build::speech::assign_speech;
    use crate::pipeline::build::test_util::*;
    use super::{apply_flags, Draft};

    fn run(events: &[Event], speech: &[Segment]) -> Vec<Draft> {
        let cfg = BuildConfig::default();
        let mut d = group(events, &cfg);
        assign_speech(&mut d, speech, cfg.lead_ms);
        apply_flags(&mut d, &cfg);
        d
    }

    #[test]
    fn no_change_when_next_screen_is_almost_equal() {
        let d = run(&[click_hash(1000, None, 0b1111), click_hash(3000, None, 0b1110)], &[]);
        assert!(d[0].cand.flags.contains(&Flag::NoChange));
        let d = run(&[click_hash(1000, None, 0), click_hash(3000, None, u64::MAX)], &[]);
        assert!(!d[0].cand.flags.contains(&Flag::NoChange));
    }

    #[test]
    fn esc_or_cancel_right_after_marks_possible_error() {
        let d = run(&[click(1000, 1, 1, None), key(2000, "Esc")], &[]);
        assert!(d[0].cand.flags.contains(&Flag::PossibleError));
        let cancel = el("Cancelar", "Button", Quality::Uia, None);
        let d = run(&[click(1000, 1, 1, None), click(3000, 900, 900, Some(cancel))], &[]);
        assert!(d[0].cand.flags.contains(&Flag::PossibleError));
        let d = run(&[click(1000, 1, 1, None), key(9000, "Esc")], &[]);
        assert!(!d[0].cand.flags.contains(&Flag::PossibleError), "fora da janela de 5 s");
    }

    #[test]
    fn noise_is_generic_without_change_and_without_speech() {
        let grid = el("", "Pane", Quality::Generic, None);
        let d = run(&[click_hash(1000, Some(grid.clone()), 7), click_hash(9000, None, 7)], &[]);
        assert!(d[0].cand.flags.contains(&Flag::Noise));
        let d = run(&[click_hash(1000, Some(grid), 7), click_hash(9000, None, 7)], &[seg(900, 1500, "clico na linha")]);
        assert!(!d[0].cand.flags.contains(&Flag::Noise));
    }
}
