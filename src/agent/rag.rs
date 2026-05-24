use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RagChunk {
    pub source: String,
    pub content: String,
    pub score: usize,
}

const SUPPORTED_EXTENSIONS: &[&str] = &["txt", "md"];
const SNIPPET_RADIUS: usize = 450;

pub fn search(docs_path: &str, query: &str, top_k: usize) -> Vec<RagChunk> {
    let terms = search_terms(query);
    if terms.is_empty() {
        return Vec::new();
    }

    let mut chunks = list_supported_files(Path::new(docs_path))
        .into_iter()
        .filter_map(|path| chunk_for_file(&path, &terms))
        .collect::<Vec<_>>();

    chunks.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.source.cmp(&right.source))
    });
    chunks.truncate(top_k);
    chunks
}

pub fn format_context(chunks: &[RagChunk]) -> String {
    if chunks.is_empty() {
        return "Nenhuma informação relevante encontrada nos documentos locais.".to_string();
    }

    chunks
        .iter()
        .map(|chunk| format!("[Fonte: {}]\n{}", chunk.source, chunk.content))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

fn list_supported_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_supported_files(root, &mut files);
    files
}

fn collect_supported_files(path: &Path, files: &mut Vec<PathBuf>) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };

    if metadata.is_file() {
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| SUPPORTED_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
            .unwrap_or(false)
        {
            files.push(path.to_path_buf());
        }
        return;
    }

    let Ok(entries) = fs::read_dir(path) else {
        return;
    };

    for entry in entries.flatten() {
        collect_supported_files(&entry.path(), files);
    }
}

fn chunk_for_file(path: &Path, terms: &[String]) -> Option<RagChunk> {
    let content = fs::read_to_string(path).ok()?;
    let normalized = normalize(&content);
    let token_counts = token_counts(&normalized);
    let mut score = 0;
    let mut first_match = None;

    for term in terms {
        let matches = token_counts.get(term.as_str()).copied().unwrap_or_default();
        if matches > 0 && first_match.is_none() {
            first_match = normalized.find(term);
        }
        score += matches;
    }

    if score == 0 {
        return None;
    }

    let source = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("documento")
        .to_string();

    Some(RagChunk {
        source,
        content: snippet(&content, first_match.unwrap_or(0)),
        score,
    })
}

fn snippet(content: &str, match_index: usize) -> String {
    let start = nearest_char_boundary(content, match_index.saturating_sub(SNIPPET_RADIUS));
    let end = nearest_char_boundary(content, (match_index + SNIPPET_RADIUS).min(content.len()));

    content[start..end]
        .replace(['\r', '\n'], " ")
        .trim()
        .to_string()
}

fn nearest_char_boundary(content: &str, mut index: usize) -> usize {
    while index > 0 && !content.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn search_terms(query: &str) -> Vec<String> {
    normalize(query)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| term.len() >= 4)
        .filter(|term| !STOP_WORDS.contains(term))
        .map(str::to_string)
        .collect()
}

fn token_counts(normalized: &str) -> HashMap<&str, usize> {
    let mut counts = HashMap::new();
    for token in normalized
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
    {
        *counts.entry(token).or_default() += 1;
    }
    counts
}

fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .replace(['á', 'à', 'ã', 'â'], "a")
        .replace(['é', 'ê'], "e")
        .replace(['í'], "i")
        .replace(['ó', 'ô', 'õ'], "o")
        .replace(['ú'], "u")
        .replace('ç', "c")
}

const STOP_WORDS: &[&str] = &[
    "qual", "quais", "quem", "onde", "como", "sobre", "dados", "traga", "mostre", "fale", "para",
    "pela", "pelo", "essa", "esse", "esta", "este", "isso", "aquela", "aquele", "capital",
    "quanto",
];
