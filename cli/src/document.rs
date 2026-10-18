//! A whole HTML document around canonical body content: the head the CLI
//! writes from a converter's values, and the body found again in a file.

/// The values the CLI writes into a head.
#[derive(Debug, Default)]
pub struct Head {
    pub title: Option<String>,
    pub lang: Option<String>,
    /// `name`/`content` pairs, in this order.
    pub meta: Vec<(String, String)>,
}

/// Writes `body` (canonical, ending with one line feed, or empty) as the
/// body of a document. The body's bytes stay as they are, so that
/// [`body`] finds them again.
pub fn write(head: &Head, body: &str) -> String {
    let mut out = String::from("<!DOCTYPE html>\n");
    match &head.lang {
        Some(lang) => out.push_str(&format!("<html lang=\"{}\">\n", attr(lang))),
        None => out.push_str("<html>\n"),
    }
    out.push_str("<head>\n<meta charset=\"utf-8\">\n");
    if let Some(title) = &head.title {
        out.push_str(&format!("<title>{}</title>\n", text(title)));
    }
    for (name, content) in &head.meta {
        out.push_str(&format!(
            "<meta name=\"{}\" content=\"{}\">\n",
            attr(name),
            attr(content)
        ));
    }
    out.push_str("</head>\n<body>\n");
    out.push_str(body);
    out.push_str("</body>\n</html>\n");
    out
}

/// The body content of a whole document — one that opens with a doctype
/// or an `html` tag — as the range from after `<body …>` and its line feed
/// to the last `</body>`. `None` for a fragment, or a document without a
/// body tag.
pub fn body(s: &str) -> Option<std::ops::Range<usize>> {
    let lower = s.to_ascii_lowercase();
    let start = lower.trim_start_matches('\u{feff}').trim_start();
    if !start.starts_with("<!doctype") && !start.starts_with("<html") {
        return None;
    }
    let bytes = lower.as_bytes();
    let open = (0..bytes.len()).find(|&i| {
        bytes[i..].starts_with(b"<body")
            && matches!(
                bytes.get(i + 5),
                Some(b'>' | b' ' | b'\t' | b'\n' | b'\r' | b'/')
            )
    })?;
    let mut from = open + lower[open..].find('>')? + 1;
    if lower[from..].starts_with("\r\n") {
        from += 2;
    } else if lower[from..].starts_with('\n') {
        from += 1;
    }
    let to = lower.rfind("</body")?;
    (to >= from).then_some(from..to)
}

fn text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_body_is_found_again() {
        let head = Head {
            title: Some("Eisen & <Ferritin>".into()),
            lang: Some("de".into()),
            meta: vec![("keywords".into(), "labor, \"anämie\"".into())],
        };
        let body_text = "<p>Ferritin erniedrigt.</p>\n";
        let doc = write(&head, body_text);
        assert!(doc.contains("<title>Eisen &amp; &lt;Ferritin&gt;</title>"));
        assert!(doc.contains("content=\"labor, &quot;anämie&quot;\""));
        assert_eq!(&doc[body(&doc).unwrap()], body_text);
        let empty = write(&Head::default(), "");
        assert_eq!(&empty[body(&empty).unwrap()], "");
    }

    #[test]
    fn fragments_have_no_body() {
        assert_eq!(body("<p>Eisen</p>\n"), None);
        assert_eq!(body("<p>&lt;body&gt;</p>\n"), None);
        assert_eq!(body("<!DOCTYPE html>\n<p>Eisen</p>\n"), None);
        let doc = "<!doctype html><BODY class=x>\r\n<p>Eisen</p>\n</BODY>";
        assert_eq!(&doc[body(doc).unwrap()], "<p>Eisen</p>\n");
        assert_eq!(body("<html><bodyx>"), None);
        let doc = "<html><p>anämie</p><body>\n<p>Eisen</p>\n</body>";
        assert_eq!(&doc[body(doc).unwrap()], "<p>Eisen</p>\n");
    }
}
