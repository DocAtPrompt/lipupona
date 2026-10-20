//! Runs the `lipupona` binary as a user would: arguments, standard input,
//! files, exit status.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(args: &[&str], stdin: &str) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lipupona"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    Run {
        code: out.status.code().unwrap(),
        stdout: String::from_utf8(out.stdout).unwrap(),
        stderr: String::from_utf8(out.stderr).unwrap(),
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cli");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn notation_and_html_read_each_other() {
    let html = "<h2>Befund</h2>\n<p>Ferritin <em>deutlich</em> erniedrigt.</p>\n";
    let display = run(&["display"], html);
    assert_eq!(display.code, 0);
    assert_eq!(
        display.stdout,
        "## Befund\n\nFerritin *deutlich* erniedrigt.\n"
    );
    let back = run(&["html", "-"], &display.stdout);
    assert_eq!((back.code, back.stdout.as_str()), (0, html));
}

#[test]
fn spans_tile_the_notation() {
    let out = run(&["spans"], "## Befund\nFerritin **tief**\n");
    assert_eq!(out.code, 0);
    assert_eq!(
        out.stdout,
        "markup        0..3     \"## \"\n\
         content       3..9     \"Befund\"\n\
         separator     9..10    \"\\n\"\n\
         content      10..19    \"Ferritin \"\n\
         markup       19..21    \"**\"\n\
         content      21..25    \"tief\"\n\
         markup       25..27    \"**\"\n\
         separator    27..28    \"\\n\"\n"
    );
}

#[test]
fn a_document_reads_back_from_its_front_matter() {
    let notation =
        "---\nlang: de\ntitle: Eisenmangel\ntags: [labor, anämie]\n---\n\n# Eisenmangel\n";
    let html = run(&["html"], notation);
    assert_eq!(html.code, 0);
    assert!(
        html.stdout
            .starts_with("<!DOCTYPE html>\n<html lang=\"de\">\n")
    );
    assert!(
        html.stdout
            .contains("<meta name=\"tags\" content=\"anämie\">\n")
    );
    let back = run(&["display"], &html.stdout);
    assert_eq!((back.code, back.stdout.as_str()), (0, notation));
    assert_eq!(run(&["check"], &html.stdout).code, 0);
}

#[test]
fn canon_reports_diagnostics_on_stderr() {
    let out = run(&["canon"], "<P>Eisen <!--x-->und Folsäure</P>");
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout, "<p>Eisen <!--x-->und Folsäure</p>\n");
    assert_eq!(out.stderr, "<stdin>: comment: x\n");
}

#[test]
fn a_converted_document_passes_the_check() {
    let md = scratch("eisenmangel.md");
    let html = scratch("eisenmangel.html");
    std::fs::write(
        &md,
        "---\nlang: de\ntags: [labor]\n---\nFerritin **erniedrigt**, siehe [[Ferritin]].\n",
    )
    .unwrap();
    let (md, html) = (md.to_str().unwrap(), html.to_str().unwrap());
    let out = run(
        &[
            "from-md",
            "--flavour=obsidian",
            "--document",
            md,
            "-o",
            html,
        ],
        "",
    );
    assert_eq!((out.code, out.stdout.as_str()), (0, ""));
    assert_eq!(out.stderr, format!("{md}: unresolved-link: [[Ferritin]]\n"));
    assert_eq!(
        std::fs::read_to_string(html).unwrap(),
        "<!DOCTYPE html>\n<html lang=\"de\">\n<head>\n<meta charset=\"utf-8\">\n\
         <title>eisenmangel</title>\n<meta name=\"tags\" content=\"labor\">\n\
         </head>\n<body>\n\
         <p>Ferritin <strong>erniedrigt</strong>, siehe [[Ferritin]].</p>\n\
         </body>\n</html>\n"
    );
    let check = run(&["check", html], "");
    assert_eq!((check.code, check.stdout.as_str()), (0, ""));
}

#[test]
fn from_html_writes_the_body() {
    let out = run(&["from-html"], "<div><p><span>Anämie</span></p></div>");
    assert_eq!((out.code, out.stdout.as_str()), (0, "<p>Anämie</p>\n"));
    let page = "<nav><a href=\"/\">Start</a></nav><p>Anämie</p>";
    let out = run(&["from-html", "--article"], page);
    assert_eq!((out.code, out.stdout.as_str()), (0, "<p>Anämie</p>\n"));
    assert_eq!(out.stderr, "<stdin>: stripped-region: nav: Start\n");
}

#[test]
fn check_names_what_keeps_a_file_from_being_stored() {
    let good = scratch("good.html");
    std::fs::write(&good, "<p>Eisen</p>\n").unwrap();
    let good = good.to_str().unwrap();
    let out = run(&["check", good, "-"], "<p>Eisen  <font>x</font></p>\r\n");
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stdout,
        "<stdin>: not canonical\n<stdin>: out-of-profile: element <font>\n"
    );
    let missing = run(&["check", good, "/nonexistent/x.html"], "");
    assert_eq!(missing.code, 2);
    assert!(
        missing
            .stderr
            .starts_with("lipupona: /nonexistent/x.html: ")
    );
}

