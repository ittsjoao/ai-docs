use serde::Serialize;

/// Estado da skill /gerar-manual personalizada pelo operador (spec 2026-10-14 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct SkillStatus {
    pub personalizada: bool,
    /// Última modificação do SKILL.md personalizado, em ms desde a época Unix.
    pub editada_em: Option<u64>,
    /// A skill embutida mudou desde que a cópia foi criada.
    pub desatualizada: bool,
}
