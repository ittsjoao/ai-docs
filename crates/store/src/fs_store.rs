use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use screenmanual_core::domain::{
    Candidate, Event, Manual, Perguntas, PublishState, Rendered, Respostas, Segment, SessionFacts,
    SessionMeta,
};
use screenmanual_core::ports::SessionStore;
use serde::de::DeserializeOwned;
use serde::Serialize;

const META: &str = "session.json";
const EVENTS: &str = "events.jsonl";
const AUDIO: &str = "audio.wav";
const TRANSCRIPT: &str = "transcript.jsonl";
const CANDIDATES: &str = "candidates.json";
const STEPS: &str = "steps.json";
const MANUAL_MD: &str = "manual.md";
const PUBLISHED: &str = "published";
const PUBLISH: &str = "publish.json";
const FEEDBACK: &str = "feedback.jsonl";
const ERROR: &str = "error.txt";
const PERGUNTAS: &str = "perguntas.json";
const RESPOSTAS: &str = "respostas.json";
/// Um WAV só com cabeçalho não tem áudio.
const WAV_HEADER: u64 = 44;

/// Pasta de sessões em disco (spec §3). Cada sessão é uma subpasta com `session.json`.
pub struct FsStore {
    root: PathBuf,
}

impl FsStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn file(&self, id: &str, name: &str) -> PathBuf {
        self.dir(id).join(name)
    }
}

/// Grava num `.tmp` e renomeia: um crash no meio não deixa JSON pela metade.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("falha ao criar {}", parent.display()))?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).with_context(|| format!("falha ao gravar {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("falha ao gravar {}", path.display()))
}

fn write_json<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<()> {
    write_atomic(path, &serde_json::to_vec_pretty(value)?)
}

fn read_bytes(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("falha ao ler {}", path.display())),
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let Some(bytes) = read_bytes(path)? else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .with_context(|| format!("{} inválido", path.display()))
}

/// JSON Lines. A última linha pode ter ficado pela metade num crash (spec §5) e é ignorada;
/// uma linha inválida no meio é erro.
fn read_jsonl<T: DeserializeOwned>(path: &Path) -> Result<Option<Vec<T>>> {
    let Some(bytes) = read_bytes(path)? else {
        return Ok(None);
    };
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let mut out = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        match serde_json::from_str(line) {
            Ok(v) => out.push(v),
            Err(_) if i + 1 == lines.len() => break,
            Err(e) => bail!("{} linha {}: {e}", path.display(), i + 1),
        }
    }
    Ok(Some(out))
}

fn write_jsonl<T: Serialize>(path: &Path, items: &[T]) -> Result<()> {
    let mut buf = Vec::new();
    for item in items {
        serde_json::to_writer(&mut buf, item)?;
        buf.push(b'\n');
    }
    write_atomic(path, &buf)
}

/// Resolve `rel` dentro de `dir`, recusando `..`, raiz e unidade (os caminhos vêm do steps.json).
fn inside(dir: &Path, rel: &str) -> Result<PathBuf> {
    let p = Path::new(rel);
    if rel.is_empty() || !p.components().all(|c| matches!(c, Component::Normal(_))) {
        bail!("caminho fora da pasta da sessão: {rel:?}");
    }
    Ok(dir.join(p))
}

/// Resolve um caminho relativo dentro da pasta da sessão, recusando `..`, raiz e unidade.
pub fn session_path(dir: &Path, rel: &str) -> Result<PathBuf> {
    inside(dir, rel)
}

fn remove_if_exists<F>(path: &Path, remove: F) -> Result<()>
where
    F: Fn(&Path) -> std::io::Result<()>,
{
    match remove(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("falha ao remover {}", path.display())),
    }
}

/// Hora atual em RFC 3339 UTC (ex.: `2026-09-30T14:03:05Z`), para carimbar o feedback.
pub fn utc_now_rfc3339() -> String {
    rfc3339_utc(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    )
}

/// Dias desde 1970 → data civil (algoritmo de Howard Hinnant), sem depender de crate de datas.
fn rfc3339_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}

