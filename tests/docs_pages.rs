//! The bundled documentation, as the binary prints it. The pages are
//! embedded at build time, and an empty set lists nothing and exits 0,
//! so a test that reads only the exit code stays green when every page
//! is gone. This reads each page back.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_missouri");

/// The page slugs, from the files under docs/.
fn pages() -> Vec<String> {
    let mut slugs: Vec<String> = std::fs::read_dir("docs")
        .expect("docs/")
        .filter_map(|e| {
            let path = e.expect("entry").path();
            (path.extension().is_some_and(|x| x == "md")).then(|| {
                path.file_stem()
                    .expect("stem")
                    .to_string_lossy()
                    .into_owned()
            })
        })
        .collect();
    slugs.sort();
    assert!(!slugs.is_empty(), "no pages under docs/");
    slugs
}

fn docs(args: &[&str]) -> (String, i32) {
    let out = Command::new(BIN)
        .arg("docs")
        .args(args)
        .output()
        .expect("binary runs");
    (
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn the_listing_names_every_page() {
    let (listing, code) = docs(&[]);
    assert_eq!(code, 0, "{listing}");
    for slug in pages() {
        assert!(
            listing.contains(&slug),
            "the listing omits {slug}:\n{listing}"
        );
    }
}

#[test]
fn every_page_prints_in_full() {
    for slug in pages() {
        let (text, code) = docs(&[&slug]);
        assert_eq!(code, 0, "{slug}: {text}");
        let source = std::fs::read_to_string(format!("docs/{slug}.md")).expect("page source");
        assert!(
            text.len() >= source.len() / 2,
            "{slug} printed {} bytes of a {}-byte page",
            text.len(),
            source.len()
        );
    }
}

#[test]
fn search_lists_the_pages_that_carry_the_word_and_no_other() {
    // A hit listing and the full listing print the same shape, so a
    // search that filtered nothing read as a success. A word one page
    // carries and no other tells the two apart.
    let slugs = pages();
    let texts: Vec<(String, String)> = slugs
        .iter()
        .map(|s| {
            let text = std::fs::read_to_string(format!("docs/{s}.md")).expect("page source");
            (s.clone(), text)
        })
        .collect();
    let (only, word) = texts
        .iter()
        .find_map(|(slug, text)| {
            text.split(|c: char| !c.is_ascii_alphabetic())
                .filter(|w| w.len() >= 8 && w.chars().all(|c| c.is_ascii_lowercase()))
                .find(|w| {
                    texts
                        .iter()
                        .all(|(s, t)| s == slug || !t.to_lowercase().contains(*w))
                })
                .map(|w| (slug.clone(), w.to_string()))
        })
        .expect("a word that one page carries and the others do not");
    let (text, code) = docs(&["search", &word]);
    assert_eq!(code, 0, "search for {word}: {text}");
    assert!(
        text.contains(&only),
        "search for {word} omits {only}:\n{text}"
    );
    for slug in &slugs {
        if slug != &only {
            assert!(
                !text.contains(slug.as_str()),
                "search for {word} lists {slug}, which does not carry it:\n{text}"
            );
        }
    }
    let (text, code) = docs(&["search", "zzzznomatch"]);
    assert_ne!(code, 0, "a search with no hit exited 0: {text}");
    assert!(text.contains("no docs matching"), "{text}");
}

#[test]
fn an_unknown_page_is_refused_with_the_listing() {
    let (text, code) = docs(&["no-such-page"]);
    assert_ne!(code, 0, "an unknown page should not exit 0: {text}");
    assert!(
        text.contains("no-such-page"),
        "the refusal should name the page: {text}"
    );
    for slug in pages() {
        assert!(
            text.contains(&slug),
            "the refusal should list {slug}: {text}"
        );
    }
}
