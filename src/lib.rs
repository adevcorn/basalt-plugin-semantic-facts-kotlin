//! Semantic-facts-kotlin — parser-derived semantic facts for Kotlin.
//!
//! Provides `semantic-facts@kotlin/v1` by consuming:
//! - `parse.call-sites@kt/v1`
//! - `parse.retrieval@kt/v1`
//!
//! Request format from core (capability handle):
//! `[src_len u32][src][path_len u32][path][root_len u32][root]`

#![no_std]
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

#[cfg(target_arch = "wasm32")]
use basalt_plugin_sdk::facts::serialize_facts;
use basalt_plugin_sdk::facts::{IOKind, SemanticFact, SymbolKind};
use basalt_plugin_sdk::prelude::*;

basalt_plugin_meta! {
    name:              "semantic-facts-kotlin",
    version:           env!("CARGO_PKG_VERSION"),
    hook_flags:        CAP_CAPABILITY_HANDLE | CAP_SEMANTIC_FACTS,
    provides:          "semantic-facts@kotlin/v1",
    requires:          "parse.call-sites@kt/v1\nparse.retrieval@kt/v1",
    optional_requires: "",
    file_globs:        "**/*.kt",
    activates_on:      "",
    activation_events: "",
}

// ── Capability handle ───────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn basalt_capability_handle(
    cap_ptr: *const u8,
    cap_len: usize,
    req_ptr: *const u8,
    req_len: usize,
) -> i64 {
    let _ = (cap_ptr, cap_len);
    if req_ptr.is_null() || req_len < 4 {
        return pack_empty();
    }
    let request = unsafe { core::slice::from_raw_parts(req_ptr, req_len) };
    let Some((src, _path)) = parse_request(request) else {
        return pack_error(-1002);
    };
    if src.is_empty() {
        return pack_empty();
    }

    match derive_semantic_facts(src) {
        Ok(facts) => pack_success(facts),
        Err(code) => pack_error(code),
    }
}

/// Decode `[src_len][src][path_len][path][root_len][root]`.
fn parse_request(request: &[u8]) -> Option<(&[u8], &str)> {
    if request.len() < 4 {
        return None;
    }
    let src_len = u32::from_le_bytes([request[0], request[1], request[2], request[3]]) as usize;
    if request.len() < 4 + src_len {
        return None;
    }
    let src = &request[4..4 + src_len];

    let mut pos = 4 + src_len;
    let mut path = "";
    if request.len() >= pos + 4 {
        let len = u32::from_le_bytes([
            request[pos],
            request[pos + 1],
            request[pos + 2],
            request[pos + 3],
        ]) as usize;
        pos += 4;
        if request.len() >= pos + len {
            path = core::str::from_utf8(&request[pos..pos + len]).unwrap_or("");
        }
    }
    Some((src, path))
}

// ── Fact derivation ─────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
fn derive_semantic_facts(src: &[u8]) -> Result<Vec<u8>, i64> {
    let call_sites_raw = invoke_parse_call_sites(src).map_err(|_| -1003)?;
    let retrieval_raw = invoke_parse_retrieval(src).map_err(|_| -1003)?;

    let call_sites = decode_call_sites(&call_sites_raw);
    let retrieval = decode_retrieval(&retrieval_raw);

    let mut facts: Vec<SemanticFact> = Vec::new();
    derive_declarations(&retrieval, src, &mut facts);
    derive_call_edges(&call_sites, &mut facts);
    derive_imports(src, &mut facts);

    Ok(serialize_facts(&facts))
}

#[cfg(target_arch = "wasm32")]
fn invoke_parse_call_sites(src: &[u8]) -> Result<Vec<u8>, i64> {
    let mut request = Vec::with_capacity(4 + src.len() + 4);
    request.extend_from_slice(&(src.len() as u32).to_le_bytes());
    request.extend_from_slice(src);
    request.extend_from_slice(&16384u32.to_le_bytes());
    invoke_capability("parse.call-sites@kt/v1", &request).map_err(|_| -1003)
}

#[cfg(target_arch = "wasm32")]
fn invoke_parse_retrieval(src: &[u8]) -> Result<Vec<u8>, i64> {
    let mut request = Vec::with_capacity(4 + src.len() + 4);
    request.extend_from_slice(&(src.len() as u32).to_le_bytes());
    request.extend_from_slice(src);
    request.extend_from_slice(&8192u32.to_le_bytes());
    invoke_capability("parse.retrieval@kt/v1", &request).map_err(|_| -1003)
}

