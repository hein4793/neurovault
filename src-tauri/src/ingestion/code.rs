//! Code ingestion — turn project source files into structured code_snippet nodes.

use crate::db::models::{CreateNodeInput, GraphNode, NODE_TYPE_CODE_SNIPPET};
use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CodeSnippet {
    pub kind: String,
    pub name: String,
    pub language: String,
    pub file_path: String,
    pub line_number: usize,
    pub source: String,
}

pub async fn ingest_project_directory(
    db: &BrainDb,
    project_path: &str,
    max_files: usize,
) -> Result<Vec<GraphNode>, BrainError> {
    let root = PathBuf::from(project_path);
    if !root.exists() { return Err(BrainError::NotFound(format!("path not found: {}", project_path))); }
    if !root.is_dir() { return Err(BrainError::Internal(format!("not a directory: {}", project_path))); }

    let project_name = root.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();
    log::info!("ingest_project_directory: scanning {}", root.display());

    let mut files: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(&root).follow_links(false).into_iter().filter_entry(|e| !is_skipped_dir(e.path())) {
        let entry = match entry { Ok(e) => e, Err(_) => continue };
        if !entry.file_type().is_file() { continue; }
        if detect_language(entry.path()).is_some() {
            files.push(entry.path().to_path_buf());
            if files.len() >= max_files { break; }
        }
    }

    log::info!("ingest_project_directory: found {} code files", files.len());

    let mut all_snippets: Vec<CodeSnippet> = Vec::new();
    for file in &files {
        let lang = match detect_language(file) { Some(l) => l, None => continue };
        let content = match std::fs::read_to_string(file) { Ok(c) => c, Err(_) => continue };
        if content.len() > 500_000 { continue; }
        let snippets = match lang {
            "rust" => extract_rust(&content, file),
            "typescript" | "javascript" => extract_ts_js(&content, file, lang),
            "python" => extract_python(&content, file),
            "go" => extract_go(&content, file),
            _ => Vec::new(),
        };
        all_snippets.extend(snippets);
    }

    log::info!("ingest_project_directory: extracted {} snippets", all_snippets.len());

    let mut created: Vec<GraphNode> = Vec::with_capacity(all_snippets.len());
    let mut prev_id_per_file: std::collections::HashMap<PathBuf, String> = std::collections::HashMap::new();

    for snip in &all_snippets {
        let title = format!("{} {} ({})", snip.kind, snip.name, snip.language);
        let topic = project_name.to_lowercase().replace(' ', "-");
        let tags = vec!["code".to_string(), snip.language.clone(), snip.kind.clone(), project_name.clone()];
        let content = format!("{} `{}` in `{}`\n\n```{}\n{}\n```", snip.kind, snip.name, snip.file_path, snip.language, snip.source);

        let input = CreateNodeInput {
            title, content, domain: "technology".into(), topic, tags,
            node_type: NODE_TYPE_CODE_SNIPPET.to_string(), source_type: "project".into(), source_url: None,
        };

        match db.create_node(input).await {
            Ok(node) => {
                // Stamp source_file
                let id = node.id.clone();
                let file_path_str = snip.file_path.clone();
                let _ = db.with_conn(move |conn| {
                    conn.execute("UPDATE nodes SET source_file = ?1 WHERE id = ?2", params![file_path_str, id])
                        .map_err(|e| BrainError::Database(e.to_string()))
                }).await;

                // part_of edge to previous snippet from the same file
                let file_path = PathBuf::from(&snip.file_path);
                if let Some(prev) = prev_id_per_file.get(&file_path) {
                    let src = node.id.clone();
                    let tgt = prev.clone();
                    let now = chrono::Utc::now().to_rfc3339();
                    let _ = db.with_conn(move |conn| {
                        let edge_id = format!("edges:{}", uuid::Uuid::now_v7());
                        conn.execute(
                            "INSERT OR IGNORE INTO edges (id, source_id, target_id, relation_type, strength, \
                             discovered_by, evidence, animated, created_at, traversal_count) \
                             VALUES (?1, ?2, ?3, 'part_of', 0.9, 'code_ingest', 'Sequential snippets in same file', 0, ?4, 0)",
                            params![edge_id, src, tgt, now],
                        ).map_err(|e| BrainError::Database(e.to_string()))
                    }).await;
                }
                prev_id_per_file.insert(file_path, node.id.clone());
                created.push(node);
            }
            Err(e) => { log::debug!("skip code snippet (probably dup): {}", e); }
        }
    }

    Ok(created)
}

