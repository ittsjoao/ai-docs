use serde::Serialize;

use super::{PublishState, PublishStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Recording,
    Interrupted,
    Stopped,
    Processing,
    Ready,
    Generating,
    Draft,
    Published,
    Error,
}

/// Operação em andamento; vive na memória do app, não em arquivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Recording,
    Processing,
    Generating,
}

/// O que os arquivos da pasta da sessão dizem sobre ela.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionFacts {
    pub ended: bool,
    pub has_candidates: bool,
    pub publish: Option<PublishState>,
    pub error: Option<String>,
}

pub fn derive_status(f: &SessionFacts, activity: Option<Activity>) -> SessionStatus {
    match activity {
        Some(Activity::Recording) => return SessionStatus::Recording,
        Some(Activity::Processing) => return SessionStatus::Processing,
        Some(Activity::Generating) => return SessionStatus::Generating,
        None => {}
    }
    if f.error.is_some() {
        return SessionStatus::Error;
    }
    if let Some(p) = f.publish.as_ref().filter(|p| p.outline_id.is_some()) {
        return if p.status == Some(PublishStatus::Published) {
            SessionStatus::Published
        } else {
            SessionStatus::Draft
        };
    }
    if f.has_candidates {
        SessionStatus::Ready
    } else if f.ended {
        SessionStatus::Stopped
    } else {
        SessionStatus::Interrupted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{PublishState, PublishStatus};

    fn published(status: PublishStatus) -> Option<PublishState> {
        let mut p = PublishState::new("col");
        p.outline_id = Some("doc".into());
        p.status = Some(status);
        Some(p)
    }

    #[test]
    fn derives_status_from_facts() {
        let f = SessionFacts::default();
        assert_eq!(derive_status(&f, None), SessionStatus::Interrupted);
        assert_eq!(
            derive_status(&f, Some(Activity::Recording)),
            SessionStatus::Recording
        );
        let f = SessionFacts {
            ended: true,
            ..Default::default()
        };
        assert_eq!(derive_status(&f, None), SessionStatus::Stopped);
        let f = SessionFacts {
            ended: true,
            has_candidates: true,
            ..Default::default()
        };
        assert_eq!(derive_status(&f, None), SessionStatus::Ready);
        assert_eq!(
            derive_status(&f, Some(Activity::Generating)),
            SessionStatus::Generating
        );
        let f = SessionFacts {
            has_candidates: true,
            publish: published(PublishStatus::Draft),
            ..Default::default()
        };
        assert_eq!(derive_status(&f, None), SessionStatus::Draft);
        let f = SessionFacts {
            publish: published(PublishStatus::Published),
            ..Default::default()
        };
        assert_eq!(derive_status(&f, None), SessionStatus::Published);
        let f = SessionFacts {
            has_candidates: true,
            error: Some("x".into()),
            ..Default::default()
        };
        assert_eq!(derive_status(&f, None), SessionStatus::Error);
    }
}