// ── Parser response decoding ────────────────────────────────────────────────

fn decode_call_sites(data: &[u8]) -> Vec<(u32, &str)> {
    if data.len() < 4 {
        return Vec::new();
    }
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let mut sites = Vec::with_capacity(count);
    let mut pos = 4;
    for _ in 0..count {
        if pos + 68 > data.len() {
            break;
        }
        let offset = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        let name_bytes = &data[pos + 4..pos + 68];
        let nul = name_bytes.iter().position(|&b| b == 0).unwrap_or(64);
        if let Ok(name) = core::str::from_utf8(&name_bytes[..nul]) {
            if !name.is_empty() {
                sites.push((offset, name));
            }
        }
        pos += 68;
    }
    sites
}

fn decode_retrieval(data: &[u8]) -> Vec<(u32, u32, &str, u8)> {
    if data.len() < 4 {
        return Vec::new();
    }
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let mut chunks = Vec::with_capacity(count);
    let mut pos = 4;
    for _ in 0..count {
        if pos + 104 > data.len() {
            break;
        }
        let offset = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        let length =
            u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]);
        let label_bytes = &data[pos + 8..pos + 103];
        let kind = data[pos + 103];
        let nul = label_bytes.iter().position(|&b| b == 0).unwrap_or(95);
        if let Ok(label) = core::str::from_utf8(&label_bytes[..nul]) {
            let label = label.trim();
            if !label.is_empty() && length > 0 {
                chunks.push((offset, length, label, kind));
            }
        }
        pos += 104;
    }
    chunks
}

fn label_name<'a>(label: &'a str, kind: &str) -> &'a str {
    label
        .strip_prefix(kind)
        .map(str::trim)
        .unwrap_or(label)
        .trim()
}

