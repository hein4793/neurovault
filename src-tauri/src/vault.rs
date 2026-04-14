#![allow(dead_code)]
//! Obsidian-style vault — plain markdown files for every knowledge node.
//!
//! Each node is written to `~/.neurovault/vault/{domain}/{slug}.md` with
//! YAML frontmatter. Your AI assistant can read these natively without any API call.

use crate::config::BrainConfig;
use crate::db::models::GraphNode;
use std::path::PathBuf;

/// Convert a title to a filesystem-safe slug.
fn slugify(title: &str) -> String {
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else if c == ' ' {
                '-'
            } else {
                '_'
            }
        })
        .collect();
    // Truncate to reasonable filename length
    let slug = if slug.len() > 80 { &slug[..80] } else { &slug };
    // Remove trailing hyphens/underscores
    slug.trim_end_matches(|c| c == '-' || c == '_').to_string()
}

/// Write a knowledge node to the vault as a markdown file.
/// Returns the relative vault path (e.g., "technology/tauri-v2-setup.md").
pub fn write_node_to_vault(config: &BrainConfig, node: &GraphNode, tags: &[String]) -> Option<String> {
    let vault_dir = config.vault_dir();
    let domain_dir = vault_dir.join(&node.domain);

    if let Err(e) = std::fs::create_dir_all(&domain_dir) {
        log::warn!("Failed to create vault domain dir {:?}: {}", domain_dir, e);
        return None;
    }

    let slug = slugify(&node.title);
    if slug.is_empty() {
        return None;
    }

    let filename = format!("{}.md", slug);
    let file_path = domain_dir.join(&filename);
    let relative_path = format!("{}/{}", node.domain, filename);

    let tags_yaml = if tags.is_empty() {
        "[]".to_string()
    } else {
        format!("[{}]", tags.iter().map(|t| format!("\"{}\"", t)).collect::<Vec<_>>().join(", "))
    };

    let frontmatter = format!(
        "---\nid: {}\ndomain: {}\ntopic: {}\ntags: {}\ntype: {}\nsource: {}\nquality: {:.2}\ncreated: {}\n---\n",
        node.id, node.domain, node.topic, tags_yaml,
        node.node_type, node.source_type, 0.7, node.created_at
    );

    let markdown = format!("{}\n# {}\n\n{}\n", frontmatter, node.title, node.content);

    match std::fs::write(&file_path, &markdown) {
        Ok(_) => {
            log::debug!("Vault: wrote {}", relative_path);
            Some(relative_path)
        }
        Err(e) => {
            log::warn!("Vault: failed to write {:?}: {}", file_path, e);
            None
        }
    }
}

/// Read a vault file and return its content (without frontmatter).
pub fn read_vault_file(config: &BrainConfig, relative_path: &str) -> Option<String> {
    let file_path = config.vault_dir().join(relative_path);
    match std::fs::read_to_string(&file_path) {
        Ok(content) => {
            // Strip YAML frontmatter if present
            if content.starts_with("---") {
                if let Some(end) = content[3..].find("---") {
                    return Some(content[end + 6..].trim().to_string());
                }
            }
            Some(content)
        }
        Err(_) => None,
    }
}

/// List all vault files across all domains.
pub fn list_vault_files(config: &BrainConfig) -> Vec<PathBuf> {
    let vault_dir = config.vault_dir();
    let mut files = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&vault_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Ok(domain_entries) = std::fs::read_dir(entry.path()) {
                    for de in domain_entries.flatten() {
                        let path = de.path();
                        if path.extension().map(|e| e == "md").unwrap_or(false) {
                            files.push(path);
                        }
                    }
                }
            }
        }
    }

    files
}