fn detect_language(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    match ext.as_str() {
        "rs" => Some("rust"), "ts" | "tsx" => Some("typescript"),
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        "py" => Some("python"), "go" => Some("go"), _ => None,
    }
}

fn is_skipped_dir(path: &Path) -> bool {
    let name = match path.file_name().and_then(|s| s.to_str()) { Some(n) => n, None => return false };
    matches!(name, "node_modules" | "target" | ".git" | ".next" | ".nuxt" | "dist" | "build" | "__pycache__"
        | ".venv" | "venv" | ".cache" | "vendor" | ".idea" | ".vscode")
        || name.starts_with('.') && path.is_dir() && name != "."
}

const MAX_SNIPPET_LINES: usize = 80;

fn take_snippet_body(lines: &[&str], start: usize, _is_block_end: impl Fn(&str) -> bool) -> String {
    let mut body = String::new();
    let end = (start + MAX_SNIPPET_LINES).min(lines.len());
    let mut depth = 0i32; let mut closed = false;
    for i in start..end {
        let line = lines[i]; body.push_str(line); body.push('\n');
        for ch in line.chars() { if ch == '{' { depth += 1; } else if ch == '}' { depth -= 1; if depth == 0 && i > start { closed = true; } } }
        if closed { break; }
    }
    body
}

fn extract_rust(content: &str, file: &Path) -> Vec<CodeSnippet> {
    let lines: Vec<&str> = content.lines().collect();
    let mut snippets = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") { continue; }
        let (kind, name) = if let Some(rest) = trimmed.strip_prefix("pub fn ").or_else(|| trimmed.strip_prefix("fn ")) { ("function", extract_ident(rest)) }
        else if let Some(rest) = trimmed.strip_prefix("pub async fn ").or_else(|| trimmed.strip_prefix("async fn ")) { ("function", extract_ident(rest)) }
        else if let Some(rest) = trimmed.strip_prefix("pub struct ").or_else(|| trimmed.strip_prefix("struct ")) { ("struct", extract_ident(rest)) }
        else if let Some(rest) = trimmed.strip_prefix("pub enum ").or_else(|| trimmed.strip_prefix("enum ")) { ("enum", extract_ident(rest)) }
        else if let Some(rest) = trimmed.strip_prefix("pub trait ").or_else(|| trimmed.strip_prefix("trait ")) { ("trait", extract_ident(rest)) }
        else if let Some(rest) = trimmed.strip_prefix("impl ") { ("impl", extract_ident(rest)) }
        else { continue; };
        let name = match name { Some(n) if !n.is_empty() => n, _ => continue };
        let body = take_snippet_body(&lines, i, |_| false);
        snippets.push(CodeSnippet { kind: kind.to_string(), name, language: "rust".to_string(), file_path: file.to_string_lossy().to_string(), line_number: i + 1, source: body });
    }
    snippets
}

fn extract_ts_js(content: &str, file: &Path, lang: &str) -> Vec<CodeSnippet> {
    let lines: Vec<&str> = content.lines().collect();
    let mut snippets = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") || trimmed.starts_with("*") { continue; }
        let (kind, name) = if let Some(rest) = trimmed.strip_prefix("export function ").or_else(|| trimmed.strip_prefix("function "))
            .or_else(|| trimmed.strip_prefix("export async function ")).or_else(|| trimmed.strip_prefix("async function "))
            .or_else(|| trimmed.strip_prefix("export default function ")) { ("function", extract_ident(rest)) }
        else if let Some(rest) = trimmed.strip_prefix("export class ").or_else(|| trimmed.strip_prefix("class "))
            .or_else(|| trimmed.strip_prefix("export default class ")) { ("class", extract_ident(rest)) }
        else if let Some(rest) = trimmed.strip_prefix("export interface ").or_else(|| trimmed.strip_prefix("interface ")) { ("interface", extract_ident(rest)) }
        else if let Some(rest) = trimmed.strip_prefix("export type ").or_else(|| trimmed.strip_prefix("type ")) {
            if !rest.contains('=') { continue; } ("type", extract_ident(rest))
        } else if (trimmed.starts_with("export const ") || trimmed.starts_with("const ")) && (trimmed.contains("=> ") || trimmed.contains("function")) {
            let rest = trimmed.trim_start_matches("export ").trim_start_matches("const ");
            ("function", extract_ident(rest))
        } else { continue; };
        let name = match name { Some(n) if !n.is_empty() => n, _ => continue };
        let body = take_snippet_body(&lines, i, |_| false);
        snippets.push(CodeSnippet { kind: kind.to_string(), name, language: lang.to_string(), file_path: file.to_string_lossy().to_string(), line_number: i + 1, source: body });
    }
    snippets
}

