use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use screenmanual_cli::run;
use screenmanual_core::ports::{Collection, DocInfo, PortResult, Wiki};

#[derive(Default)]
struct Inner {
    docs: HashMap<String, (String, u64)>,
    attachments: HashMap<String, Vec<u8>>,
    edited_remotely: bool,
}

/// Outline em memória; clonável para o teste inspecionar e alterar o estado entre execuções.
#[derive(Clone, Default)]
struct FakeWiki(Arc<Mutex<Inner>>);

impl FakeWiki {
    fn doc(&self, id: &str) -> PortResult<DocInfo> {
        let s = self.0.lock().unwrap();
        let (text, rev) = s
            .docs
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("doc {id} não existe"))?;
        let rev = if s.edited_remotely { rev + 10 } else { rev };
        Ok(DocInfo {
            id: id.into(),
            url: format!("https://wiki.x/doc/{id}"),
            revision: rev,
            text,
        })
    }
}

impl Wiki for FakeWiki {
    fn collections(&self) -> PortResult<Vec<Collection>> {
        Ok(vec![])
    }
    fn upload_image(&self, _doc: Option<&str>, _name: &str, bytes: &[u8]) -> PortResult<String> {
        let mut s = self.0.lock().unwrap();
        let id = format!("att-{}", s.attachments.len() + 1);
        s.attachments.insert(id.clone(), bytes.to_vec());
        Ok(id)
    }
    fn create_draft(&self, _col: &str, _title: &str, text: &str) -> PortResult<DocInfo> {
        self.0
            .lock()
            .unwrap()
            .docs
            .insert("doc-1".into(), (text.into(), 1));
        self.doc("doc-1")
    }
    fn update(&self, id: &str, _title: &str, text: &str) -> PortResult<DocInfo> {
        {
            let mut s = self.0.lock().unwrap();
            let rev = s.docs.get(id).map_or(0, |d| d.1) + 1;
            s.docs.insert(id.into(), (text.into(), rev));
        }
        self.doc(id)
    }
    fn info(&self, id: &str) -> PortResult<DocInfo> {
        self.doc(id)
    }
    fn publish(&self, id: &str) -> PortResult<DocInfo> {
        self.doc(id)
    }
    fn download_attachment(&self, id: &str) -> PortResult<Vec<u8>> {
        self.0
            .lock()
            .unwrap()
            .attachments
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("anexo {id} não existe"))
    }
}

const CANDIDATES: &str = r#"[{"id":"c001","t":1000,"kind":"click","app":"erp.exe","window":"ERP","url":null,"el":null,"input":null,"crop":"crops/c001.png","context_shot":null}]"#;

fn steps(imagem: &str) -> String {
    format!(
        r#"{{"schema_version":1,"titulo":"Emitir NFS-e","objetivo":"Emitir a nota.","secoes":[{{"titulo":"Cadastro","passos":[{{"candidatos":["c001"],"imagem":"{imagem}","texto":"Clique em **Nova nota**.","aviso":null,"dica":null}}]}}]}}"#
    )
}

/// Pasta de sessão mínima: session.json, candidates.json, steps.json e um recorte branco 100×100.
fn session(name: &str, imagem: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir()
        .join(format!("smcli-{name}-{nanos}"))
        .join("s1");
    std::fs::create_dir_all(dir.join("crops")).unwrap();
    std::fs::write(
        dir.join("session.json"),
        r#"{"schema_version":1,"id":"s1","title":"Emitir NFS-e","started_at":"2026-10-04T10:00:00-03:00","audio_offset_ms":null,"duration_ms":9000}"#,
    )
    .unwrap();
    std::fs::write(dir.join("candidates.json"), CANDIDATES).unwrap();
    std::fs::write(dir.join("steps.json"), steps(imagem)).unwrap();
    image::RgbaImage::from_pixel(100, 100, image::Rgba([255, 255, 255, 255]))
        .save(dir.join("crops/c001.png"))
        .unwrap();
    dir
}

fn cli(dir: &Path, wiki: &FakeWiki, args: &[&str]) -> (i32, String) {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let w = wiki.clone();
    run(&args, dir, move || Ok(w))
}

fn no_wiki() -> anyhow::Result<FakeWiki> {
    anyhow::bail!("o Outline não deveria ser usado aqui")
}