/// Enclosing type scopes for a byte offset, outermost first.
fn scope_chain<'a>(
    scopes: &[(u32, u32, &'a str)],
    offset: u32,
    self_start: u32,
    self_len: u32,
    self_name: &str,
) -> Vec<&'a str> {
    let mut chain: Vec<(u32, &'a str)> = Vec::new();
    for &(start, len, name) in scopes {
        if start == self_start && len == self_len && name == self_name {
            continue;
        }
        if start <= offset && offset < start.saturating_add(len) && !name.is_empty() {
            chain.push((len, name));
        }
    }
    chain.sort_by(|a, b| b.0.cmp(&a.0));
    chain.into_iter().map(|(_, name)| name).collect()
}

fn qualify(package: &str, scope: &[&str], name: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    if !package.is_empty() {
        parts.push(package);
    }
    parts.extend_from_slice(scope);
    parts.push(name);
    if parts.len() <= 1 {
        None
    } else {
        Some(parts.join("::"))
    }
}

fn derive_declarations(
    retrieval: &[(u32, u32, &str, u8)],
    src: &[u8],
    facts: &mut Vec<SemanticFact>,
) {
    let package = package_name(src);
    let type_scopes: Vec<(u32, u32, &str)> = retrieval
        .iter()
        .filter(|(_, _, _, kind)| *kind == 2)
        .map(|(offset, length, label, _)| (*offset, *length, label_name(label, "type")))
        .collect();

    for &(offset, length, label, kind) in retrieval {
        match kind {
            2 => {
                let name = label_name(label, "type");
                if name.is_empty() {
                    continue;
                }
                let scope = scope_chain(&type_scopes, offset, offset, length, name);
                facts.push(SemanticFact::DeclareSymbol {
                    kind: SymbolKind::Type,
                    offset,
                    length,
                    name: name.to_string(),
                    qualified_name: qualify(&package, &scope, name),
                });
                derive_supertypes(src, offset, length, name, facts);
            }
            3 => {
                let name = label_name(label, "function");
                if name.is_empty() {
                    continue;
                }
                let scope = scope_chain(&type_scopes, offset, offset, length, name);
                facts.push(SemanticFact::DeclareSymbol {
                    kind: SymbolKind::Function,
                    offset,
                    length,
                    name: name.to_string(),
                    qualified_name: qualify(&package, &scope, name),
                });
                if let Some(text) = chunk_text(src, offset, length) {
                    if text.contains("suspend ") {
                        facts.push(SemanticFact::AsyncBoundary {
                            boundary_kind: basalt_plugin_sdk::facts::AsyncBoundaryKind::Await,
                            offset,
                            length: 7,
                        });
                    }
                    if let Some(idx) = text.find("@Composable") {
                        facts.push(SemanticFact::ReferenceSymbol {
                            offset: offset + idx as u32,
                            length: "@Composable".len() as u32,
                            target: "androidx.compose.runtime.Composable".to_string(),
                        });
                    }
                }
            }
            5 => {
                let name = label_name(label, "property");
                if name.is_empty() {
                    continue;
                }
                let scope = scope_chain(&type_scopes, offset, offset, length, name);
                facts.push(SemanticFact::DeclareSymbol {
                    kind: SymbolKind::Property,
                    offset,
                    length,
                    name: name.to_string(),
                    qualified_name: qualify(&package, &scope, name),
                });
            }
            _ => {}
        }
    }
}

fn chunk_text(src: &[u8], offset: u32, length: u32) -> Option<&str> {
    let start = offset as usize;
    let end = start.checked_add(length as usize)?;
    if end > src.len() {
        return None;
    }
    core::str::from_utf8(&src[start..end]).ok()
}

/// Parse `class Foo(...) : Bar(), Baz` headers into `Extends` facts.
fn derive_supertypes(
    src: &[u8],
    offset: u32,
    length: u32,
    child: &str,
    facts: &mut Vec<SemanticFact>,
) {
    let Some(text) = chunk_text(src, offset, length) else {
        return;
    };
    let head = text.split('{').next().unwrap_or(text);
    let Some(rest) = delegation_clause(head) else {
        return;
    };
    for part in rest.split(',') {
        let parent = part
            .trim()
            .trim_start_matches(|c: char| c == '\'' || c.is_whitespace())
            .split(|c: char| c == '(' || c == '<' || c.is_whitespace())
            .next()
            .unwrap_or("")
            .trim();
        if parent.is_empty() || parent == "constructor" || parent == "where" {
            continue;
        }
        facts.push(SemanticFact::Extends {
            offset,
            child: child.to_string(),
            parent: parent.to_string(),
        });
    }
}

/// Return the delegation clause after the top-level `:` in a class header.
/// Tracks parentheses/generics so primary-constructor parameter types
/// (`private val repo: Repository`) are not mistaken for the delegation colon.
fn delegation_clause(head: &str) -> Option<&str> {
    let mut depth: i32 = 0;
    let mut in_string = false;
    for (idx, ch) in head.char_indices() {
        match ch {
            '"' => in_string = !in_string,
            '(' | '[' | '<' if !in_string => depth += 1,
            ')' | ']' | '>' if !in_string => depth -= 1,
            ':' if !in_string && depth == 0 => return Some(&head[idx + 1..]),
            _ => {}
        }
    }
    None
}

fn derive_call_edges(call_sites: &[(u32, &str)], facts: &mut Vec<SemanticFact>) {
    for &(offset, callee) in call_sites {
        let length = callee.len() as u32;
        facts.push(SemanticFact::Calls {
            caller_offset: offset,
            caller_length: length,
            callee: callee.to_string(),
        });
        if let Some(io_kind) = classify_io(callee) {
            facts.push(SemanticFact::IOOperation {
                io_kind,
                offset,
                length,
                descriptor: Some(callee.to_string()),
            });
        }
        let tail = callee.rsplit('.').next().unwrap_or(callee);
        match tail {
            "launch" | "async" | "withContext" | "runBlocking" => {
                facts.push(SemanticFact::AsyncBoundary {
                    boundary_kind: basalt_plugin_sdk::facts::AsyncBoundaryKind::Spawn,
                    offset,
                    length,
                });
            }
            "await" | "awaitAll" | "join" => {
                facts.push(SemanticFact::AsyncBoundary {
                    boundary_kind: basalt_plugin_sdk::facts::AsyncBoundaryKind::Await,
                    offset,
                    length,
                });
            }
            _ => {}
        }
    }
}

fn classify_io(callee: &str) -> Option<IOKind> {
    let lower = callee.to_ascii_lowercase();
    let tail = lower.rsplit('.').next().unwrap_or(&lower);

    if lower.contains("okhttp")
        || lower.contains("httpurlconnection")
        || lower.contains("httpclient")
        || lower.contains("retrofit")
        || matches!(tail, "get" | "post" | "execute")
            && (lower.contains("client") || lower.contains("request"))
    {
        Some(IOKind::Network)
    } else if lower.contains("fileinputstream")
        || lower.contains("readtext")
        || lower.contains("readbytes")
        || lower.contains("bufferedreader")
        || tail == "read"
    {
        Some(IOKind::FileRead)
    } else if lower.contains("fileoutputstream")
        || lower.contains("writetext")
        || lower.contains("writebytes")
        || lower.contains("bufferedwriter")
        || tail == "write"
    {
        Some(IOKind::FileWrite)
    } else if lower.contains("system.out")
        || lower.contains("println")
        || lower.starts_with("log.")
        || lower.contains("android.util.log")
    {
        Some(IOKind::StdIO)
    } else {
        None
    }
}

/// Extract `package a.b` and `import a.b.C [as D]` / `import a.b.*`.
fn package_name(src: &[u8]) -> String {
    let text = String::from_utf8_lossy(src);
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("package ") {
            return rest
                .split(|c: char| c == ';' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
        }
    }
    String::new()
}

fn derive_imports(src: &[u8], facts: &mut Vec<SemanticFact>) {
    let text = String::from_utf8_lossy(src);
    let mut line_offset = 0u32;
    for line in text.lines() {
        let line_len = line.len() as u32 + 1;
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("import ") {
            let rest = rest.trim();
            let module = rest
                .split(|c: char| c.is_whitespace() || c == ';')
                .next()
                .unwrap_or("")
                .trim();
            if !module.is_empty() {
                let alias = rest
                    .split_once(" as ")
                    .map(|(_, alias)| {
                        alias
                            .trim()
                            .trim_end_matches(';')
                            .split_whitespace()
                            .next()
                            .unwrap_or("")
                            .to_string()
                    })
                    .filter(|alias| !alias.is_empty());
                let offset =
                    line_offset + (line.len() - trimmed.len()) as u32 + "import ".len() as u32;
                facts.push(SemanticFact::ImportModule {
                    offset,
                    length: module.len() as u32,
                    module_path: module.to_string(),
                    alias,
                });
                facts.push(SemanticFact::DependsOn {
                    offset,
                    length: module.len() as u32,
                    dependency: module.to_string(),
                });
            }
        }
        line_offset += line_len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    const SOURCE: &str = r#"package com.adevcorn.addhelper

import androidx.lifecycle.ViewModel
import kotlinx.coroutines.flow.StateFlow as Flow

class AddHelperViewModel(
    private val repository: AddHelperRepository,
) : ViewModel(), ItemRepository {
    private val _state = MutableStateFlow(UiState())
    val state: Flow<UiState> = _state

    suspend fun refresh() {
        viewModelScope.launch {
            repository.loadItems()
        }
    }

    @Composable
    fun StatusBadge(text: String) {
        Text(text)
    }
}

interface ItemRepository {
    suspend fun loadItems(): Result<List<String>>
}
"#;

    fn fixture() -> Vec<(u32, u32, &'static str, u8)> {
        let src = SOURCE;
        let class_start = src.find("class AddHelperViewModel").unwrap() as u32;
        let iface_start = src.find("interface ItemRepository").unwrap() as u32;
        let refresh = src.find("suspend fun refresh").unwrap() as u32;
        let state = src.find("val state:").unwrap() as u32;
        let badge = src.find("@Composable").unwrap() as u32;
        let load = src.find("suspend fun loadItems").unwrap() as u32;
        vec![
            (
                class_start,
                iface_start - class_start,
                "type AddHelperViewModel",
                2,
            ),
            (refresh, 40, "function refresh", 3),
            (state, 20, "property state", 5),
            (badge, 60, "function StatusBadge", 3),
            (
                iface_start,
                src.len() as u32 - iface_start,
                "type ItemRepository",
                2,
            ),
            (load, 40, "function loadItems", 3),
        ]
    }

    #[test]
    fn declarations_carry_package_and_type_scope() {
        let mut facts = Vec::new();
        derive_declarations(&fixture(), SOURCE.as_bytes(), &mut facts);

        let qualified_name = |name: &str| {
            facts.iter().find_map(|fact| match fact {
                SemanticFact::DeclareSymbol {
                    name: n,
                    qualified_name,
                    ..
                } if n == name => qualified_name.clone(),
                _ => None,
            })
        };

        assert_eq!(
            qualified_name("AddHelperViewModel"),
            Some("com.adevcorn.addhelper::AddHelperViewModel".to_string())
        );
        assert_eq!(
            qualified_name("refresh"),
            Some("com.adevcorn.addhelper::AddHelperViewModel::refresh".to_string())
        );
        assert_eq!(
            qualified_name("state"),
            Some("com.adevcorn.addhelper::AddHelperViewModel::state".to_string())
        );
        assert_eq!(
            qualified_name("loadItems"),
            Some("com.adevcorn.addhelper::ItemRepository::loadItems".to_string())
        );
    }

    #[test]
    fn class_headers_emit_extends_facts() {
        let mut facts = Vec::new();
        derive_declarations(&fixture(), SOURCE.as_bytes(), &mut facts);
        let parents: Vec<&str> = facts
            .iter()
            .filter_map(|fact| match fact {
                SemanticFact::Extends { child, parent, .. } if child == "AddHelperViewModel" => {
                    Some(parent.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(parents, vec!["ViewModel", "ItemRepository"]);
    }

    #[test]
    fn suspend_and_composable_are_recorded() {
        let mut facts = Vec::new();
        derive_declarations(&fixture(), SOURCE.as_bytes(), &mut facts);
        assert!(facts
            .iter()
            .any(|f| matches!(f, SemanticFact::AsyncBoundary { .. })));
        assert!(facts.iter().any(|f| matches!(
            f,
            SemanticFact::ReferenceSymbol { target, .. }
                if target == "androidx.compose.runtime.Composable"
        )));
    }

    #[test]
    fn imports_emit_module_alias_and_dependency_facts() {
        let mut facts = Vec::new();
        derive_imports(SOURCE.as_bytes(), &mut facts);
        assert!(facts.iter().any(|f| matches!(
            f,
            SemanticFact::ImportModule { module_path, alias: None, .. }
                if module_path == "androidx.lifecycle.ViewModel"
        )));
        assert!(facts.iter().any(|f| matches!(
            f,
            SemanticFact::ImportModule { module_path, alias: Some(alias), .. }
                if module_path == "kotlinx.coroutines.flow.StateFlow" && alias == "Flow"
        )));
        assert!(facts.iter().any(|f| matches!(
            f,
            SemanticFact::DependsOn { dependency, .. }
                if dependency == "kotlinx.coroutines.flow.StateFlow"
        )));
    }

    #[test]
    fn package_is_extracted_from_source() {
        assert_eq!(package_name(SOURCE.as_bytes()), "com.adevcorn.addhelper");
        assert_eq!(package_name(b"// no package\n"), "");
    }

    #[test]
    fn calls_classify_io_and_coroutine_boundaries() {
        let sites = vec![
            (0u32, "viewModelScope.launch"),
            (10u32, "job.await"),
            (20u32, "println"),
            (30u32, "okhttpClient.newCall"),
            (40u32, "FileInputStream.read"),
            (50u32, "writeText"),
        ];
        let mut facts = Vec::new();
        derive_call_edges(&sites, &mut facts);

        let kinds: Vec<IOKind> = facts
            .iter()
            .filter_map(|f| match f {
                SemanticFact::IOOperation { io_kind, .. } => Some(*io_kind),
                _ => None,
            })
            .collect();
        assert!(kinds.contains(&IOKind::Network));
        assert!(kinds.contains(&IOKind::FileRead));
        assert!(kinds.contains(&IOKind::FileWrite));
        assert!(kinds.contains(&IOKind::StdIO));

        let async_kinds: Vec<_> = facts
            .iter()
            .filter_map(|f| match f {
                SemanticFact::AsyncBoundary { boundary_kind, .. } => Some(*boundary_kind),
                _ => None,
            })
            .collect();
        assert!(async_kinds.contains(&basalt_plugin_sdk::facts::AsyncBoundaryKind::Spawn));
        assert!(async_kinds.contains(&basalt_plugin_sdk::facts::AsyncBoundaryKind::Await));
    }

    #[test]
    fn request_decoder_reads_source_and_path() {
        let src = b"package a";
        let path = b"app/Foo.kt";
        let mut request = Vec::new();
        request.extend_from_slice(&(src.len() as u32).to_le_bytes());
        request.extend_from_slice(src);
        request.extend_from_slice(&(path.len() as u32).to_le_bytes());
        request.extend_from_slice(path);
        let (decoded_src, decoded_path) = parse_request(&request).unwrap();
        assert_eq!(decoded_src, src);
        assert_eq!(decoded_path, "app/Foo.kt");
    }
}
