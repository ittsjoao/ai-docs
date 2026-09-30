use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMeta {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    /// RFC 3339 com fuso local, ex.: 2026-09-29T14:30:02-03:00
    pub started_at: String,
    pub audio_offset_ms: Option<i64>,
    pub duration_ms: Option<u64>,
}

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        let c = match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let cut: String = out.trim_end_matches('-').chars().take(40).collect();
    let cut = cut.trim_end_matches('-');
    if cut.is_empty() {
        "sessao".to_string()
    } else {
        cut.to_string()
    }
}

pub fn session_id(started_at: &str, title: &str) -> String {
    let date = started_at.get(0..10).unwrap_or("0000-00-00");
    let hhmm = started_at.get(11..16).unwrap_or("00:00").replace(':', "");
    format!("{date}_{hhmm}_{}", slug(title))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_strips_accents_and_symbols() {
        assert_eq!(slug("Emitir NFS-e (São Paulo)"), "emitir-nfs-e-sao-paulo");
        assert_eq!(slug("  !!  "), "sessao");
        assert_eq!(slug(&"a".repeat(60)).len(), 40);
    }

    #[test]
    fn session_id_uses_local_date_and_time() {
        assert_eq!(
            session_id("2026-09-29T14:30:02-03:00", "Emitir NFS-e"),
            "2026-09-29_1430_emitir-nfs-e"
        );
    }
}