#[test]
fn usage_and_wrong_folder() {
    let dir = session("uso", "c001");
    let (code, out) = run(&[], &dir, no_wiki);
    assert_eq!(code, 2);
    assert!(out.contains("uso: screenmanual-cli"));
    let (code, out) = run(&["render".to_string()], dir.parent().unwrap(), no_wiki);
    assert_eq!(code, 1);
    assert!(out.contains("dentro da pasta da sessão"), "{out}");
}

#[test]
fn render_writes_manual_and_images_or_reports_the_step() {
    let dir = session("render", "c001");
    let (code, out) = run(&["render".to_string()], &dir, no_wiki);
    assert_eq!(code, 0, "{out}");
    assert!(std::fs::read_to_string(dir.join("manual.md"))
        .unwrap()
        .contains("Nova nota"));
    assert!(dir.join("img/c001.png").exists());

    std::fs::write(dir.join("steps.json"), steps("c999")).unwrap();
    let (code, out) = run(&["render".to_string()], &dir, no_wiki);
    assert_eq!(code, 1);
    assert!(out.starts_with("erro: ") && out.contains("c999"), "{out}");
}

#[test]
fn publish_creates_then_detects_manual_edit_with_exit_3() {
    let dir = session("publish", "c001");
    std::fs::write(dir.join("publish.json"), r#"{"collection_id":"col-1"}"#).unwrap();
    let wiki = FakeWiki::default();
    let (code, out) = cli(&dir, &wiki, &["publish"]);
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v["url"], "https://wiki.x/doc/doc-1");
    assert_eq!(v["status"], "draft");
    assert!(wiki.0.lock().unwrap().docs["doc-1"]
        .0
        .contains("/api/attachments.redirect?id=att-1"));

    wiki.0.lock().unwrap().edited_remotely = true;
    let (code, out) = cli(&dir, &wiki, &["publish"]);
    assert_eq!(code, 3, "{out}");
    assert!(out.contains("editado no Outline"), "{out}");
}

#[test]
fn fetch_saves_the_published_copy_and_reports() {
    let dir = session("fetch", "c001");
    std::fs::write(dir.join("publish.json"), r#"{"collection_id":"col-1"}"#).unwrap();
    let wiki = FakeWiki::default();
    assert_eq!(cli(&dir, &wiki, &["publish"]).0, 0);
    let (code, out) = cli(&dir, &wiki, &["fetch"]);
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v["arquivo"], "published/manual.md");
    assert_eq!(v["imagens"], 1);
    assert_eq!(v["faltando"], serde_json::json!([]));
    assert!(dir.join("published/manual.md").exists());
    assert!(dir.join("published/img/att-1.png").exists());
}

#[test]
fn redact_only_touches_crops() {
    let dir = session("redact", "c001");
    let (code, out) = run(
        &[
            "redact".into(),
            "crops/c001.png".into(),
            "10,10,20,20".into(),
        ],
        &dir,
        no_wiki,
    );
    assert_eq!(code, 0, "{out}");
    let img = image::open(dir.join("crops/c001.png")).unwrap().to_rgba8();
    assert_eq!(
        *img.get_pixel(5, 5),
        image::Rgba([0, 0, 0, 255]),
        "margem de 6 px"
    );
    assert_eq!(*img.get_pixel(2, 2), image::Rgba([255, 255, 255, 255]));
    for (path, rect) in [
        ("../fora.png", "1,1,1,1"),
        ("img/c001.png", "1,1,1,1"),
        ("crops/c001.png", "1,2,3"),
    ] {
        let (code, _) = run(&["redact".into(), path.into(), rect.into()], &dir, no_wiki);
        assert_ne!(code, 0, "{path} {rect}");
    }
}

#[test]
fn redact_blocks_traversal_and_accepts_backslash() {
    let dir = session("redact_bs", "c001");
    let antes = std::fs::read(dir.join("session.json")).unwrap();
    let (code, _) = run(
        &[
            "redact".into(),
            "crops/../session.json".into(),
            "1,1,5,5".into(),
        ],
        &dir,
        no_wiki,
    );
    assert_ne!(code, 0);
    assert_eq!(std::fs::read(dir.join("session.json")).unwrap(), antes);
    let (code, out) = run(
        &[
            "redact".into(),
            "crops\\c001.png".into(),
            "10,10,20,20".into(),
        ],
        &dir,
        no_wiki,
    );
    assert_eq!(code, 0, "{out}");
}
