//! Skill /gerar-manual personalizada pelo operador (spec 2026-10-14 §3).
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use anyhow::{bail, Context, Result};
use screenmanual_core::domain::SkillStatus;
use sha2::{Digest, Sha256};

use crate::SKILL;

/// Sem isto o Claude não reconhece a skill; a cópia que não tem é recusada.
const MARCA: &str = "name: gerar-manual";

/// Cópia em `dir\SKILL.md`; `dir\base.sha256` guarda o hash da embutida que a originou.
pub struct CustomSkill {
    pub dir: PathBuf,
}

/// Hash com o fim de linha normalizado: o `include_str!` muda de bytes entre checkouts LF e CRLF,
/// e isso não pode marcar a cópia como desatualizada.
fn sha256(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.replace("\r\n", "\n").as_bytes()))
}

impl CustomSkill {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn file(&self) -> PathBuf {
        self.dir.join("SKILL.md")
    }

    fn base(&self) -> PathBuf {
        self.dir.join("base.sha256")
    }

    /// A personalizada, se existir; senão a embutida.
    pub fn content(&self) -> Result<String> {
        // Editores podem gravar BOM; antes de `---` o Claude não reconheceria a skill.
        match std::fs::read_to_string(self.file())
            .map(|s| s.strip_prefix('\u{FEFF}').map(str::to_string).unwrap_or(s))
        {
            Ok(s) if s.contains(MARCA) => Ok(s),
            Ok(_) => bail!(
                "skill personalizada inválida (falta `{MARCA}`); corrija ou restaure o padrão nas Configurações"
            ),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(SKILL.to_string()),
            Err(e) => Err(e).with_context(|| format!("falha ao ler {}", self.file().display())),
        }
    }

    pub fn status(&self) -> SkillStatus {
        let Ok(md) = std::fs::metadata(self.file()) else {
            return SkillStatus::default();
        };
        let editada_em = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64);
        let base = std::fs::read_to_string(self.base()).unwrap_or_default();
        SkillStatus {
            personalizada: true,
            editada_em,
            desatualizada: base.trim() != sha256(SKILL),
        }
    }

    /// Cria a cópia a partir da embutida se faltar; nunca sobrescreve a do operador.
    pub fn editar(&self) -> Result<PathBuf> {
        let file = self.file();
        if !file.is_file() {
            std::fs::create_dir_all(&self.dir)
                .with_context(|| format!("falha ao criar {}", self.dir.display()))?;
            std::fs::write(&file, SKILL)
                .with_context(|| format!("falha ao gravar {}", file.display()))?;
            std::fs::write(self.base(), sha256(SKILL))
                .with_context(|| format!("falha ao gravar {}", self.base().display()))?;
        }
        Ok(file)
    }

    /// Apaga a cópia; sem cópia não é erro.
    pub fn restaurar(&self) -> Result<()> {
        for p in [self.file(), self.base()] {
            match std::fs::remove_file(&p) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(e).with_context(|| format!("falha ao apagar {}", p.display()))
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SKILL;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("smskill-{name}-{nanos}"))
    }

    #[test]
    fn sem_copia_usa_a_embutida() {
        let s = CustomSkill::new(temp("padrao"));
        assert_eq!(s.content().unwrap(), SKILL);
        assert_eq!(s.status(), SkillStatus::default());
    }

    #[test]
    fn editar_cria_a_copia_e_nao_sobrescreve_a_do_operador() {
        let s = CustomSkill::new(temp("editar"));
        let path = s.editar().unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SKILL);
        let st = s.status();
        assert!(st.personalizada && !st.desatualizada);
        assert!(st.editada_em.is_some());

        let minha = format!("{SKILL}\n## Regra minha\n");
        std::fs::write(&path, &minha).unwrap();
        assert_eq!(s.editar().unwrap(), path);
        assert_eq!(s.content().unwrap(), minha);
    }

    #[test]
    fn copia_de_outra_versao_fica_desatualizada() {
        let s = CustomSkill::new(temp("velha"));
        s.editar().unwrap();
        std::fs::write(s.dir.join("base.sha256"), "0".repeat(64)).unwrap();
        assert!(s.status().desatualizada);
        std::fs::remove_file(s.dir.join("base.sha256")).unwrap();
        assert!(
            s.status().desatualizada,
            "sem base também conta como desatualizada"
        );
    }

    #[test]
    fn copia_invalida_da_erro_claro() {
        let s = CustomSkill::new(temp("invalida"));
        let path = s.editar().unwrap();
        std::fs::write(&path, "").unwrap();
        let err = s.content().unwrap_err().to_string();
        assert!(err.contains("skill personalizada inválida"), "{err}");
        assert!(err.contains("restaure o padrão"), "{err}");
    }

    #[test]
    fn hash_ignora_o_fim_de_linha() {
        assert_eq!(sha256("a\r\nb"), sha256("a\nb"));
    }

    #[test]
    fn content_ignora_bom_da_copia() {
        let s = CustomSkill::new(temp("bom"));
        let path = s.editar().unwrap();
        std::fs::write(&path, format!("\u{FEFF}{SKILL}")).unwrap();
        assert_eq!(s.content().unwrap(), SKILL);
    }

    #[test]
    fn restaurar_volta_ao_padrao_e_tolera_ausencia() {
        let s = CustomSkill::new(temp("restaurar"));
        s.restaurar().unwrap();
        s.editar().unwrap();
        s.restaurar().unwrap();
        assert_eq!(s.status(), SkillStatus::default());
        assert_eq!(s.content().unwrap(), SKILL);
        s.restaurar().unwrap();
    }
}
