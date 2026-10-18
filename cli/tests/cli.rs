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
         <title>eisenmangel</title>\n<meta name=\"keywords\" content=\"labor\">\n\
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