#[test]
fn usage_errors_exit_with_2() {
    for args in [
        &[][..],
        &["frob"],
        &["canon", "--flavour", "gfm"],
        &["display", "--document"],
        &["from-html", "--definition-lists"],
        &["from-md", "--article"],
        &["from-html", "--vault", "."],
        &["from-md", "--vault", "."],
        &["canon", "--name", "Konsona"],
        &["html", "a", "b"],
        &["check", "-o", "x"],
        &["from-md", "--flavour", "wiki"],
        &["from-md", "-o"],
        &["canon", "--frob"],
    ] {
        let out = run(args, "");
        assert_eq!(out.code, 2, "{args:?}");
        assert!(out.stderr.starts_with("lipupona: "), "{args:?}");
    }
    let help = run(&["--help"], "");
    assert_eq!(help.code, 0);
    assert!(help.stdout.starts_with("lipupona — "));
}

#[test]
fn a_folder_becomes_a_folder_with_links_between_its_pages() {
    let vault = scratch("vault");
    let out = scratch("vault-out");
    let _ = std::fs::remove_dir_all(&vault);
    let _ = std::fs::remove_dir_all(&out);
    for dir in ["Labor", "Befunde", "bilder", ".obsidian"] {
        std::fs::create_dir_all(vault.join(dir)).unwrap();
    }
    let write = |name: &str, text: &str| std::fs::write(vault.join(name), text).unwrap();
    write("Labor/Ferritin.md", "Siehe [[Anämie]].\n");
    write(
        "Befunde/Anämie.md",
        "Ferritin [[Ferritin#Normwerte|erniedrigt]], ![[hb.png]], [[Fehlt]].\n",
    );
    write("bilder/hb.png", "PNG");
    write(".obsidian/app.json", "{}");
    let (v, o) = (vault.to_str().unwrap(), out.to_str().unwrap());
    let run_ = run(&["from-md", "--flavour", "obsidian", v, "-o", o], "");
    assert_eq!(run_.code, 0);
    let anaemie = vault.join("Befunde/Anämie.md");
    assert_eq!(
        run_.stderr,
        format!("{}: unresolved-link: [[Fehlt]]\n", anaemie.display())
    );
    let read = |name: &str| std::fs::read_to_string(out.join(name)).unwrap();
    assert_eq!(
        read("Befunde/Anämie.html"),
        "<p>Ferritin <a href=\"../Labor/Ferritin.html\">erniedrigt</a>, \
         <img alt=\"hb.png\" src=\"../bilder/hb.png\">, [[Fehlt]].</p>\n"
    );
    assert_eq!(
        read("Labor/Ferritin.html"),
        "<p>Siehe <a href=\"../Befunde/An%C3%A4mie.html\">Anämie</a>.</p>\n"
    );
    assert_eq!(read("bilder/hb.png"), "PNG");
    assert!(!out.join(".obsidian").exists());
    assert_eq!(run(&["check", o], "").code, 0);
    let inside = vault.join("out");
    let inside = run(&["from-md", v, "-o", inside.to_str().unwrap()], "");
    assert_eq!(inside.code, 2);
    assert_eq!(run(&["from-md", v], "").code, 2);
    assert_eq!(run(&["display", v], "").code, 2);
}

#[test]
fn a_file_finds_its_links_in_a_vault() {
    let vault = scratch("single-vault");
    let _ = std::fs::remove_dir_all(&vault);
    std::fs::create_dir_all(vault.join("Labor")).unwrap();
    let ferritin = vault.join("Labor/Ferritin.md");
    std::fs::write(&ferritin, "Siehe [[Eisenmangel]] und [[Fehlt]].\n").unwrap();
    std::fs::write(vault.join("Eisenmangel.md"), "Häufig.\n").unwrap();
    let (v, f) = (vault.to_str().unwrap(), ferritin.to_str().unwrap());
    let out = run(&["from-md", "--flavour", "obsidian", "--vault", v, f], "");
    assert_eq!(out.code, 0);
    assert_eq!(
        out.stdout,
        "<p>Siehe <a href=\"../Eisenmangel.html\">Eisenmangel</a> und [[Fehlt]].</p>\n"
    );
    assert_eq!(out.stderr, format!("{f}: unresolved-link: [[Fehlt]]\n"));
    let outside = scratch("outside.md");
    std::fs::write(&outside, "[[Eisenmangel]]\n").unwrap();
    let outside = run(&["from-md", "--vault", v, outside.to_str().unwrap()], "");
    assert_eq!(outside.code, 2);
    assert!(outside.stderr.contains("does not lie in the vault"));
    let folder = run(&["from-md", "--vault", v, v, "-o", "x"], "");
    assert_eq!(folder.code, 2);
}

#[test]
fn name_makes_the_islands_of_an_implementation_its_own() {
    let island = "<svg data-konsona-rendered=\"mermaid\" viewBox=\"0 0 10 10\"></svg>\n";
    let page = scratch("insel.html");
    std::fs::write(&page, island).unwrap();
    let p = page.to_str().unwrap();
    assert_eq!(run(&["check", "--name", "konsona", p], "").code, 0);
    assert_eq!(run(&["check", p], "").code, 1);
    let out = run(&["canon", "--name", "konsona"], island);
    assert_eq!(
        (out.code, out.stdout.as_str(), out.stderr.as_str()),
        (0, island, "")
    );
}
