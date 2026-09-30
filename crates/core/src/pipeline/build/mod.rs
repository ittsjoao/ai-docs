mod group;
#[cfg(test)]
mod test_util;

use crate::domain::{BuildConfig, Candidate, CropSpec, Event, Segment};

// removido na Task 6, quando add_flag passa a ser usado
#[allow(unused_imports)]
pub(crate) use group::{add_flag, Draft};

#[derive(Debug, Clone, PartialEq)]
pub struct BuildOutput {
    pub candidates: Vec<Candidate>,
    pub crops: Vec<CropSpec>,
}

pub fn build(events: &[Event], _speech: &[Segment], cfg: &BuildConfig) -> BuildOutput {
    let drafts = group::group(events, cfg);
    BuildOutput { candidates: drafts.into_iter().map(|d| d.cand).collect(), crops: vec![] }
}
