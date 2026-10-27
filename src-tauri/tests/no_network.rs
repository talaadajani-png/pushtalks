//! PRD §10: fail the build if networking code appears anywhere except models/download.rs.

use std::fs;
use std::path::Path;

const ALLOWED: &str = "src/models/download.rs";
const PATTERNS: &[&str] = &[
    "ureq::",
    "reqwest",
    "hyper::",
    "TcpStream",
    "UdpSocket",
    "std::net",
    "tokio::net",
    "http::",
];

fn scan(dir: &Path, root: &Path, hits: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("read dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            scan(&path, root, hits);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let rel = path
                .strip_prefix(root)
                .expect("prefix")
                .to_string_lossy()
                .replace('\\', "/");
            if rel == ALLOWED {
                continue;
            }
            let text = fs::read_to_string(&path).expect("read");
            for p in PATTERNS {
                if text.contains(p) {
                    hits.push(format!("{rel}: {p}"));
                }
            }
        }
    }
}

#[test]
fn only_the_model_downloader_uses_the_network() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut hits = Vec::new();
    scan(&root.join("src"), root, &mut hits);
    scan(&root.join("../scripts"), root, &mut hits);
    assert!(hits.is_empty(), "network code outside {ALLOWED}: {hits:?}");
}