fn extract_python(content: &str, file: &Path) -> Vec<CodeSnippet> {
    let lines: Vec<&str> = content.lines().collect();
    let mut snippets = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if indent > 4 { continue; }
        let (kind, name) = if let Some(rest) = trimmed.strip_prefix("def ").or_else(|| trimmed.strip_prefix("async def ")) { ("function", extract_ident_until(rest, '(')) }
        else if let Some(rest) = trimmed.strip_prefix("class ") { ("class", extract_ident_until(rest, '(')) }
        else { continue; };
        let name = match name { Some(n) if !n.is_empty() => n, _ => continue };
        let body = take_python_body(&lines, i, indent);
        snippets.push(CodeSnippet { kind: kind.to_string(), name, language: "python".to_string(), file_path: file.to_string_lossy().to_string(), line_number: i + 1, source: body });
    }
    snippets
}

fn take_python_body(lines: &[&str], start: usize, base_indent: usize) -> String {
    let mut body = String::new();
    let end = (start + MAX_SNIPPET_LINES).min(lines.len());
    body.push_str(lines[start]); body.push('\n');
    for i in (start + 1)..end {
        let line = lines[i];
        if line.trim().is_empty() { body.push('\n'); continue; }
        let indent = line.len() - line.trim_start().len();
        if indent <= base_indent { break; }
        body.push_str(line); body.push('\n');
    }
    body
}

fn extract_go(content: &str, file: &Path) -> Vec<CodeSnippet> {
    let lines: Vec<&str> = content.lines().collect();
    let mut snippets = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") { continue; }
        let (kind, name) = if let Some(rest) = trimmed.strip_prefix("func ") { ("function", extract_ident(rest)) }
        else if let Some(rest) = trimmed.strip_prefix("type ") {
            if rest.contains("struct") || rest.contains("interface") {
                let k = if rest.contains("interface") { "interface" } else { "struct" };
                (k, extract_ident(rest))
            } else { continue; }
        } else { continue; };
        let name = match name { Some(n) if !n.is_empty() => n, _ => continue };
        let body = take_snippet_body(&lines, i, |_| false);
        snippets.push(CodeSnippet { kind: kind.to_string(), name, language: "go".to_string(), file_path: file.to_string_lossy().to_string(), line_number: i + 1, source: body });
    }
    snippets
}

fn extract_ident(rest: &str) -> Option<String> {
    let mut name = String::new();
    for ch in rest.chars() { if ch.is_alphanumeric() || ch == '_' { name.push(ch); } else { break; } }
    if name.is_empty() { None } else { Some(name) }
}

fn extract_ident_until(rest: &str, stop: char) -> Option<String> {
    let mut name = String::new();
    for ch in rest.chars() { if ch == stop || ch.is_whitespace() || ch == ':' { break; } if ch.is_alphanumeric() || ch == '_' { name.push(ch); } }
    if name.is_empty() { None } else { Some(name) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn detects_languages() { assert_eq!(detect_language(Path::new("foo.rs")), Some("rust")); assert_eq!(detect_language(Path::new("README.md")), None); }
    #[test] fn extracts_rust_function() { let snips = extract_rust("pub fn hello(name: &str) -> String {\n    format!(\"hi {}\", name)\n}\n", Path::new("test.rs")); assert_eq!(snips.len(), 1); assert_eq!(snips[0].name, "hello"); }
    #[test] fn extracts_rust_struct() { let snips = extract_rust("pub struct Brain {\n    nodes: Vec<Node>,\n}\n", Path::new("test.rs")); assert_eq!(snips.len(), 1); assert_eq!(snips[0].name, "Brain"); }
    #[test] fn extracts_python_function() { let snips = extract_python("def my_func(x):\n    return x * 2\n\ndef other():\n    pass\n", Path::new("test.py")); assert_eq!(snips.len(), 2); }
    #[test] fn skips_dot_dirs() { assert!(is_skipped_dir(Path::new("project/node_modules"))); assert!(!is_skipped_dir(Path::new("project/src"))); }
}
