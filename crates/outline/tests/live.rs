//! Outline real (não roda no CI). Cria um rascunho de teste; apague-o depois pelo Outline.
//! $env:OUTLINE_URL = "https://wiki.auster.local"; $env:OUTLINE_API_TOKEN = "<token>"; $env:OUTLINE_COLLECTION = "<id da coleção>"
//! cargo test -p screenmanual-outline -- --ignored --nocapture
use screenmanual_core::ports::Wiki;
use screenmanual_outline::Outline;

#[test]
#[ignore = "requer OUTLINE_URL, OUTLINE_API_TOKEN e OUTLINE_COLLECTION"]
fn draft_with_image_roundtrip() {
    let wiki = Outline::from_env().unwrap();
    let collection = std::env::var("OUTLINE_COLLECTION").expect("defina OUTLINE_COLLECTION");
    assert!(
        wiki.collections()
            .unwrap()
            .iter()
            .any(|c| c.id == collection),
        "coleção não encontrada"
    );

    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(8, 8, image::Rgba([200, 30, 30, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let png = png.into_inner();
    let att = wiki
        .upload_image(None, "teste-screenmanual.png", &png)
        .unwrap();

    let text = format!("Teste do screenManual.\n\n![img](/api/attachments.redirect?id={att})\n");
    let doc = wiki
        .create_draft(
            &collection,
            None,
            "Teste screenManual (pode apagar)",
            "📘",
            &text,
        )
        .unwrap();
    println!("rascunho: {}", doc.url);
    let info = wiki.info(&doc.id).unwrap();
    assert_eq!(info.id, doc.id);
    assert!(info.text.contains(&att));
    let updated = wiki
        .update(
            &doc.id,
            "Teste screenManual (pode apagar)",
            "📘",
            &format!("{text}\nAtualizado."),
        )
        .unwrap();
    assert!(updated.revision >= info.revision);
    assert_eq!(
        wiki.download_attachment(&att).unwrap(),
        png,
        "anexo baixado igual ao enviado"
    );
}
