use std::collections::BTreeMap;

const ATTACHMENT_PREFIX: &str = "/api/attachments.redirect?id=";

pub fn strip_frontmatter(md: &str) -> &str {
    if let Some(rest) = md.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            return rest[end + 5..].trim_start_matches('\n');
        }
    }
    md
}

/// Remove o `# título` inicial (o Outline guarda o título à parte) e as linhas em branco seguintes.
pub fn strip_title(md: &str) -> &str {
    match md.strip_prefix("# ") {
        Some(rest) => rest
            .split_once('\n')
            .map_or("", |(_, tail)| tail.trim_start_matches('\n')),
        None => md,
    }
}

pub fn rewrite_images(md: &str, map: &BTreeMap<String, String>) -> String {
    let mut out = md.to_string();
    for (path, attachment) in map {
        out = out.replace(
            &format!("]({path})"),
            &format!("]({ATTACHMENT_PREFIX}{attachment})"),
        );
    }
    out
}

pub fn extract_attachment_ids(md: &str) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let mut rest = md;
    while let Some(pos) = rest.find(ATTACHMENT_PREFIX) {
        rest = &rest[pos + ATTACHMENT_PREFIX.len()..];
        let id: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        if !id.is_empty() && !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_leading_frontmatter_only() {
        assert_eq!(strip_frontmatter("---\nsessao: s1\n---\n\n# T\n"), "# T\n");
        assert_eq!(
            strip_frontmatter("# T\n---\nx\n---\n"),
            "# T\n---\nx\n---\n"
        );
    }

    #[test]
    fn strips_leading_title_only() {
        assert_eq!(strip_title("# T\n\nCorpo\n\n## S\n"), "Corpo\n\n## S\n");
        assert_eq!(strip_title("Corpo\n# T\n"), "Corpo\n# T\n");
        assert_eq!(strip_title("## S\n"), "## S\n");
        assert_eq!(strip_title("# T"), "");
    }

    #[test]
    fn rewrites_image_links_to_attachments() {
        let map = BTreeMap::from([("img/c001.png".to_string(), "abc-123".to_string())]);
        assert_eq!(
            rewrite_images("![P](img/c001.png)", &map),
            "![P](/api/attachments.redirect?id=abc-123)"
        );
    }

    #[test]
    fn extracts_unique_attachment_ids() {
        let md = "![a](/api/attachments.redirect?id=abc-1) ![b](/api/attachments.redirect?id=def-2 \"=300x200\") ![a](/api/attachments.redirect?id=abc-1)";
        assert_eq!(extract_attachment_ids(md), vec!["abc-1", "def-2"]);
    }

    #[test]
    fn sha256_is_lowercase_hex() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
