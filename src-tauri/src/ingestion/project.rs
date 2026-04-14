//! Project Structure Understanding — detect project types, extract architecture.
#![allow(dead_code)]

use std::path::Path;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStructure {
    pub name: String, pub project_type: String, pub tech_stack: Vec<String>,
    pub entry_points: Vec<String>, pub key_directories: Vec<String>,
    pub file_count: u64, pub total_size_kb: u64,
}

pub fn detect_project_type(root: &Path) -> (&'static str, Vec<String>) {
    let mut tech_stack = Vec::new();
    let mut project_type = "unknown";
    if root.join("Cargo.toml").exists() {
        project_type = "rust"; tech_stack.push("Rust".to_string());
        if root.join("src-tauri").exists() || root.join("tauri.conf.json").exists() { project_type = "tauri"; tech_stack.push("Tauri".to_string()); }
    }
    if root.join("package.json").exists() {
        if let Ok(pkg) = std::fs::read_to_string(root.join("package.json")) {
            if pkg.contains("\"react\"") { tech_stack.push("React".to_string()); }
            if pkg.contains("\"next\"") { project_type = "nextjs"; tech_stack.push("Next.js".to_string()); }
            if pkg.contains("\"vue\"") { project_type = "vue"; tech_stack.push("Vue".to_string()); }
            if pkg.contains("\"svelte\"") { project_type = "svelte"; tech_stack.push("Svelte".to_string()); }
            if pkg.contains("\"express\"") { tech_stack.push("Express".to_string()); }
            if pkg.contains("\"tailwindcss\"") { tech_stack.push("Tailwind".to_string()); }
            if pkg.contains("\"prisma\"") { tech_stack.push("Prisma".to_string()); }
            if pkg.contains("\"typescript\"") || pkg.contains("\"ts-node\"") { tech_stack.push("TypeScript".to_string()); }
            if pkg.contains("\"three\"") { tech_stack.push("Three.js".to_string()); }
        }
        if project_type == "unknown" || project_type == "rust" {
            if project_type == "rust" { project_type = "tauri"; } else { project_type = "node"; }
        }
    }
    if root.join("pyproject.toml").exists() || root.join("setup.py").exists() || root.join("requirements.txt").exists() {
        project_type = "python"; tech_stack.push("Python".to_string());
        if let Ok(reqs) = std::fs::read_to_string(root.join("requirements.txt")) {
            if reqs.contains("django") { tech_stack.push("Django".to_string()); }
            if reqs.contains("fastapi") { tech_stack.push("FastAPI".to_string()); }
            if reqs.contains("flask") { tech_stack.push("Flask".to_string()); }
        }
    }
    if root.join("go.mod").exists() { project_type = "go"; tech_stack.push("Go".to_string()); }
    if root.join("Dockerfile").exists() || root.join("docker-compose.yml").exists() { tech_stack.push("Docker".to_string()); }
    (project_type, tech_stack)
}

pub fn analyze_structure(root: &Path) -> ProjectStructure {
    let name = root.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();
    let (project_type, tech_stack) = detect_project_type(root);
    let mut entry_points = Vec::new();
    for ep in &["src/main.ts", "src/main.tsx", "src/index.ts", "src/index.tsx", "src/App.tsx", "src/app.tsx", "src/lib.rs", "src/main.rs", "main.py", "app.py", "main.go", "index.js", "server.js"] {
        if root.join(ep).exists() { entry_points.push(ep.to_string()); }
    }
    let mut key_dirs = Vec::new();
    for d in &["src", "src-tauri", "lib", "app", "pages", "components", "api", "server", "public", "tests", "scripts", "config"] {
        if root.join(d).is_dir() { key_dirs.push(d.to_string()); }
    }
    let mut file_count = 0u64; let mut total_size = 0u64;
    count_project_files(root, &mut file_count, &mut total_size, 0);
    ProjectStructure { name, project_type: project_type.to_string(), tech_stack, entry_points, key_directories: key_dirs, file_count, total_size_kb: total_size / 1024 }
}

fn count_project_files(dir: &Path, count: &mut u64, size: &mut u64, depth: u32) {
    if depth > 5 { return; }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') || name == "node_modules" || name == "target" || name == "__pycache__" || name == "dist" || name == "build" { continue; }
            if path.is_dir() { count_project_files(&path, count, size, depth + 1); } else { *count += 1; *size += entry.metadata().map(|m| m.len()).unwrap_or(0); }
        }
    }
}

pub fn generate_architecture_summary(structure: &ProjectStructure) -> String {
    let mut summary = format!("{} is a {} project using {}.", structure.name, structure.project_type,
        if structure.tech_stack.is_empty() { "unknown technologies".to_string() } else { structure.tech_stack.join(", ") });
    if !structure.entry_points.is_empty() { summary.push_str(&format!(" Entry points: {}.", structure.entry_points.join(", "))); }
    if !structure.key_directories.is_empty() { summary.push_str(&format!(" Key directories: {}.", structure.key_directories.join(", "))); }
    summary.push_str(&format!(" Contains {} files ({} KB total).", structure.file_count, structure.total_size_kb));
    summary
}
