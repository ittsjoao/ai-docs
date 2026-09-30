use image::{Rgba, RgbaImage};
use screenmanual_core::domain::{CaptureConfig, Rect};

pub(crate) fn denied(app: &str, title: &str, cfg: &CaptureConfig) -> bool {
    let exe = app.rsplit(['\\', '/']).next().unwrap_or(app).to_lowercase();
    let title = title.to_lowercase();
    cfg.deny_processes.iter().any(|p| p.to_lowercase() == exe)
        || cfg
            .deny_title_words
            .iter()
            .any(|w| title.contains(&w.to_lowercase()))
}

/// Pinta `r` (coordenadas da imagem) de preto, recortado aos limites da imagem.
pub(crate) fn blackout(img: &mut RgbaImage, r: Rect) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let (x0, x1) = (r.left.clamp(0, w), r.right.clamp(0, w));
    let (y0, y1) = (r.top.clamp(0, h), r.bottom.clamp(0, h));
    for y in y0..y1 {
        for x in x0..x1 {
            img.put_pixel(x as u32, y as u32, Rgba([0, 0, 0, 255]));
        }
    }
}

/// Reduz a URL da barra de endereço a `esquema://host/caminho`: sem query, fragmento ou credenciais.
/// Texto sem cara de endereço (placeholder, termo de busca) vira `None`.
pub(crate) fn sanitize_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.contains(char::is_whitespace) {
        return None;
    }
    // Cut query/fragment first to avoid leaking them when parsing schemeless URLs
    let before_query = raw.split(['?', '#']).next().unwrap_or("");

    // Try to extract scheme, validating it matches [A-Za-z][A-Za-z0-9+.-]*
    let (scheme, rest) = match before_query.split_once("://") {
        Some((s, r)) => {
            // Validate scheme format
            if s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '.' || c == '-')
            {
                (s.to_lowercase(), r)
            } else {
                // Invalid scheme, treat whole thing as schemeless
                ("https".to_string(), before_query)
            }
        }
        None => ("https".to_string(), before_query),
    };

    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let host = authority.rsplit('@').next().unwrap_or(authority);
    if host.is_empty() || !(host.contains('.') || host.contains(':') || host == "localhost") {
        return None;
    }
    Some(format!("{scheme}://{host}{path}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn denylist_matches_exe_and_title_words_case_insensitive() {
        let cfg = CaptureConfig::default();
        assert!(denied("KeePass.exe", "Banco de dados", &cfg));
        assert!(denied(r"C:\Apps\bitwarden.exe", "", &cfg));
        assert!(denied("chrome.exe", "Alterar SENHA - Portal", &cfg));
        assert!(!denied("chrome.exe", "Nota Fiscal", &cfg));
    }

    #[test]
    fn blackout_paints_clamped_rect() {
        let mut img = image::RgbaImage::from_pixel(10, 10, Rgba([255, 255, 255, 255]));
        blackout(
            &mut img,
            Rect {
                left: 8,
                top: -5,
                right: 20,
                bottom: 2,
            },
        );
        assert_eq!(*img.get_pixel(9, 0), Rgba([0, 0, 0, 255]));
        assert_eq!(*img.get_pixel(8, 1), Rgba([0, 0, 0, 255]));
        assert_eq!(*img.get_pixel(7, 1), Rgba([255, 255, 255, 255]));
        assert_eq!(*img.get_pixel(9, 2), Rgba([255, 255, 255, 255]));
        blackout(
            &mut img,
            Rect {
                left: 50,
                top: 50,
                right: 60,
                bottom: 60,
            },
        ); // fora: não entra em pânico
    }

    #[test]
    fn url_keeps_only_scheme_host_and_path() {
        assert_eq!(
            sanitize_url("https://nfse.prefeitura.sp.gov.br/emitir?cnpj=123#topo").as_deref(),
            Some("https://nfse.prefeitura.sp.gov.br/emitir")
        );
        assert_eq!(
            sanitize_url("google.com/search?q=senha").as_deref(),
            Some("https://google.com/search")
        );
        assert_eq!(
            sanitize_url("https://joao:segredo@intranet.local/painel").as_deref(),
            Some("https://intranet.local/painel")
        );
        assert_eq!(
            sanitize_url("http://10.10.30.1:8080").as_deref(),
            Some("http://10.10.30.1:8080")
        );
        assert_eq!(sanitize_url("  "), None);
        assert_eq!(
            sanitize_url("Pesquisar no Google ou digitar URL"),
            None,
            "placeholder da barra vazia"
        );
        // Regression: query should not leak when it contains another URL
        assert_eq!(
            sanitize_url("google.com/login?next=https://evil.com/a").as_deref(),
            Some("https://google.com/login")
        );
    }
}