impl SessionStore for FsStore {
    fn dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    fn list_ids(&self) -> Result<Vec<String>> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => {
                return Err(e).with_context(|| format!("falha ao listar {}", self.root.display()))
            }
        };
        let mut ids = Vec::new();
        for entry in entries {
            let path = entry?.path();
            if path.join(META).is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    ids.push(name.to_string());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    fn create(&self, meta: &SessionMeta) -> Result<()> {
        let path = self.file(&meta.id, META);
        if path.exists() {
            bail!("a sessão {} já existe", meta.id);
        }
        write_json(&path, meta)
    }

    fn meta(&self, id: &str) -> Result<SessionMeta> {
        read_json(&self.file(id, META))?
            .with_context(|| format!("session.json ausente em {}", self.dir(id).display()))
    }

    fn save_meta(&self, meta: &SessionMeta) -> Result<()> {
        write_json(&self.file(&meta.id, META), meta)
    }

    fn events(&self, id: &str) -> Result<Vec<Event>> {
        Ok(read_jsonl(&self.file(id, EVENTS))?.unwrap_or_default())
    }

    fn audio_path(&self, id: &str) -> Result<Option<PathBuf>> {
        let path = self.file(id, AUDIO);
        Ok(fs::metadata(&path)
            .is_ok_and(|m| m.len() > WAV_HEADER)
            .then_some(path))
    }

    fn transcript(&self, id: &str) -> Result<Option<Vec<Segment>>> {
        read_jsonl(&self.file(id, TRANSCRIPT))
    }

    fn save_transcript(&self, id: &str, segments: &[Segment]) -> Result<()> {
        write_jsonl(&self.file(id, TRANSCRIPT), segments)
    }

    fn candidates(&self, id: &str) -> Result<Vec<Candidate>> {
        read_json(&self.file(id, CANDIDATES))?
            .context("candidates.json ausente; processe a sessão primeiro")
    }

    fn save_candidates(&self, id: &str, candidates: &[Candidate]) -> Result<()> {
        write_json(&self.file(id, CANDIDATES), candidates)
    }

    fn manual(&self, id: &str) -> Result<Option<Manual>> {
        read_json(&self.file(id, STEPS))
    }

    fn save_manual(&self, id: &str, manual: &Manual) -> Result<()> {
        write_json(&self.file(id, STEPS), manual)
    }

    fn add_image(&self, id: &str, bytes: &[u8]) -> Result<String> {
        let img = image::load_from_memory(bytes)
            .map_err(|_| anyhow::anyhow!("não é uma imagem PNG ou JPEG"))?;
        let crops = self.dir(id).join("crops");
        fs::create_dir_all(&crops)?;
        let n = (1..)
            .find(|n| !crops.join(format!("u{n:03}.png")).exists())
            .expect("sempre há um número livre");
        let nome = format!("u{n:03}");
        img.save_with_format(crops.join(format!("{nome}.png")), image::ImageFormat::Png)
            .with_context(|| format!("falha ao gravar crops/{nome}.png"))?;
        Ok(nome)
    }

    fn save_rendered(&self, id: &str, rendered: &Rendered) -> Result<()> {
        let dir = self.dir(id);
        // Validate all paths first before copying anything (atomic validation)
        let mut copies = Vec::new();
        for img in &rendered.images {
            let (from, to) = (inside(&dir, &img.from)?, inside(&dir, &img.to)?);
            copies.push((from, to));
        }
        // Now copy all images
        for (from, to) in copies {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&from, &to).with_context(|| format!("crop ausente: {}", from.display()))?;
        }
        write_atomic(&dir.join(MANUAL_MD), rendered.markdown.as_bytes())
    }

    fn read_file(&self, id: &str, rel: &str) -> Result<Vec<u8>> {
        let path = inside(&self.dir(id), rel)?;
        fs::read(&path).with_context(|| format!("arquivo ausente: {rel}"))
    }

    fn save_published(&self, id: &str, markdown: &str, images: &[(String, Vec<u8>)]) -> Result<()> {
        for (attachment, _) in images {
            if attachment.is_empty()
                || !attachment
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                bail!("id de anexo inválido: {attachment:?}");
            }
        }
        let dir = self.dir(id).join(PUBLISHED);
        let img_dir = dir.join("img");
        remove_if_exists(&img_dir, |p| fs::remove_dir_all(p))?;
        fs::create_dir_all(&img_dir)?;
        for (attachment, bytes) in images {
            fs::write(img_dir.join(format!("{attachment}.png")), bytes)?;
        }
        write_atomic(&dir.join(MANUAL_MD), markdown.as_bytes())
    }

    fn publish_state(&self, id: &str) -> Result<Option<PublishState>> {
        read_json(&self.file(id, PUBLISH))
    }

    fn save_publish_state(&self, id: &str, state: &PublishState) -> Result<()> {
        write_json(&self.file(id, PUBLISH), state)
    }

    fn append_feedback(&self, id: &str, text: &str) -> Result<()> {
        let path = self.file(id, FEEDBACK);
        let mut line =
            serde_json::to_vec(&serde_json::json!({ "t": utc_now_rfc3339(), "texto": text }))?;
        line.push(b'\n');
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("falha ao abrir {}", path.display()))?;
        file.write_all(&line)?;
        Ok(())
    }

    fn perguntas(&self, id: &str) -> Result<Option<Perguntas>> {
        read_json(&self.file(id, PERGUNTAS))
    }

    fn save_respostas(&self, id: &str, respostas: &Respostas) -> Result<()> {
        write_json(&self.file(id, RESPOSTAS), respostas)
    }

    fn clear_perguntas(&self, id: &str) -> Result<()> {
        for f in [PERGUNTAS, RESPOSTAS] {
            remove_if_exists(&self.file(id, f), |p| fs::remove_file(p))?;
        }
        Ok(())
    }

    fn facts(&self, id: &str) -> Result<SessionFacts> {
        Ok(SessionFacts {
            ended: self
                .events(id)?
                .iter()
                .any(|e| matches!(e, Event::SessionEnd { .. })),
            has_candidates: self.file(id, CANDIDATES).is_file(),
            publish: self.publish_state(id)?,
            error: read_bytes(&self.file(id, ERROR))?
                .map(|b| String::from_utf8_lossy(&b).into_owned()),
            perguntas_pendentes: self.file(id, PERGUNTAS).is_file()
                && !self.file(id, RESPOSTAS).is_file(),
        })
    }

    fn set_error(&self, id: &str, message: Option<&str>) -> Result<()> {
        let path = self.file(id, ERROR);
        match message {
            Some(m) => write_atomic(&path, m.as_bytes()),
            None => remove_if_exists(&path, |p| fs::remove_file(p)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenmanual_core::domain::{ImageCopy, SCHEMA_VERSION};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("smstore-{name}-{nanos}"))
    }

    fn meta(id: &str) -> SessionMeta {
        SessionMeta {
            schema_version: SCHEMA_VERSION,
            id: id.into(),
            title: "Emitir NFS-e".into(),
            started_at: "2026-09-30T14:03:05-03:00".into(),
            audio_offset_ms: None,
            duration_ms: None,
        }
    }

    #[test]
    fn add_image_converte_jpeg_e_recusa_lixo() {
        let root = temp_root("add-image");
        let store = FsStore::new(&root);
        store.create(&meta("s")).unwrap();
        let mut jpg = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(4, 4)
            .write_to(&mut jpg, image::ImageFormat::Jpeg)
            .unwrap();
        assert_eq!(store.add_image("s", jpg.get_ref()).unwrap(), "u001");
        assert_eq!(store.add_image("s", jpg.get_ref()).unwrap(), "u002");
        assert!(image::open(root.join("s/crops/u001.png")).is_ok());
        let e = store.add_image("s", b"texto copiado").unwrap_err();
        assert_eq!(e.to_string(), "não é uma imagem PNG ou JPEG");
        assert!(!root.join("s/crops/u003.png").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn creates_lists_and_updates_meta() {
        let root = temp_root("meta");
        let store = FsStore::new(&root);
        assert!(
            store.list_ids().unwrap().is_empty(),
            "raiz inexistente = nenhuma sessão"
        );
        store.create(&meta("b")).unwrap();
        store.create(&meta("a")).unwrap();
        fs::create_dir_all(root.join("sem-session-json")).unwrap();
        assert_eq!(store.list_ids().unwrap(), vec!["a", "b"]);
        assert!(
            store.create(&meta("a")).is_err(),
            "sessão existente não é sobrescrita"
        );
        let mut m = store.meta("a").unwrap();
        m.duration_ms = Some(9000);
        store.save_meta(&m).unwrap();
        assert_eq!(store.meta("a").unwrap().duration_ms, Some(9000));
        assert!(store
            .meta("zzz")
            .unwrap_err()
            .to_string()
            .contains("session.json"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn events_tolerate_truncated_last_line_but_not_corruption_in_the_middle() {
        let root = temp_root("events");
        let store = FsStore::new(&root);
        store.create(&meta("s")).unwrap();
        assert!(
            store.events("s").unwrap().is_empty(),
            "sem events.jsonl ainda"
        );
        let path = root.join("s").join("events.jsonl");
        fs::write(
            &path,
            "{\"type\":\"session_start\",\"t\":0}\n{\"type\":\"marker\",\"t\":10}\n{\"type\":\"cli",
        )
        .unwrap();
        assert_eq!(
            store.events("s").unwrap(),
            vec![Event::SessionStart { t: 0 }, Event::Marker { t: 10 }]
        );
        fs::write(
            &path,
            "{\"type\":\"session_start\",\"t\":0}\nlixo\n{\"type\":\"marker\",\"t\":10}\n",
        )
        .unwrap();
        assert!(store
            .events("s")
            .unwrap_err()
            .to_string()
            .contains("linha 2"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn facts_and_error_follow_the_files() {
        let root = temp_root("facts");
        let store = FsStore::new(&root);
        store.create(&meta("s")).unwrap();
        assert_eq!(store.facts("s").unwrap(), SessionFacts::default());
        fs::write(
            root.join("s").join("events.jsonl"),
            "{\"type\":\"session_end\",\"t\":900}\n",
        )
        .unwrap();
        store.save_candidates("s", &[]).unwrap();
        store.set_error("s", Some("falhou")).unwrap();
        let f = store.facts("s").unwrap();
        assert!(f.ended && f.has_candidates);
        fs::write(root.join("s/perguntas.json"), "{}").unwrap();
        assert!(store.facts("s").unwrap().perguntas_pendentes);
        fs::write(root.join("s/respostas.json"), "{}").unwrap();
        assert!(!store.facts("s").unwrap().perguntas_pendentes);
        store.clear_perguntas("s").unwrap();
        assert!(!root.join("s/perguntas.json").exists());
        assert_eq!(f.error.as_deref(), Some("falhou"));
        store.set_error("s", None).unwrap();
        store.set_error("s", None).unwrap(); // limpar duas vezes não falha
        assert_eq!(store.facts("s").unwrap().error, None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn audio_path_needs_samples_beyond_the_header() {
        let root = temp_root("audio");
        let store = FsStore::new(&root);
        store.create(&meta("s")).unwrap();
        assert_eq!(store.audio_path("s").unwrap(), None);
        let wav = root.join("s").join("audio.wav");
        fs::write(&wav, [0u8; 44]).unwrap();
        assert_eq!(store.audio_path("s").unwrap(), None, "só cabeçalho");
        fs::write(&wav, [0u8; 100]).unwrap();
        assert_eq!(store.audio_path("s").unwrap(), Some(wav));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn transcript_candidates_manual_and_publish_roundtrip() {
        let root = temp_root("roundtrip");
        let store = FsStore::new(&root);
        store.create(&meta("s")).unwrap();
        assert_eq!(store.transcript("s").unwrap(), None);
        let segs = vec![Segment {
            start: 1500,
            end: 2500,
            text: "clico em nova nota".into(),
            words: vec![],
        }];
        store.save_transcript("s", &segs).unwrap();
        assert_eq!(store.transcript("s").unwrap(), Some(segs));
        store.save_transcript("s", &[]).unwrap();
        assert_eq!(
            store.transcript("s").unwrap(),
            Some(vec![]),
            "vazio ≠ ausente"
        );
        assert!(store
            .candidates("s")
            .unwrap_err()
            .to_string()
            .contains("candidates.json"));
        store.save_candidates("s", &[]).unwrap();
        assert!(store.candidates("s").unwrap().is_empty());
        assert_eq!(store.manual("s").unwrap(), None);
        fs::write(
            root.join("s").join("steps.json"),
            r#"{"schema_version":1,"titulo":"T","secoes":[]}"#,
        )
        .unwrap();
        assert_eq!(store.manual("s").unwrap().unwrap().titulo, "T");
        assert_eq!(store.publish_state("s").unwrap(), None);
        let mut p = PublishState::new("col");
        p.outline_id = Some("doc".into());
        store.save_publish_state("s", &p).unwrap();
        assert_eq!(store.publish_state("s").unwrap(), Some(p));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rendered_images_are_copied_and_paths_stay_inside_the_session() {
        let root = temp_root("rendered");
        let store = FsStore::new(&root);
        store.create(&meta("s")).unwrap();
        let dir = root.join("s");
        fs::create_dir_all(dir.join("crops")).unwrap();
        fs::write(dir.join("crops").join("c001.png"), b"png").unwrap();
        let rendered = Rendered {
            markdown: "# T\n".into(),
            images: vec![ImageCopy {
                from: "crops/c001.png".into(),
                to: "img/c001.png".into(),
            }],
        };
        store.save_rendered("s", &rendered).unwrap();
        assert_eq!(fs::read(dir.join("img").join("c001.png")).unwrap(), b"png");
        assert_eq!(fs::read_to_string(dir.join("manual.md")).unwrap(), "# T\n");
        assert_eq!(store.read_file("s", "img/c001.png").unwrap(), b"png");
        for bad in [
            "../outra/session.json",
            "C:/Windows/win.ini",
            "/etc/passwd",
            "",
        ] {
            let err = store.read_file("s", bad).unwrap_err().to_string();
            assert!(err.contains("fora da pasta"), "{bad}: {err}");
        }
        let missing = Rendered {
            markdown: String::new(),
            images: vec![ImageCopy {
                from: "crops/c999.png".into(),
                to: "img/c999.png".into(),
            }],
        };
        assert!(store
            .save_rendered("s", &missing)
            .unwrap_err()
            .to_string()
            .contains("crops/c999.png"));
        // Test that save_rendered with bad 'to' path rejects and leaves no trace
        let bad_to = Rendered {
            markdown: String::new(),
            images: vec![ImageCopy {
                from: "crops/c001.png".into(),
                to: "../fora.png".into(),
            }],
        };
        let err = store.save_rendered("s", &bad_to).unwrap_err().to_string();
        assert!(
            err.contains("fora da pasta"),
            "save_rendered with bad 'to': {err}"
        );
        assert!(
            !root.join("fora.png").exists(),
            "bad path should not create files outside session"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn published_copy_replaces_previous_images() {
        let root = temp_root("published");
        let store = FsStore::new(&root);
        store.create(&meta("s")).unwrap();
        let img = root.join("s").join("published").join("img");
        store
            .save_published(
                "s",
                "v1",
                &[
                    ("att1".into(), b"a".to_vec()),
                    ("att2".into(), b"b".to_vec()),
                ],
            )
            .unwrap();
        store
            .save_published("s", "v2", &[("att3".into(), b"c".to_vec())])
            .unwrap();
        assert_eq!(
            fs::read_to_string(root.join("s").join("published").join("manual.md")).unwrap(),
            "v2"
        );
        assert!(!img.join("att1.png").exists());
        assert_eq!(fs::read(img.join("att3.png")).unwrap(), b"c");
        assert!(store
            .save_published("s", "x", &[("../fora".into(), vec![])])
            .is_err());
        assert!(
            img.join("att3.png").exists(),
            "id inválido não apaga a cópia anterior"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn feedback_lines_are_appended_with_a_utc_timestamp() {
        let root = temp_root("feedback");
        let store = FsStore::new(&root);
        store.create(&meta("s")).unwrap();
        store
            .append_feedback("s", "troque a imagem do passo 2")
            .unwrap();
        store.append_feedback("s", "linha\ncom quebra").unwrap();
        let text = fs::read_to_string(root.join("s").join("feedback.jsonl")).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1]["texto"], "linha\ncom quebra");
        assert!(lines[0]["t"].as_str().unwrap().ends_with('Z'));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rfc3339_utc_formats_known_instants() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339_utc(1_790_776_985), "2026-09-30T14:03:05Z");
    }

    #[test]
    fn session_path_is_the_public_face_of_inside() {
        let dir = Path::new("sessao");
        assert_eq!(
            session_path(dir, "crops/c001.png").unwrap(),
            dir.join("crops/c001.png")
        );
        assert!(session_path(dir, "../fora.png")
            .unwrap_err()
            .to_string()
            .contains("fora da pasta"));
    }
}
