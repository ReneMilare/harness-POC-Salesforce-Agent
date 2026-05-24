use std::{fs, time::SystemTime};

use vps_rust::agent::rag;

#[test]
fn rag_search_returns_matching_txt_files() {
    let root = temp_docs_dir("rag_search_returns_matching_txt_files");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("manual.txt"),
        "A República de Platão discute justiça e cidade ideal.",
    )
    .unwrap();

    let chunks = rag::search(root.to_str().unwrap(), "justiça em Platão", 3);

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].source, "manual.txt");
    assert!(chunks[0].content.contains("República"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rag_search_ignores_unsupported_files() {
    let root = temp_docs_dir("rag_search_ignores_unsupported_files");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("manual.pdf"), "justiça").unwrap();

    let chunks = rag::search(root.to_str().unwrap(), "justiça", 3);

    assert!(chunks.is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rag_search_scores_whole_words_not_substrings() {
    let root = temp_docs_dir("rag_search_scores_whole_words_not_substrings");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("wrong.txt"),
        "Contemporary attempts mention temporal topics repeatedly.",
    )
    .unwrap();
    fs::write(
        root.join("right.md"),
        "O julgamento de Sócrates durou um dia.",
    )
    .unwrap();

    let chunks = rag::search(
        root.to_str().unwrap(),
        "quanto tempo durou o julgamento de Sócrates?",
        3,
    );

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].source, "right.md");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rag_search_retrieves_downloaded_socrates_apology_text() {
    let chunks = rag::search("./docs", "quanto tempo durou o julgamento de Sócrates?", 3);

    assert!(
        chunks
            .iter()
            .any(|chunk| chunk.source == "apologia_socrates_pt.txt")
    );
    assert!(chunks.iter().all(|chunk| chunk.source != "FONTE.source"));
}

fn temp_docs_dir(test_name: &str) -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{test_name}_{suffix}"))
}
