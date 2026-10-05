//! Modelos do whisper: catálogo, download com progresso e verificação sha256 (spec §6.1.7, D11).

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use screenmanual_core::domain::TranscriptionModel;
use screenmanual_whisper::model_file;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelInfo {
    pub file: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// Tamanho e sha256 do repositório ggerganov/whisper.cpp no Hugging Face (consultado em 2026-10-04).
pub fn model_info(m: TranscriptionModel) -> ModelInfo {
    let (size, sha256) = match m {
        TranscriptionModel::Rapido => (
            487_601_967,
            "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        ),
        TranscriptionModel::Equilibrado => (
            539_212_467,
            "19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f",
        ),
        TranscriptionModel::Preciso => (
            574_041_195,
            "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        ),
    };
    ModelInfo {
        file: model_file(m),
        size,
        sha256,
    }
}

/// Copia `reader` para `dest` conferindo o sha256; grava em `.part` e só renomeia se bater.
pub fn save_verified(
    mut reader: impl Read,
    dest: &Path,
    info: &ModelInfo,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<()> {
    let part = dest.with_extension("part");
    let mut out =
        File::create(&part).with_context(|| format!("falha ao criar {}", part.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader
            .read(&mut buf)
            .context("download do modelo interrompido")?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n])
            .with_context(|| format!("falha ao gravar {}", part.display()))?;
        done += n as u64;
        progress(done, info.size);
    }
    drop(out);
    if format!("{:x}", hasher.finalize()) != info.sha256 {
        let _ = std::fs::remove_file(&part);
        bail!(
            "modelo {} corrompido no download (sha256 diferente); tente de novo",
            info.file
        );
    }
    std::fs::rename(&part, dest).with_context(|| format!("falha ao gravar {}", dest.display()))
}

/// Caminho do modelo, baixando-o se faltar.
// ponytail: arquivo presente com o tamanho certo é aceito sem rehash (o hash de ~550 MB custaria
// segundos a cada processamento); corrompido com o mesmo tamanho só se resolve apagando o arquivo.
pub fn ensure_model(
    models_dir: &Path,
    m: TranscriptionModel,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<PathBuf> {
    let info = model_info(m);
    let dest = models_dir.join(info.file);
    if std::fs::metadata(&dest).is_ok_and(|md| md.len() == info.size) {
        return Ok(dest);
    }
    std::fs::create_dir_all(models_dir)
        .with_context(|| format!("falha ao criar {}", models_dir.display()))?;
    let url = format!("{BASE_URL}/{}", info.file);
    // o padrão do reqwest blocking é 30 s no total; um modelo de ~550 MB ganha até 1 h, e sem
    // resposta na conexão desiste em 30 s (cancelar o download fica para o plano 06)
    let http = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(3600))
        .build()?;
    let resp = http
        .get(&url)
        .send()
        .with_context(|| format!("falha ao baixar {url}"))?;
    if !resp.status().is_success() {
        bail!("download do modelo falhou ({}): {url}", resp.status());
    }
    save_verified(resp, &dest, &info, progress)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenmanual_core::domain::TranscriptionModel::*;

    const ABC: ModelInfo = ModelInfo {
        file: "abc.bin",
        size: 3,
        sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    };

    fn temp(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("smmodels-{name}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn catalog_matches_the_whisper_file_names() {
        for m in [Rapido, Equilibrado, Preciso] {
            let i = model_info(m);
            assert_eq!(i.file, model_file(m));
            assert_eq!(i.sha256.len(), 64);
            assert!(i.size > 400_000_000, "{}", i.file);
        }
        assert_eq!(model_info(Preciso).size, 574_041_195);
    }

    #[test]
    fn save_verified_renames_only_when_the_hash_matches() {
        let dir = temp("verify");
        let dest = dir.join("abc.bin");
        let mut seen = vec![];
        save_verified(&b"abc"[..], &dest, &ABC, &mut |d, t| seen.push((d, t))).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"abc");
        assert_eq!(seen.last(), Some(&(3, 3)));
        assert!(!dest.with_extension("part").exists());

        let bad = dir.join("bad.bin");
        let err = save_verified(&b"abd"[..], &bad, &ABC, &mut |_, _| {})
            .unwrap_err()
            .to_string();
        assert!(err.contains("corrompido"), "{err}");
        assert!(!bad.exists() && !bad.with_extension("part").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "baixa o ggml-small (~490 MB) do Hugging Face"]
    fn downloads_and_verifies_a_real_model() {
        let dir = temp("download");
        let mut last = (0, 0);
        let path = ensure_model(&dir, Rapido, &mut |d, t| last = (d, t)).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().len(),
            model_info(Rapido).size
        );
        assert_eq!(last.0, last.1);
        let again = ensure_model(&dir, Rapido, &mut |_, _| {
            panic!("não deveria baixar de novo")
        })
        .unwrap();
        assert_eq!(again, path);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
