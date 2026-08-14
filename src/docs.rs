//! Bundled documentation, baked in at compile time.

/// A single documentation page.
pub struct DocPage {
    pub slug: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub raw: &'static str,
}

impl DocPage {
    /// Return the markdown content with the metadata comment stripped.
    pub fn content(&self) -> &str {
        let md = self.raw;
        if let Some(start) = md.find("<!-- metadata") {
            if let Some(end) = md[start..].find("-->") {
                return md[start + end + 3..].trim_start_matches('\n');
            }
        }
        md
    }
}

/// All missouri documentation pages.
pub static PAGES: &[DocPage] = &[
    DocPage {
        slug: "what-is-missouri",
        title: "What is Missouri?",
        description: "Why filesystem state graphs and how missouri's testing model works",
        raw: include_str!("../docs/what-is-missouri.md"),
    },
    DocPage {
        slug: "getting-started",
        title: "Getting Started",
        description: "Create your first state graph test suite",
        raw: include_str!("../docs/getting-started.md"),
    },
    DocPage {
        slug: "writing-tests",
        title: "Writing Tests",
        description: "How to model tests as state graphs with transitions, assertions, and services",
        raw: include_str!("../docs/writing-tests.md"),
    },
    DocPage {
        slug: "cli-reference",
        title: "CLI Reference",
        description: "Complete command reference for the missouri test framework",
        raw: include_str!("../docs/cli-reference.md"),
    },
];

/// Print a listing of all docs to stdout.
pub fn list() {
    print!("{}", format_list(PAGES));
}

/// Print a doc by slug. Returns false if not found.
pub fn show(slug: &str) -> bool {
    if let Some(page) = find(slug) {
        print!("{}", page.content());
        true
    } else {
        false
    }
}

/// Search docs for a query string. Prints matching doc titles.
pub fn search(query: &str) {
    let matches = find_matching(PAGES, query);
    if matches.is_empty() {
        eprintln!("no docs matching '{query}'");
    } else {
        print!("{}", format_list_from_refs(&matches));
    }
}

/// Find a doc page by flexible identifier: exact slug, slug prefix, or
/// case-insensitive title match.
pub fn find(identifier: &str) -> Option<&'static DocPage> {
    find_in(PAGES, identifier)
}

/// Find a doc page in a given slice by flexible identifier.
pub fn find_in<'a>(pages: &'a [DocPage], identifier: &str) -> Option<&'a DocPage> {
    // 1. Exact slug match
    if let Some(page) = pages.iter().find(|p| p.slug == identifier) {
        return Some(page);
    }
    // 2. Case-insensitive slug match
    let lower = identifier.to_lowercase();
    if let Some(page) = pages.iter().find(|p| p.slug.to_lowercase() == lower) {
        return Some(page);
    }
    // 3. Case-insensitive title match
    if let Some(page) = pages.iter().find(|p| p.title.to_lowercase() == lower) {
        return Some(page);
    }
    // 4. Unique slug prefix
    let prefix_matches: Vec<_> = pages
        .iter()
        .filter(|p| p.slug.starts_with(identifier) || p.slug.starts_with(&lower))
        .collect();
    if prefix_matches.len() == 1 {
        return Some(prefix_matches[0]);
    }
    None
}

/// Format a listing of doc pages showing slug (what to type) and description.
pub fn format_list(pages: &[DocPage]) -> String {
    let mut out = String::new();
    for page in pages {
        out.push_str(&format!("  {:<25} {}\n", page.slug, page.description));
    }
    out
}

/// Format a listing from a vec of page references.
pub fn format_list_from_refs(pages: &[&DocPage]) -> String {
    let mut out = String::new();
    for page in pages {
        out.push_str(&format!("  {:<25} {}\n", page.slug, page.description));
    }
    out
}

/// Find all docs matching a query string. Returns matching pages.
pub fn find_matching<'a>(pages: &'a [DocPage], query: &str) -> Vec<&'a DocPage> {
    let q = query.to_lowercase();
    pages
        .iter()
        .filter(|page| {
            page.slug.to_lowercase().contains(&q)
                || page.title.to_lowercase().contains(&q)
                || page.description.to_lowercase().contains(&q)
                || page.content().to_lowercase().contains(&q)
        })
        .collect()
}
