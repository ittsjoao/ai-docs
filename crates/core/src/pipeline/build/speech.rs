use super::Draft;
use crate::domain::Segment;

/// Cada segmento vai ao candidato cuja janela [t_i − lead, t_{i+1} − lead) mais se sobrepõe a ele.
/// A fala anterior ao primeiro candidato vai para o primeiro.
/// ponytail: O(candidatos × segmentos); vira varredura com dois ponteiros se sessões passarem de milhares de passos.
pub(crate) fn assign_speech(drafts: &mut [Draft], segments: &[Segment], lead_ms: u64) {
    if drafts.is_empty() {
        return;
    }
    let starts: Vec<u64> = drafts
        .iter()
        .enumerate()
        .map(|(i, d)| {
            if i == 0 {
                0
            } else {
                d.cand.t.saturating_sub(lead_ms)
            }
        })
        .collect();
    for seg in segments {
        let end = seg.end.max(seg.start + 1);
        let mut best: Option<(usize, u64)> = None;
        for i in 0..drafts.len() {
            let a = starts[i];
            let b = starts.get(i + 1).copied().unwrap_or(u64::MAX);
            let overlap = end.min(b).saturating_sub(seg.start.max(a));
            if overlap > 0 && best.is_none_or(|(_, o)| overlap > o) {
                best = Some((i, overlap));
            }
        }
        if let Some((i, _)) = best {
            drafts[i].cand.speech.push(seg.text.trim().to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::BuildConfig;
    use crate::pipeline::build::group::group;
    use crate::pipeline::build::test_util::*;

    fn drafts() -> Vec<Draft> {
        group(
            &[click(10_000, 1, 1, None), click(20_000, 900, 900, None)],
            &BuildConfig::default(),
        )
    }

    #[test]
    fn speech_goes_to_window_with_most_overlap() {
        let mut d = drafts();
        assign_speech(
            &mut d,
            &[
                seg(1000, 2000, "antes de tudo"),
                seg(8600, 9800, "agora clico"),
                seg(18_000, 19_500, "e depois este"),
            ],
            1500,
        );
        assert_eq!(d[0].cand.speech, vec!["antes de tudo", "agora clico"]);
        assert_eq!(d[1].cand.speech, vec!["e depois este"]);
    }

    #[test]
    fn zero_length_segment_is_kept() {
        let mut d = drafts();
        assign_speech(&mut d, &[seg(25_000, 25_000, "fim")], 1500);
        assert_eq!(d[1].cand.speech, vec!["fim"]);
    }
}
