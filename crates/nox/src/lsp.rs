use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    fmt::Write as _,
    fs,
    hash::{Hash, Hasher},
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
};

use nox_core::{Diagnostic, HostFunctionSignature, LintWarning, Session, Span};

use crate::{
    external_modules_for_manifest, install_lsp_stdlib, load_external_module, manifest::Manifest,
    std_module_source, Runtime, RuntimePermissions,
};

pub fn run_stdio() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    run(stdin.lock(), stdout.lock())
}

pub(crate) fn run(input: impl Read, mut output: impl Write) -> io::Result<()> {
    let mut server = LspServer::default();
    let mut reader = BufReader::new(input);
    while let Some(message) = read_message(&mut reader)? {
        for response in server.handle(&message) {
            write_message(&mut output, &response)?;
        }
        if server.exited {
            break;
        }
    }
    Ok(())
}

#[derive(Default)]
struct LspServer {
    documents: HashMap<String, String>,
    sessions: HashMap<String, Session>,
    symbol_graph_cache: Option<Vec<SymbolGraphSource>>,
    diagnostic_cache: HashMap<String, CachedDiagnostics>,
    document_revision: u64,
    workspace_roots: Vec<PathBuf>,
    shutdown: bool,
    exited: bool,
}

struct CachedDiagnostics {
    revision: u64,
    source_hash: u64,
    response: String,
}

impl LspServer {
    fn handle(&mut self, message: &str) -> Vec<String> {
        let Some(method) = json_string_field(message, "method") else {
            return Vec::new();
        };
        match method.as_str() {
            "initialize" => {
                let Some(id) = json_id(message) else {
                    return Vec::new();
                };
                if let Some(root_uri) = json_string_field(message, "rootUri") {
                    if let Some(root) = file_uri_path(&root_uri) {
                        self.workspace_roots = vec![root];
                        self.clear_symbol_graph_cache();
                        self.clear_diagnostic_cache();
                    }
                }
                vec![format!(
                    "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"capabilities\":{{\"textDocumentSync\":1,\"hoverProvider\":true,\"definitionProvider\":true,\"renameProvider\":{{\"prepareProvider\":true}},\"documentSymbolProvider\":true,\"workspaceSymbolProvider\":true,\"documentFormattingProvider\":true,\"semanticTokensProvider\":{{\"legend\":{{\"tokenTypes\":[\"namespace\",\"type\",\"function\",\"variable\",\"keyword\",\"string\",\"number\",\"comment\"],\"tokenModifiers\":[\"declaration\",\"readonly\",\"async\"]}},\"full\":true}},\"completionProvider\":{{\"triggerCharacters\":[\".\"]}},\"signatureHelpProvider\":{{\"triggerCharacters\":[\"(\",\",\"]}},\"codeActionProvider\":{{\"codeActionKinds\":[\"quickfix\",\"source\",\"source.fixAll.nox\",\"source.format.nox\"]}},\"codeLensProvider\":{{\"resolveProvider\":false}}}}}}}}"
                )]
            }
            "shutdown" => {
                self.shutdown = true;
                let Some(id) = json_id(message) else {
                    return Vec::new();
                };
                vec![format!(
                    "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}"
                )]
            }
            "exit" => {
                self.exited = true;
                Vec::new()
            }
            "textDocument/didOpen" => {
                let Some(uri) = json_string_field(message, "uri") else {
                    return Vec::new();
                };
                let Some(text) = json_string_field(message, "text") else {
                    return Vec::new();
                };
                self.documents.insert(uri, text);
                self.clear_session_caches();
                self.clear_symbol_graph_cache();
                self.bump_document_revision();
                self.publish_all_diagnostics()
            }
            "textDocument/didChange" => {
                let Some(uri) = json_string_field(message, "uri") else {
                    return Vec::new();
                };
                let Some(text) = json_last_string_field(message, "text") else {
                    return Vec::new();
                };
                self.documents.insert(uri, text);
                self.clear_session_caches();
                self.clear_symbol_graph_cache();
                self.bump_document_revision();
                self.publish_all_diagnostics()
            }
            "textDocument/hover" => {
                let (Some(id), Some(uri), Some(line), Some(character)) = (
                    json_id(message),
                    json_string_field(message, "uri"),
                    json_number_field(message, "line"),
                    json_number_field(message, "character"),
                ) else {
                    return Vec::new();
                };
                let response = self.hover_response(&id, &uri, line, character);
                vec![response]
            }
            "textDocument/formatting" => {
                let (Some(id), Some(uri)) = (json_id(message), json_string_field(message, "uri"))
                else {
                    return Vec::new();
                };
                vec![self.formatting_response(&id, &uri)]
            }
            "textDocument/documentSymbol" => {
                let (Some(id), Some(uri)) = (json_id(message), json_string_field(message, "uri"))
                else {
                    return Vec::new();
                };
                vec![self.document_symbol_response(&id, &uri)]
            }
            "textDocument/semanticTokens/full" => {
                let (Some(id), Some(uri)) = (json_id(message), json_string_field(message, "uri"))
                else {
                    return Vec::new();
                };
                vec![self.semantic_tokens_response(&id, &uri)]
            }
            "workspace/symbol" => {
                let Some(id) = json_id(message) else {
                    return Vec::new();
                };
                let query = json_string_field(message, "query").unwrap_or_default();
                vec![self.workspace_symbol_response(&id, &query)]
            }
            "textDocument/codeLens" => {
                let (Some(id), Some(uri)) = (json_id(message), json_string_field(message, "uri"))
                else {
                    return Vec::new();
                };
                vec![self.code_lens_response(&id, &uri)]
            }
            "nox/testDiscovery" => {
                let (Some(id), Some(uri)) = (json_id(message), json_string_field(message, "uri"))
                else {
                    return Vec::new();
                };
                vec![self.test_discovery_response(&id, &uri)]
            }
            "textDocument/definition" => {
                let (Some(id), Some(uri), Some(line), Some(character)) = (
                    json_id(message),
                    json_string_field(message, "uri"),
                    json_number_field(message, "line"),
                    json_number_field(message, "character"),
                ) else {
                    return Vec::new();
                };
                vec![self.definition_response(&id, &uri, line, character)]
            }
            "textDocument/prepareRename" => {
                let (Some(id), Some(uri), Some(line), Some(character)) = (
                    json_id(message),
                    json_string_field(message, "uri"),
                    json_number_field(message, "line"),
                    json_number_field(message, "character"),
                ) else {
                    return Vec::new();
                };
                vec![self.prepare_rename_response(&id, &uri, line, character)]
            }
            "textDocument/rename" => {
                let (Some(id), Some(uri), Some(line), Some(character), Some(new_name)) = (
                    json_id(message),
                    json_string_field(message, "uri"),
                    json_number_field(message, "line"),
                    json_number_field(message, "character"),
                    json_string_field(message, "newName"),
                ) else {
                    return Vec::new();
                };
                vec![self.rename_response(&id, &uri, line, character, &new_name)]
            }
            "textDocument/completion" => {
                let (Some(id), Some(uri), Some(line), Some(character)) = (
                    json_id(message),
                    json_string_field(message, "uri"),
                    json_number_field(message, "line"),
                    json_number_field(message, "character"),
                ) else {
                    return Vec::new();
                };
                vec![self.completion_response(&id, &uri, line, character)]
            }
            "textDocument/signatureHelp" => {
                let (Some(id), Some(uri), Some(line), Some(character)) = (
                    json_id(message),
                    json_string_field(message, "uri"),
                    json_number_field(message, "line"),
                    json_number_field(message, "character"),
                ) else {
                    return Vec::new();
                };
                vec![self.signature_help_response(&id, &uri, line, character)]
            }
            "textDocument/codeAction" => {
                let (Some(id), Some(uri)) = (json_id(message), json_string_field(message, "uri"))
                else {
                    return Vec::new();
                };
                vec![self.code_action_response(&id, &uri)]
            }
            _ => Vec::new(),
        }
    }

    fn clear_session_caches(&mut self) {
        for session in self.sessions.values_mut() {
            session.clear_module_cache();
        }
    }

    fn clear_symbol_graph_cache(&mut self) {
        self.symbol_graph_cache = None;
    }

    fn clear_diagnostic_cache(&mut self) {
        self.diagnostic_cache.clear();
    }

    fn bump_document_revision(&mut self) {
        self.document_revision = self.document_revision.saturating_add(1);
    }

    fn publish_all_diagnostics(&mut self) -> Vec<String> {
        let documents = self
            .documents
            .iter()
            .map(|(uri, text)| (uri.clone(), text.clone()))
            .collect::<Vec<_>>();
        documents
            .into_iter()
            .map(|(uri, text)| self.publish_diagnostics_cached(&uri, &text))
            .collect()
    }

    fn publish_diagnostics_cached(&mut self, uri: &str, source: &str) -> String {
        let source_hash = stable_hash(source);
        if let Some(cached) = self.diagnostic_cache.get(uri) {
            if cached.revision == self.document_revision && cached.source_hash == source_hash {
                return cached.response.clone();
            }
        }
        let response = self.publish_diagnostics(uri, source);
        self.diagnostic_cache.insert(
            uri.to_string(),
            CachedDiagnostics {
                revision: self.document_revision,
                source_hash,
                response: response.clone(),
            },
        );
        response
    }

    fn formatting_response(&self, id: &str, uri: &str) -> String {
        let Some(source) = self.documents.get(uri) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let runtime = Runtime::with_permissions(RuntimePermissions::cli());
        let Ok(formatted) = runtime.format_source(source) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        if formatted == *source {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[]}}");
        }
        let (end_line, end_character) = line_character(source, source.len());
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[{{\"range\":{{\"start\":{{\"line\":0,\"character\":0}},\"end\":{{\"line\":{end_line},\"character\":{end_character}}}}},\"newText\":\"{}\"}}]}}",
            json_escape(&formatted)
        )
    }

    fn code_lens_response(&self, id: &str, uri: &str) -> String {
        let Some(source) = self.documents.get(uri) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[]}}");
        };
        let lenses = top_level_symbols(source)
            .into_iter()
            .filter(|symbol| symbol.kind == 12 && symbol.name.starts_with("test_"))
            .map(|symbol| {
                let (line, character) = line_character(source, symbol.offset);
                let end_character = character + symbol.name.len();
                format!(
                    "{{\"range\":{{\"start\":{{\"line\":{line},\"character\":{character}}},\"end\":{{\"line\":{line},\"character\":{end_character}}}}},\"command\":{{\"title\":\"Run {}\",\"command\":\"nox.runTest\",\"arguments\":[\"{}\",\"{}\"]}}}}",
                    json_escape(&symbol.name),
                    json_escape(uri),
                    json_escape(&symbol.name)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[{lenses}]}}")
    }

    fn test_discovery_response(&self, id: &str, uri: &str) -> String {
        let Some(source) = self.documents.get(uri) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[]}}");
        };
        let tests = top_level_symbols(source)
            .into_iter()
            .filter(|symbol| symbol.kind == 12 && symbol.name.starts_with("test_"))
            .map(|symbol| {
                let (line, character) = line_character(source, symbol.offset);
                let end_character = character + symbol.name.len();
                format!(
                    "{{\"uri\":\"{}\",\"name\":\"{}\",\"range\":{{\"start\":{{\"line\":{line},\"character\":{character}}},\"end\":{{\"line\":{line},\"character\":{end_character}}}}}}}",
                    json_escape(uri),
                    json_escape(&symbol.name)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[{tests}]}}")
    }

    fn document_symbol_response(&self, id: &str, uri: &str) -> String {
        let Some(source) = self.documents.get(uri) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[]}}");
        };
        let symbols = top_level_symbols(source)
            .into_iter()
            .map(|symbol| {
                let (line, character) = line_character(source, symbol.offset);
                let end_character = character + symbol.name.len();
                format!(
                    "{{\"name\":\"{}\",\"kind\":{},\"range\":{{\"start\":{{\"line\":{line},\"character\":{character}}},\"end\":{{\"line\":{line},\"character\":{end_character}}}}},\"selectionRange\":{{\"start\":{{\"line\":{line},\"character\":{character}}},\"end\":{{\"line\":{line},\"character\":{end_character}}}}}}}",
                    json_escape(&symbol.name),
                    symbol.kind
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[{symbols}]}}")
    }

    fn semantic_tokens_response(&self, id: &str, uri: &str) -> String {
        let Some(source) = self.documents.get(uri) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let data = encode_semantic_tokens(&semantic_tokens(source));
        format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"data\":[{data}]}}}}")
    }

    fn workspace_symbol_response(&mut self, id: &str, query: &str) -> String {
        let query = query.to_ascii_lowercase();
        let symbols = self
            .symbol_graph_sources()
            .iter()
            .flat_map(|entry| {
                entry
                    .symbols
                    .iter()
                    .filter(|symbol| matches!(symbol.kind, 5 | 10 | 11 | 12 | 23))
                    .map(move |symbol| (&entry.uri, &entry.source, symbol))
            })
            .filter(|(_, _, symbol)| {
                query.is_empty() || symbol.name.to_ascii_lowercase().contains(&query)
            })
            .map(|(uri, source, symbol)| {
                let (line, character) = line_character(source, symbol.offset);
                let end_character = character + symbol.name.len();
                format!(
                    "{{\"name\":\"{}\",\"kind\":{},\"location\":{{\"uri\":\"{}\",\"range\":{{\"start\":{{\"line\":{line},\"character\":{character}}},\"end\":{{\"line\":{line},\"character\":{end_character}}}}}}}}}",
                    json_escape(&symbol.name),
                    symbol.kind,
                    json_escape(uri)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[{symbols}]}}")
    }

    fn definition_response(&self, id: &str, uri: &str, line: usize, character: usize) -> String {
        let Some(source) = self.documents.get(uri) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let Some(offset) = byte_offset(source, line, character) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let Some((identifier_start, _, identifier)) = identifier_bounds_at(source, offset) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let Some(base) = file_uri_base(uri) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        if let Some(alias) = namespace_member_alias_at(source, identifier_start) {
            if let Some(specifier) = namespace_import_specifier(source, &alias) {
                if let Some((target_uri, target_source)) =
                    self.resolve_module_location(&base, &specifier)
                {
                    if let Some(symbol) = module_surface_symbol(&target_source, &identifier) {
                        return definition_location_response(
                            id,
                            &target_uri,
                            &target_source,
                            &symbol,
                        );
                    }
                }
            }
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        }
        let Some(symbol) = top_level_symbols(source)
            .into_iter()
            .find(|symbol| symbol.name == identifier)
        else {
            for specifier in direct_import_specifiers(source) {
                if let Some((target_uri, target_source)) =
                    self.resolve_module_location(&base, &specifier)
                {
                    if let Some(symbol) = module_surface_symbol(&target_source, &identifier) {
                        return definition_location_response(
                            id,
                            &target_uri,
                            &target_source,
                            &symbol,
                        );
                    }
                }
            }
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        definition_location_response(id, uri, source, &symbol)
    }

    fn prepare_rename_response(
        &self,
        id: &str,
        uri: &str,
        line: usize,
        character: usize,
    ) -> String {
        let Some((source, symbol)) = self.current_file_rename_symbol(uri, line, character) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let (line, character) = line_character(source, symbol.offset);
        let end_character = character + symbol.name.len();
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"range\":{{\"start\":{{\"line\":{line},\"character\":{character}}},\"end\":{{\"line\":{line},\"character\":{end_character}}}}},\"placeholder\":\"{}\"}}}}",
            json_escape(&symbol.name)
        )
    }

    fn rename_response(
        &self,
        id: &str,
        uri: &str,
        line: usize,
        character: usize,
        new_name: &str,
    ) -> String {
        if !is_valid_identifier(new_name) {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        }
        let Some((source, symbol)) = self.current_file_rename_symbol(uri, line, character) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let edits = rename_identifier_ranges(source, &symbol.name)
            .into_iter()
            .map(|(start, end)| {
                let (start_line, start_character) = line_character(source, start);
                let (end_line, end_character) = line_character(source, end);
                format!(
                    "{{\"range\":{{\"start\":{{\"line\":{start_line},\"character\":{start_character}}},\"end\":{{\"line\":{end_line},\"character\":{end_character}}}}},\"newText\":\"{}\"}}",
                    json_escape(new_name)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"changes\":{{\"{}\": [{edits}]}}}}}}",
            json_escape(uri)
        )
    }

    fn current_file_rename_symbol(
        &self,
        uri: &str,
        line: usize,
        character: usize,
    ) -> Option<(&str, TopLevelSymbol)> {
        let source = self.documents.get(uri)?;
        let offset = byte_offset(source, line, character)?;
        let (identifier_start, _, identifier) = identifier_bounds_at(source, offset)?;
        if namespace_member_alias_at(source, identifier_start).is_some() {
            return None;
        }
        let symbol = top_level_symbols(source)
            .into_iter()
            .find(|symbol| symbol.name == identifier)?;
        if has_unsafe_rename_shadow(source, &symbol) {
            return None;
        }
        Some((source, symbol))
    }

    fn completion_response(
        &mut self,
        id: &str,
        uri: &str,
        line: usize,
        character: usize,
    ) -> String {
        let Some(source) = self.documents.get(uri).cloned() else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[]}}");
        };
        let Some(offset) = byte_offset(&source, line, character) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[]}}");
        };
        let prefix_source = &source[..offset];
        if let Some(prefix) = import_completion_prefix(prefix_source) {
            let items = self
                .import_completion_specifiers(uri, &prefix)
                .into_iter()
                .map(|specifier| completion_item(&specifier, 9))
                .collect::<Vec<_>>();
            return format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[{}]}}",
                items.join(",")
            );
        }
        if let Some(receiver) = namespace_completion_alias(prefix_source) {
            let members = if namespace_import_specifier(&source, &receiver).is_some() {
                self.namespace_completion_members(uri, &source, &receiver)
            } else {
                receiver_member_completion_members(&source, &receiver)
            };
            let items = members
                .into_iter()
                .map(|member| completion_item(&member, 6))
                .collect::<Vec<_>>();
            return format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[{}]}}",
                items.join(",")
            );
        }
        let mut items = Vec::new();
        for keyword in [
            "let", "fn", "async", "await", "return", "if", "else", "while", "for", "in", "break",
            "continue", "import", "export", "record", "true", "false", "null",
        ] {
            items.push(completion_item(keyword, 14));
        }
        for builtin in [
            "args",
            "contains",
            "env_get",
            "env_list",
            "exists",
            "len",
            "map_get",
            "ok",
            "err",
            "none",
            "read_text",
            "sleep_ms",
            "some",
            "sqrt",
            "task_cancel",
            "task_ready",
            "task_sleep_ms",
            "tcp_connect",
            "to_float",
            "to_int",
            "write_text",
        ] {
            items.push(completion_item(builtin, 3));
        }
        for signature in self.host_function_signatures(uri) {
            if signature.name.starts_with("__") {
                continue;
            }
            items.push(completion_item_with_detail(
                &signature.name,
                3,
                &host_signature_label(&signature),
                signature.docstring.as_deref(),
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for (name, kind) in self.project_top_level_completion_symbols(uri) {
            if seen.insert(name.clone()) {
                items.push(completion_item(&name, kind));
            }
        }
        for identifier in collect_identifiers(prefix_source) {
            if seen.insert(identifier.clone()) {
                items.push(completion_item(&identifier, 6));
            }
        }
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[{}]}}",
            items.join(",")
        )
    }

    fn import_completion_specifiers(&self, uri: &str, prefix: &str) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut specifiers = Vec::new();
        for specifier in std_module_specifiers() {
            if specifier.starts_with(prefix) && seen.insert(specifier.to_string()) {
                specifiers.push(specifier.to_string());
            }
        }
        let Some(base) = file_uri_base(uri) else {
            return specifiers;
        };
        let overlay = self.documents_overlay();
        for root in manifest_search_paths(&base) {
            let mut files = Vec::new();
            collect_nox_files(&root, &mut files);
            for path in files {
                let specifier = match path.strip_prefix(&root) {
                    Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
                    Err(_) => continue,
                };
                if specifier.starts_with(prefix) && seen.insert(specifier.clone()) {
                    specifiers.push(specifier);
                }
            }
        }
        for path in overlay.keys() {
            if path.extension().is_some_and(|extension| extension == "nox") {
                let specifier = path
                    .strip_prefix(&base)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if !specifier.starts_with("..")
                    && specifier.starts_with(prefix)
                    && seen.insert(specifier.clone())
                {
                    specifiers.push(specifier);
                }
            }
        }
        specifiers.sort();
        specifiers
    }

    fn project_top_level_completion_symbols(&mut self, uri: &str) -> Vec<(String, u8)> {
        let Some(base) = file_uri_base(uri) else {
            return Vec::new();
        };
        let mut roots = manifest_search_paths(&base);
        if roots.is_empty() {
            roots.extend(self.workspace_roots.iter().cloned());
        }
        if roots.is_empty() {
            return Vec::new();
        }

        let mut symbols = Vec::new();
        for entry in self.symbol_graph_sources() {
            let Some(path) = file_uri_path(&entry.uri) else {
                continue;
            };
            if !roots.iter().any(|root| path.starts_with(root)) {
                continue;
            }
            for symbol in &entry.symbols {
                if matches!(symbol.kind, 5 | 10 | 11 | 12 | 23) {
                    symbols.push((
                        symbol.name.clone(),
                        completion_kind_for_symbol_kind(symbol.kind),
                    ));
                }
            }
        }
        symbols.sort_by(|left, right| left.0.cmp(&right.0));
        symbols
    }

    fn namespace_completion_members(&self, uri: &str, source: &str, alias: &str) -> Vec<String> {
        let Some(specifier) = namespace_import_specifier(source, alias) else {
            return Vec::new();
        };
        let Some(base) = file_uri_base(uri) else {
            return Vec::new();
        };
        if let Ok(Some(source)) = std_module_source(&specifier) {
            return module_surface_members(source);
        }
        let Some(module_source) = self.resolve_module_source(&base, &specifier) else {
            return Vec::new();
        };
        module_surface_members(&module_source)
    }

    fn hover_response(&mut self, id: &str, uri: &str, line: usize, character: usize) -> String {
        let Some(source) = self.documents.get(uri).cloned() else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let Some(offset) = byte_offset(&source, line, character) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let identifier = identifier_at(&source, offset);
        if let Some(value) = identifier
            .as_deref()
            .and_then(|alias| self.module_hover_value(uri, &source, alias))
        {
            let value = append_generated_source_note(value, generated_source_hover_note(uri));
            return format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"contents\":{{\"kind\":\"plaintext\",\"value\":\"{}\"}}}}}}",
                json_escape(&value)
            );
        }
        let generated_note = generated_source_hover_note(uri);
        let doc_comment = identifier
            .as_deref()
            .and_then(|name| doc_comment_for_top_level(&source, name));
        let source_signature = identifier
            .as_deref()
            .and_then(|name| function_signature(&source, name));
        let host_signature = identifier
            .as_deref()
            .and_then(|name| self.host_function_signature(uri, name));
        let source_clone = source.clone();
        let overlay = self.documents_overlay();
        let result = if let Some(base) = file_uri_base(uri) {
            let session = self.sessions.entry(uri.to_string()).or_default();
            install_lsp_stdlib(session);
            check_session_loader(session, base.clone(), overlay.clone());
            session.hover_type(&source_clone, offset)
        } else {
            let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
            runtime.hover_type_source(&source_clone, offset)
        };
        match result {
            Ok(Some(ty)) => {
                let mut value = ty.to_string();
                if let Some(note) = generated_note.as_ref() {
                    value.push_str("\n\n");
                    value.push_str(note);
                }
                if let Some(signature) = source_signature.as_ref() {
                    value.push_str("\n\n");
                    value.push_str(&signature.label);
                }
                if let Some(doc) = doc_comment {
                    value.push_str("\n\n");
                    value.push_str(&doc);
                } else if let Some(signature) = host_signature.as_ref() {
                    value.push_str("\n\n");
                    value.push_str(&host_signature_label(signature));
                    if let Some(doc) = &signature.docstring {
                        value.push_str("\n\n");
                        value.push_str(doc);
                    }
                    if !signature.capabilities.is_empty() {
                        value.push_str("\n\ncapabilities: ");
                        value.push_str(&signature.capabilities.join(", "));
                    }
                }
                format!(
                    "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"contents\":{{\"kind\":\"plaintext\",\"value\":\"{}\"}}}}}}",
                    json_escape(&value)
                )
            }
            Ok(None) | Err(_) => match host_signature {
                Some(signature) => {
                    let mut value = host_signature_label(&signature);
                    if let Some(note) = generated_note.as_ref() {
                        value.push_str("\n\n");
                        value.push_str(note);
                    }
                    if let Some(doc) = signature.docstring {
                        value.push_str("\n\n");
                        value.push_str(&doc);
                    }
                    if !signature.capabilities.is_empty() {
                        value.push_str("\n\ncapabilities: ");
                        value.push_str(&signature.capabilities.join(", "));
                    }
                    format!(
                        "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"contents\":{{\"kind\":\"plaintext\",\"value\":\"{}\"}}}}}}",
                        json_escape(&value)
                    )
                }
                None => match generated_note {
                    Some(value) => format!(
                        "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"contents\":{{\"kind\":\"plaintext\",\"value\":\"{}\"}}}}}}",
                        json_escape(&value)
                    ),
                    None => format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}"),
                },
            },
        }
    }

    fn module_hover_value(&self, uri: &str, source: &str, alias: &str) -> Option<String> {
        let specifier = namespace_import_specifier(source, alias)?;
        let base = file_uri_base(uri)?;
        let module_source = if let Ok(Some(source)) = std_module_source(&specifier) {
            source.to_string()
        } else {
            self.resolve_module_source(&base, &specifier)?
        };
        let exports = module_surface_members(&module_source);
        let export_text = if exports.is_empty() {
            "(none)".to_string()
        } else {
            exports.join(", ")
        };
        Some(format!("module {specifier}\n\nexports: {export_text}"))
    }

    fn signature_help_response(
        &mut self,
        id: &str,
        uri: &str,
        line: usize,
        character: usize,
    ) -> String {
        let Some(source) = self.documents.get(uri) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let Some(offset) = byte_offset(source, line, character) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let Some(call) = active_call(source, offset) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let signature = function_signature(source, &call.name).or_else(|| {
            self.host_function_signature(uri, &call.name)
                .map(|signature| FunctionSignature {
                    label: host_signature_label(&signature),
                    parameters: signature
                        .params
                        .iter()
                        .map(|(name, ty)| format!("{name}: {ty}"))
                        .collect(),
                })
        });
        let Some(signature) = signature else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let parameters = signature
            .parameters
            .iter()
            .map(|parameter| format!("{{\"label\":\"{}\"}}", json_escape(parameter)))
            .collect::<Vec<_>>()
            .join(",");
        let active_parameter = call
            .active_parameter
            .min(signature.parameters.len().saturating_sub(1));
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"signatures\":[{{\"label\":\"{}\",\"parameters\":[{}]}}],\"activeSignature\":0,\"activeParameter\":{active_parameter}}}}}",
            json_escape(&signature.label),
            parameters
        )
    }

    fn host_function_signatures(&mut self, uri: &str) -> Vec<HostFunctionSignature> {
        let Some(base) = file_uri_base(uri) else {
            return Vec::new();
        };
        let overlay = self.documents_overlay();
        let session = self.sessions.entry(uri.to_string()).or_default();
        install_lsp_stdlib(session);
        check_session_loader(session, base, overlay);
        session
            .host_function_names()
            .into_iter()
            .filter_map(|name| session.host_function_signature(&name))
            .collect()
    }

    fn host_function_signature(&mut self, uri: &str, name: &str) -> Option<HostFunctionSignature> {
        let base = file_uri_base(uri)?;
        let overlay = self.documents_overlay();
        let session = self.sessions.entry(uri.to_string()).or_default();
        install_lsp_stdlib(session);
        check_session_loader(session, base, overlay);
        session.host_function_signature(name)
    }

    fn code_action_response(&self, id: &str, uri: &str) -> String {
        let Some(source) = self.documents.get(uri) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[]}}");
        };
        let mut actions = Vec::new();
        if let Some((start, end)) = todo_marker_range(source) {
            let (start_line, start_character) = line_character(source, start);
            let (end_line, end_character) = line_character(source, end);
            actions.push(format!(
                "{{\"title\":\"Remove TODO marker\",\"kind\":\"quickfix\",\"edit\":{{\"changes\":{{\"{}\":[{{\"range\":{{\"start\":{{\"line\":{start_line},\"character\":{start_character}}},\"end\":{{\"line\":{end_line},\"character\":{end_character}}}}},\"newText\":\"\"}}]}}}}}}",
                json_escape(uri)
            ));
        }
        actions.push(format!(
            "{{\"title\":\"Run nox check\",\"kind\":\"source.fixAll.nox\",\"command\":{{\"title\":\"Run nox check\",\"command\":\"nox.check\",\"arguments\":[\"{}\"]}}}}",
            json_escape(uri)
        ));
        actions.push(format!(
            "{{\"title\":\"Format document\",\"kind\":\"source.format.nox\",\"command\":{{\"title\":\"Format document\",\"command\":\"nox.format\",\"arguments\":[\"{}\"]}}}}",
            json_escape(uri)
        ));
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[{}]}}",
            actions.join(",")
        )
    }

    fn documents_overlay(&self) -> HashMap<PathBuf, String> {
        let mut overlay = HashMap::new();
        for (uri, text) in &self.documents {
            if let Some(path) = file_uri_path(uri) {
                overlay.insert(path, text.clone());
            }
        }
        overlay
    }

    fn symbol_graph_sources(&mut self) -> &[SymbolGraphSource] {
        if self.symbol_graph_cache.is_none() {
            self.symbol_graph_cache = Some(self.build_symbol_graph_sources());
        }
        self.symbol_graph_cache.as_deref().unwrap_or(&[])
    }

    fn build_symbol_graph_sources(&self) -> Vec<SymbolGraphSource> {
        let overlay = self.documents_overlay();
        let mut seen = std::collections::HashSet::new();
        let mut sources = Vec::new();

        for (uri, text) in &self.documents {
            if seen.insert(uri.clone()) {
                sources.push(SymbolGraphSource::new(uri.clone(), text.clone()));
            }
        }

        for uri in self.documents.keys() {
            let Some(base) = file_uri_base(uri) else {
                continue;
            };
            for root in manifest_search_paths(&base) {
                for path in workspace_nox_files(&root) {
                    let uri = path_to_file_uri(&path);
                    if !seen.insert(uri.clone()) {
                        continue;
                    }
                    if let Some(source) = overlay.get(&path).cloned() {
                        sources.push(SymbolGraphSource::new(uri, source));
                    } else if let Ok(source) = fs::read_to_string(&path) {
                        sources.push(SymbolGraphSource::new(uri, source));
                    }
                }
            }
        }

        for root in &self.workspace_roots {
            for path in workspace_nox_files(root) {
                let uri = path_to_file_uri(&path);
                if !seen.insert(uri.clone()) {
                    continue;
                }
                if let Some(source) = overlay.get(&path).cloned() {
                    sources.push(SymbolGraphSource::new(uri, source));
                } else if let Ok(source) = fs::read_to_string(&path) {
                    sources.push(SymbolGraphSource::new(uri, source));
                }
            }
        }

        sources
    }

    fn resolve_module_source(&self, base: &Path, specifier: &str) -> Option<String> {
        if let Ok(Some(source)) = std_module_source(specifier) {
            return Some(source.to_string());
        }
        let overlay = self.documents_overlay();
        let primary = base.join(specifier);
        if let Some(source) = overlay.get(&primary) {
            return Some(source.clone());
        }
        if primary.is_file() {
            return fs::read_to_string(primary).ok();
        }
        for search in manifest_search_paths(base) {
            let candidate = search.join(specifier);
            if let Some(source) = overlay.get(&candidate) {
                return Some(source.clone());
            }
            if candidate.is_file() {
                return fs::read_to_string(candidate).ok();
            }
        }
        let external_modules = external_modules_for_base(base).ok()?;
        if let Ok(Some(source)) = load_external_module(specifier, &external_modules) {
            return Some(source);
        }
        None
    }

    fn resolve_module_location(&self, base: &Path, specifier: &str) -> Option<(String, String)> {
        if matches!(std_module_source(specifier), Ok(Some(_))) {
            return None;
        }
        let overlay = self.documents_overlay();
        let primary = base.join(specifier);
        if let Some(source) = overlay.get(&primary) {
            return Some((path_to_file_uri(&primary), source.clone()));
        }
        if primary.is_file() {
            return fs::read_to_string(&primary)
                .ok()
                .map(|source| (path_to_file_uri(&primary), source));
        }
        for search in manifest_search_paths(base) {
            let candidate = search.join(specifier);
            if let Some(source) = overlay.get(&candidate) {
                return Some((path_to_file_uri(&candidate), source.clone()));
            }
            if candidate.is_file() {
                return fs::read_to_string(&candidate)
                    .ok()
                    .map(|source| (path_to_file_uri(&candidate), source));
            }
        }
        None
    }

    fn publish_diagnostics(&mut self, uri: &str, source: &str) -> String {
        let overlay = self.documents_overlay();
        let (errors, warnings) = if let Some(base) = file_uri_base(uri) {
            match manifest_diagnostic(&base) {
                Some(diagnostic) => (vec![diagnostic], Vec::new()),
                None => {
                    let session = self.sessions.entry(uri.to_string()).or_default();
                    install_lsp_stdlib(session);
                    check_session_loader(session, base, overlay);
                    let errors = session.check_diagnostics(source).err().unwrap_or_default();
                    let warnings = if errors.is_empty() {
                        session.lint(source).ok().unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    (errors, warnings)
                }
            }
        } else {
            let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
            let errors = runtime
                .check_source_diagnostics(source)
                .err()
                .unwrap_or_default();
            let warnings = if errors.is_empty() {
                runtime.lint(source).ok().unwrap_or_default()
            } else {
                Vec::new()
            };
            (errors, warnings)
        };
        let mut entries: Vec<String> = Vec::with_capacity(errors.len() + warnings.len());
        for diagnostic in &errors {
            entries.push(diagnostic_json(diagnostic, source));
        }
        for warning in &warnings {
            entries.push(lint_warning_json(warning, source));
        }
        let diagnostics = entries.join(",");
        format!(
            "{{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/publishDiagnostics\",\"params\":{{\"uri\":\"{}\",\"diagnostics\":[{diagnostics}]}}}}",
            json_escape(uri)
        )
    }
}

fn doc_comment_for_top_level(source: &str, name: &str) -> Option<String> {
    let mut pending: Vec<String> = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("///") {
            let cleaned = rest.strip_prefix(' ').unwrap_or(rest);
            pending.push(cleaned.to_string());
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        let declares = declaration_matches(trimmed, name);
        if declares && !pending.is_empty() {
            return Some(pending.join("\n"));
        }
        pending.clear();
    }
    None
}

fn stable_hash(value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn todo_marker_range(source: &str) -> Option<(usize, usize)> {
    let mut line_start = 0;
    for line in source.split_inclusive('\n') {
        if let Some(comment_start) = line.find("//") {
            if let Some(todo_start) = line[comment_start..].find("TODO") {
                let start = line_start + comment_start + todo_start;
                return Some((start, start + "TODO".len()));
            }
        }
        line_start += line.len();
    }
    None
}

const SEMANTIC_TOKEN_NAMESPACE: u32 = 0;
const SEMANTIC_TOKEN_TYPE: u32 = 1;
const SEMANTIC_TOKEN_FUNCTION: u32 = 2;
const SEMANTIC_TOKEN_VARIABLE: u32 = 3;
const SEMANTIC_TOKEN_KEYWORD: u32 = 4;
const SEMANTIC_TOKEN_STRING: u32 = 5;
const SEMANTIC_TOKEN_NUMBER: u32 = 6;
const SEMANTIC_TOKEN_COMMENT: u32 = 7;

const SEMANTIC_MOD_DECLARATION: u32 = 1;
const SEMANTIC_MOD_READONLY: u32 = 1 << 1;
const SEMANTIC_MOD_ASYNC: u32 = 1 << 2;

#[derive(Debug, Clone, Copy)]
struct SemanticToken {
    line: u32,
    start: u32,
    length: u32,
    token_type: u32,
    modifiers: u32,
}

fn semantic_tokens(source: &str) -> Vec<SemanticToken> {
    let mut tokens = Vec::new();
    for (line, text) in source.lines().enumerate() {
        semantic_tokens_for_line(line as u32, text, &mut tokens);
    }
    tokens
}

fn semantic_tokens_for_line(line_number: u32, line: &str, tokens: &mut Vec<SemanticToken>) {
    let mut byte = 0usize;
    let mut character = 0u32;
    let mut pending_declaration: Option<(u32, u32)> = None;
    let mut pending_async = false;

    while byte < line.len() {
        if line[byte..].starts_with("//") {
            tokens.push(SemanticToken {
                line: line_number,
                start: character,
                length: line[byte..].chars().count() as u32,
                token_type: SEMANTIC_TOKEN_COMMENT,
                modifiers: 0,
            });
            break;
        }

        let ch = line[byte..]
            .chars()
            .next()
            .expect("byte index stays on a char boundary");
        if ch.is_whitespace() {
            byte += ch.len_utf8();
            character += 1;
            continue;
        }

        if ch == '"' {
            let start = character;
            byte += ch.len_utf8();
            character += 1;
            let mut escaped = false;
            while byte < line.len() {
                let current = line[byte..]
                    .chars()
                    .next()
                    .expect("byte index stays on a char boundary");
                byte += current.len_utf8();
                character += 1;
                if current == '"' && !escaped {
                    break;
                }
                escaped = current == '\\' && !escaped;
                if current != '\\' {
                    escaped = false;
                }
            }
            tokens.push(SemanticToken {
                line: line_number,
                start,
                length: character - start,
                token_type: SEMANTIC_TOKEN_STRING,
                modifiers: 0,
            });
            pending_declaration = None;
            continue;
        }

        if is_semantic_number_start(line, byte, ch) {
            let start = character;
            let start_byte = byte;
            while byte < line.len() {
                let current = line[byte..]
                    .chars()
                    .next()
                    .expect("byte index stays on a char boundary");
                if !(current.is_ascii_alphanumeric() || current == '_' || current == '.') {
                    break;
                }
                byte += current.len_utf8();
                character += 1;
            }
            if byte == start_byte {
                byte += ch.len_utf8();
                character += 1;
            }
            tokens.push(SemanticToken {
                line: line_number,
                start,
                length: character - start,
                token_type: SEMANTIC_TOKEN_NUMBER,
                modifiers: 0,
            });
            pending_declaration = None;
            continue;
        }

        if is_semantic_identifier_start(ch) {
            let start = character;
            let start_byte = byte;
            byte += ch.len_utf8();
            character += 1;
            while byte < line.len() {
                let current = line[byte..]
                    .chars()
                    .next()
                    .expect("byte index stays on a char boundary");
                if !is_semantic_identifier_continue(current) {
                    break;
                }
                byte += current.len_utf8();
                character += 1;
            }
            let word = &line[start_byte..byte];
            if is_semantic_keyword(word) {
                if word == "async" {
                    pending_async = true;
                } else {
                    pending_declaration = semantic_declaration_after_keyword(word, pending_async);
                    if word != "export" {
                        pending_async = false;
                    }
                }
                tokens.push(SemanticToken {
                    line: line_number,
                    start,
                    length: character - start,
                    token_type: SEMANTIC_TOKEN_KEYWORD,
                    modifiers: 0,
                });
                continue;
            }

            if let Some((token_type, modifiers)) = pending_declaration.take() {
                tokens.push(SemanticToken {
                    line: line_number,
                    start,
                    length: character - start,
                    token_type,
                    modifiers,
                });
            } else {
                let token_type = if is_builtin_type_name(word) || starts_with_uppercase(word) {
                    SEMANTIC_TOKEN_TYPE
                } else if next_non_whitespace_char(line, byte) == Some('(') {
                    SEMANTIC_TOKEN_FUNCTION
                } else {
                    SEMANTIC_TOKEN_VARIABLE
                };
                tokens.push(SemanticToken {
                    line: line_number,
                    start,
                    length: character - start,
                    token_type,
                    modifiers: 0,
                });
            }
            continue;
        }

        byte += ch.len_utf8();
        character += 1;
        if !matches!(ch, '<' | '>' | '[' | ']' | ',' | ':' | '.') {
            pending_declaration = None;
        }
    }
}

fn encode_semantic_tokens(tokens: &[SemanticToken]) -> String {
    let mut encoded = String::new();
    let mut previous_line = 0u32;
    let mut previous_start = 0u32;
    for (index, token) in tokens.iter().enumerate() {
        if index > 0 {
            encoded.push(',');
        }
        let delta_line = token.line - previous_line;
        let delta_start = if delta_line == 0 {
            token.start - previous_start
        } else {
            token.start
        };
        let _ = write!(
            encoded,
            "{delta_line},{delta_start},{},{},{}",
            token.length, token.token_type, token.modifiers
        );
        previous_line = token.line;
        previous_start = token.start;
    }
    encoded
}

fn semantic_declaration_after_keyword(keyword: &str, pending_async: bool) -> Option<(u32, u32)> {
    match keyword {
        "fn" => {
            let mut modifiers = SEMANTIC_MOD_DECLARATION;
            if pending_async {
                modifiers |= SEMANTIC_MOD_ASYNC;
            }
            Some((SEMANTIC_TOKEN_FUNCTION, modifiers))
        }
        "record" | "enum" | "trait" | "type" => {
            Some((SEMANTIC_TOKEN_TYPE, SEMANTIC_MOD_DECLARATION))
        }
        "let" => Some((SEMANTIC_TOKEN_VARIABLE, SEMANTIC_MOD_DECLARATION)),
        "const" => Some((
            SEMANTIC_TOKEN_VARIABLE,
            SEMANTIC_MOD_DECLARATION | SEMANTIC_MOD_READONLY,
        )),
        "as" => Some((SEMANTIC_TOKEN_NAMESPACE, SEMANTIC_MOD_DECLARATION)),
        _ => None,
    }
}

fn is_semantic_keyword(word: &str) -> bool {
    matches!(
        word,
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "else"
            | "enum"
            | "err"
            | "export"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "import"
            | "in"
            | "let"
            | "match"
            | "none"
            | "ok"
            | "record"
            | "return"
            | "some"
            | "trait"
            | "type"
            | "while"
    )
}

fn is_builtin_type_name(word: &str) -> bool {
    matches!(
        word,
        "bool" | "float" | "int" | "json" | "map" | "null" | "option" | "result" | "str" | "task"
    )
}

fn starts_with_uppercase(word: &str) -> bool {
    word.chars().next().is_some_and(char::is_uppercase)
}

fn is_semantic_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_semantic_identifier_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn is_semantic_number_start(line: &str, byte: usize, ch: char) -> bool {
    ch.is_ascii_digit()
        || ((ch == '-' || ch == '+')
            && line[byte + ch.len_utf8()..]
                .chars()
                .next()
                .is_some_and(|next| next.is_ascii_digit()))
}

fn next_non_whitespace_char(line: &str, byte: usize) -> Option<char> {
    line[byte..].chars().find(|ch| !ch.is_whitespace())
}

fn declaration_matches(trimmed: &str, name: &str) -> bool {
    let pattern_pairs = [
        ("export fn ", "("),
        ("export async fn ", "("),
        ("fn ", "("),
        ("async fn ", "("),
        ("export record ", " {"),
        ("record ", " {"),
        ("export enum ", " {"),
        ("enum ", " {"),
        ("export trait ", " {"),
        ("trait ", " {"),
        ("export type ", " "),
        ("type ", " "),
        ("export let ", ""),
        ("let ", ""),
        ("export const ", ""),
        ("const ", ""),
    ];
    for (prefix, after) in &pattern_pairs {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let candidate = if after.is_empty() {
                // For let / const, name ends at ':' or '=' or whitespace
                rest.split(|c: char| c == ':' || c == '=' || c.is_whitespace())
                    .next()
                    .unwrap_or("")
            } else if let Some(idx) = rest.find(after) {
                &rest[..idx]
            } else {
                rest.split_whitespace().next().unwrap_or("")
            };
            let trimmed_candidate = candidate.trim_end_matches('<');
            let final_candidate = trimmed_candidate
                .split('<')
                .next()
                .unwrap_or(trimmed_candidate);
            if final_candidate == name {
                return true;
            }
        }
    }
    false
}

fn lint_warning_json(warning: &LintWarning, source: &str) -> String {
    let (start_line, start_character) = line_character(source, warning.span.start);
    let (end_line, end_character) = line_character(source, warning.span.end);
    let data = lsp_diagnostic_data(
        "lint",
        warning.code,
        &warning.message,
        warning.span,
        source,
        &[],
    );
    format!(
        "{{\"range\":{{\"start\":{{\"line\":{start_line},\"character\":{start_character}}},\"end\":{{\"line\":{end_line},\"character\":{end_character}}}}},\"severity\":2,\"source\":\"nox\",\"code\":\"{}\",\"message\":\"{}\"{data}}}",
        json_escape(warning.code),
        json_escape(&warning.message)
    )
}

fn check_session_loader(session: &mut Session, base: PathBuf, overlay: HashMap<PathBuf, String>) {
    let search_paths = manifest_search_paths(&base);
    let external_modules = external_modules_for_base(&base).unwrap_or_default();
    session.set_module_loader(move |specifier| {
        if let Some(source) = std_module_source(specifier)? {
            return Ok(source.to_string());
        }
        let primary = base.join(specifier);
        if let Some(source) = overlay.get(&primary) {
            return Ok(source.clone());
        }
        if primary.is_file() {
            return read_module(&primary);
        }
        for search in &search_paths {
            let candidate = search.join(specifier);
            if let Some(source) = overlay.get(&candidate) {
                return Ok(source.clone());
            }
            if candidate.is_file() {
                return read_module(&candidate);
            }
        }
        if let Some(source) = load_external_module(specifier, &external_modules)? {
            return Ok(source);
        }
        read_module(&primary)
    });
}

fn manifest_search_paths(base: &Path) -> Vec<PathBuf> {
    let probe = base.join("probe.nox");
    match Manifest::discover(&probe) {
        Ok(Some(manifest)) => manifest.source_dirs(),
        _ => Vec::new(),
    }
}

fn generated_source_hover_note(uri: &str) -> Option<String> {
    let path = file_uri_path(uri)?;
    let manifest = Manifest::discover(&path).ok()??;
    let artifact = manifest
        .codegen
        .iter()
        .find(|artifact| manifest.root.join(&artifact.generated) == path)?;
    let mut note = format!("generated source\n\nartifact: {}", artifact.name);
    if let Some(generator) = &artifact.generator {
        note.push_str("\ngenerator: ");
        note.push_str(generator);
    }
    if let Some(template) = &artifact.template {
        note.push_str("\ntemplate: ");
        note.push_str(&manifest.root.join(template).display().to_string());
    }
    if let Some(input_hash) = &artifact.input_hash {
        note.push_str("\ninput_hash: ");
        note.push_str(input_hash);
    }
    if let Some(source_map) = &artifact.source_map {
        note.push_str("\nsource_map: ");
        note.push_str(&manifest.root.join(source_map).display().to_string());
    }
    if let Some(source_map_hash) = &artifact.source_map_hash {
        note.push_str("\nsource_map_hash: ");
        note.push_str(source_map_hash);
    }
    if let Some(command) = &artifact.command {
        note.push_str("\ncommand: ");
        note.push_str(command);
    }
    Some(note)
}

fn append_generated_source_note(mut value: String, note: Option<String>) -> String {
    if let Some(note) = note {
        value.push_str("\n\n");
        value.push_str(&note);
    }
    value
}

fn external_modules_for_base(
    base: &Path,
) -> Result<Vec<crate::ExternalModuleDependency>, Diagnostic> {
    let probe = base.join("probe.nox");
    match Manifest::discover(&probe)? {
        Some(manifest) => external_modules_for_manifest(&manifest),
        None => Ok(Vec::new()),
    }
}

fn workspace_nox_files(root: &Path) -> Vec<PathBuf> {
    let roots = match Manifest::discover(&root.join("probe.nox")) {
        Ok(Some(manifest)) => manifest.source_dirs(),
        _ => vec![root.to_path_buf()],
    };
    let mut files = Vec::new();
    for root in roots {
        collect_nox_files(&root, &mut files);
    }
    files.sort();
    files
}

fn collect_nox_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_nox_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "nox") {
            files.push(path);
        }
    }
}

fn manifest_diagnostic(base: &Path) -> Option<Diagnostic> {
    let probe = base.join("probe.nox");
    Manifest::discover(&probe).err()
}

fn read_module(path: &Path) -> Result<String, Diagnostic> {
    fs::read_to_string(path).map_err(|err| {
        Diagnostic::new(
            format!("failed to load module '{}': {err}", path.display()),
            Span { start: 0, end: 0 },
        )
        .with_code("module.not-found")
    })
}

fn completion_item(label: &str, kind: u8) -> String {
    format!("{{\"label\":\"{}\",\"kind\":{kind}}}", json_escape(label))
}

fn completion_item_with_detail(
    label: &str,
    kind: u8,
    detail: &str,
    documentation: Option<&str>,
) -> String {
    let documentation = documentation
        .map(|doc| format!(r#","documentation":"{}""#, json_escape(doc)))
        .unwrap_or_default();
    format!(
        "{{\"label\":\"{}\",\"kind\":{kind},\"detail\":\"{}\"{documentation}}}",
        json_escape(label),
        json_escape(detail)
    )
}

fn completion_kind_for_symbol_kind(symbol_kind: u8) -> u8 {
    match symbol_kind {
        12 => 3,  // function
        10 => 13, // enum
        11 => 8,  // interface/trait
        23 => 7,  // struct/record
        5 => 25,  // type parameter/type alias
        _ => 6,
    }
}

fn host_signature_label(signature: &HostFunctionSignature) -> String {
    let params = signature
        .params
        .iter()
        .map(|(name, ty)| format!("{name}: {ty}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "fn {}({params}) -> {}",
        signature.name, signature.return_type
    )
}

fn collect_identifiers(source: &str) -> Vec<String> {
    let mut identifiers = Vec::new();
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut in_string = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if byte == b'"' {
                in_string = false;
            } else if byte == b'\\' && index + 1 < bytes.len() {
                index += 2;
                continue;
            }
            index += 1;
            continue;
        }
        if byte == b'"' {
            in_string = true;
            index += 1;
            continue;
        }
        if byte == b'/' && index + 1 < bytes.len() && bytes[index + 1] == b'/' {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            identifiers.push(source[start..index].to_string());
            continue;
        }
        index += 1;
    }
    identifiers
}

fn import_completion_prefix(source: &str) -> Option<String> {
    let line_start = source.rfind('\n').map(|index| index + 1).unwrap_or(0);
    let line = &source[line_start..];
    let trimmed = line.trim_start();
    if !trimmed.starts_with("import") {
        return None;
    }
    let mut rest = &trimmed["import".len()..];
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    rest = rest.trim_start();
    let rest = rest.strip_prefix('"')?;
    if rest.contains('"') {
        return None;
    }
    Some(rest.to_string())
}

fn std_module_specifiers() -> &'static [&'static str] {
    &[
        "std/array.nox",
        "std/bytes.nox",
        "std/csv.nox",
        "std/dotenv.nox",
        "std/encoding.nox",
        "std/env.nox",
        "std/fs.nox",
        "std/hash.nox",
        "std/http.nox",
        "std/ini.nox",
        "std/json.nox",
        "std/jsonl.nox",
        "std/map.nox",
        "std/option.nox",
        "std/path.nox",
        "std/process.nox",
        "std/random.nox",
        "std/result.nox",
        "std/string.nox",
        "std/task.nox",
        "std/term.nox",
        "std/time.nox",
        "std/toml.nox",
        "std/traits.nox",
        "std/tsv.nox",
        "std/url.nox",
    ]
}

fn namespace_completion_alias(source: &str) -> Option<String> {
    let bytes = source.as_bytes();
    let mut index = bytes.len();
    while index > 0 && is_identifier_continue(bytes[index - 1]) {
        index -= 1;
    }
    if index == 0 || bytes[index - 1] != b'.' {
        return None;
    }
    let dot = index - 1;
    let mut start = dot;
    while start > 0 && is_identifier_continue(bytes[start - 1]) {
        start -= 1;
    }
    if start == dot || !is_identifier_start(bytes[start]) {
        return None;
    }
    Some(source[start..dot].to_string())
}

fn namespace_import_specifier(source: &str, alias: &str) -> Option<String> {
    let tokens = lexical_tokens(source);
    for window in tokens.windows(4) {
        if window[0] == "import" && window[2] == "as" && window[3] == alias {
            return Some(window[1].clone());
        }
    }
    None
}

fn direct_import_specifiers(source: &str) -> Vec<String> {
    let tokens = lexical_tokens(source);
    let mut specifiers = Vec::new();
    let mut index = 0;
    while index + 1 < tokens.len() {
        if tokens[index] == "import" {
            let specifier = tokens[index + 1].clone();
            if tokens.get(index + 2).is_some_and(|token| token == "as") {
                index += 4;
            } else {
                specifiers.push(specifier);
                index += 2;
            }
            continue;
        }
        index += 1;
    }
    specifiers
}

fn module_surface_members(source: &str) -> Vec<String> {
    module_surface_symbols(source)
        .into_iter()
        .map(|symbol| symbol.name)
        .collect()
}

struct ActiveCall {
    name: String,
    active_parameter: usize,
}

struct FunctionSignature {
    label: String,
    parameters: Vec<String>,
}

struct SymbolGraphSource {
    uri: String,
    source: String,
    symbols: Vec<TopLevelSymbol>,
}

impl SymbolGraphSource {
    fn new(uri: String, source: String) -> Self {
        let symbols = top_level_symbols(&source);
        Self {
            uri,
            source,
            symbols,
        }
    }
}

#[derive(Clone)]
struct TopLevelSymbol {
    name: String,
    kind: u8,
    offset: usize,
    exported: bool,
}

fn definition_location_response(
    id: &str,
    uri: &str,
    source: &str,
    symbol: &TopLevelSymbol,
) -> String {
    let (line, character) = line_character(source, symbol.offset);
    let end_character = character + symbol.name.len();
    format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"uri\":\"{}\",\"range\":{{\"start\":{{\"line\":{line},\"character\":{character}}},\"end\":{{\"line\":{line},\"character\":{end_character}}}}}}}}}",
        json_escape(uri)
    )
}

fn active_call(source: &str, offset: usize) -> Option<ActiveCall> {
    let prefix = &source[..offset];
    let open = prefix.rfind('(')?;
    let before = prefix[..open].trim_end();
    let name_end = before.len();
    let mut name_start = name_end;
    let bytes = before.as_bytes();
    while name_start > 0 && is_identifier_continue(bytes[name_start - 1]) {
        name_start -= 1;
    }
    if name_start == name_end || !is_identifier_start(bytes[name_start]) {
        return None;
    }
    let active_parameter = prefix[open + 1..]
        .bytes()
        .filter(|byte| *byte == b',')
        .count();
    Some(ActiveCall {
        name: before[name_start..name_end].to_string(),
        active_parameter,
    })
}

fn function_signature(source: &str, name: &str) -> Option<FunctionSignature> {
    let needle = format!("fn {name}");
    let needle_start = source.find(&needle)?;
    let is_async = source[..needle_start].trim_end().ends_with("async");
    let start = needle_start + "fn ".len();
    let rest = &source[start..];
    let after_name = rest.strip_prefix(name)?;
    let after_name = after_name.trim_start();
    let (type_params, after_type_params) = if let Some(tail) = after_name.strip_prefix('<') {
        let close = tail.find('>')?;
        let params = tail[..close].trim();
        (Some(params), tail[close + 1..].trim_start())
    } else {
        (None, after_name)
    };
    let open = after_type_params.find('(')?;
    if !after_type_params[..open].trim().is_empty() {
        return None;
    }
    let rest = after_type_params;
    let close = rest[open + 1..].find(')')? + open + 1;
    let after_params = &rest[close + 1..];
    let return_type = after_params
        .find("->")
        .and_then(|arrow| {
            let tail = after_params[arrow + 2..].trim_start();
            let end = tail.find('{').unwrap_or(tail.len());
            let ty = tail[..end].trim();
            if ty.is_empty() {
                None
            } else {
                Some(ty)
            }
        })
        .unwrap_or("null");
    let params = rest[open + 1..close].trim();
    let parameters = if params.is_empty() {
        Vec::new()
    } else {
        params
            .split(',')
            .map(|param| param.trim().to_string())
            .collect()
    };
    let type_params = type_params
        .filter(|params| !params.is_empty())
        .map(|params| format!("<{params}>"))
        .unwrap_or_default();
    let label = if is_async {
        format!("async fn {name}{type_params}({params}) -> {return_type} (task[{return_type}])")
    } else {
        format!("fn {name}{type_params}({params}) -> {return_type}")
    };
    Some(FunctionSignature { label, parameters })
}

fn receiver_member_completion_members(source: &str, receiver: &str) -> Vec<String> {
    let Some(receiver_type) = binding_type(source, receiver) else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut members = Vec::new();
    for (name, params) in function_declarations(source) {
        if first_param_matches_receiver(&params, &receiver_type) && seen.insert(name.clone()) {
            members.push(name);
        }
    }
    for name in impl_method_names_for_type(source, &receiver_type) {
        if seen.insert(name.clone()) {
            members.push(name);
        }
    }
    members.sort();
    members
}

fn binding_type(source: &str, name: &str) -> Option<String> {
    for line in source.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed
            .strip_prefix("let ")
            .or_else(|| trimmed.strip_prefix("const "))
            .or_else(|| trimmed.strip_prefix("export let "))
            .or_else(|| trimmed.strip_prefix("export const "))
        else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(after_name) = rest.strip_prefix(name) else {
            continue;
        };
        let after_name = after_name.trim_start();
        let Some(after_colon) = after_name.strip_prefix(':') else {
            continue;
        };
        let after_colon = after_colon.trim_start();
        let ty = after_colon
            .split(|c: char| c == '=' || c == ';' || c.is_whitespace())
            .next()
            .unwrap_or("")
            .trim();
        if !ty.is_empty() {
            return Some(ty.to_string());
        }
    }
    None
}

fn function_declarations(source: &str) -> Vec<(String, String)> {
    let mut declarations = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let rest = trimmed
            .strip_prefix("fn ")
            .or_else(|| trimmed.strip_prefix("export fn "))
            .or_else(|| trimmed.strip_prefix("async fn "))
            .or_else(|| trimmed.strip_prefix("export async fn "));
        let Some(rest) = rest else {
            continue;
        };
        let Some(open) = rest.find('(') else {
            continue;
        };
        let Some(close) = rest[open + 1..].find(')') else {
            continue;
        };
        let name = rest[..open].trim();
        if name.is_empty() {
            continue;
        }
        declarations.push((
            name.to_string(),
            rest[open + 1..open + 1 + close].to_string(),
        ));
    }
    declarations
}

fn first_param_matches_receiver(params: &str, receiver_type: &str) -> bool {
    let Some(first) = params.split(',').next() else {
        return false;
    };
    let Some((_, ty)) = first.split_once(':') else {
        return false;
    };
    ty.trim() == receiver_type
}

fn impl_method_names_for_type(source: &str, receiver_type: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut lines = source.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if !(trimmed.starts_with("impl ") && trimmed.contains(&format!(" for {receiver_type}"))) {
            continue;
        }
        for body_line in lines.by_ref() {
            let body = body_line.trim_start();
            if body.starts_with('}') {
                break;
            }
            let Some(rest) = body.strip_prefix("fn ") else {
                continue;
            };
            let Some(open) = rest.find('(') else {
                continue;
            };
            let name = rest[..open].trim();
            if !name.is_empty() {
                names.push(name.to_string());
            }
        }
    }
    names
}

fn top_level_symbols(source: &str) -> Vec<TopLevelSymbol> {
    let mut symbols = Vec::new();
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut depth = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\\' && index + 1 < bytes.len() {
                        index += 2;
                    } else if bytes[index] == b'"' {
                        index += 1;
                        break;
                    } else {
                        index += 1;
                    }
                }
            }
            b'/' if index + 1 < bytes.len() && bytes[index + 1] == b'/' => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'{' => {
                depth += 1;
                index += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                index += 1;
            }
            byte if depth == 0 && is_identifier_start(byte) => {
                let keyword_start = index;
                index += 1;
                while index < bytes.len() && is_identifier_continue(bytes[index]) {
                    index += 1;
                }
                let mut keyword = &source[keyword_start..index];
                let mut exported = false;
                if keyword == "export" {
                    exported = true;
                    index = skip_ws(source, index);
                    let next_start = index;
                    while index < bytes.len() && is_identifier_continue(bytes[index]) {
                        index += 1;
                    }
                    keyword = &source[next_start..index];
                }
                if keyword == "async" {
                    index = skip_ws(source, index);
                    let next_start = index;
                    while index < bytes.len() && is_identifier_continue(bytes[index]) {
                        index += 1;
                    }
                    keyword = &source[next_start..index];
                }
                let kind = match keyword {
                    "fn" => Some(12),
                    "record" => Some(23),
                    "enum" => Some(10),
                    "trait" => Some(11),
                    "type" => Some(5),
                    "let" | "const" => Some(13),
                    _ => None,
                };
                if let Some(kind) = kind {
                    index = skip_ws(source, index);
                    let name_start = index;
                    if index < bytes.len() && is_identifier_start(bytes[index]) {
                        index += 1;
                        while index < bytes.len() && is_identifier_continue(bytes[index]) {
                            index += 1;
                        }
                        symbols.push(TopLevelSymbol {
                            name: source[name_start..index].to_string(),
                            kind,
                            offset: name_start,
                            exported,
                        });
                    }
                }
            }
            _ => index += 1,
        }
    }
    symbols
}

fn module_surface_symbols(source: &str) -> Vec<TopLevelSymbol> {
    let symbols = top_level_symbols(source);
    let has_exports = symbols.iter().any(|symbol| symbol.exported);
    symbols
        .into_iter()
        .filter(|symbol| !has_exports || symbol.exported)
        .collect()
}

fn module_surface_symbol(source: &str, name: &str) -> Option<TopLevelSymbol> {
    module_surface_symbols(source)
        .into_iter()
        .find(|symbol| symbol.name == name)
}

fn skip_ws(source: &str, mut index: usize) -> usize {
    let bytes = source.as_bytes();
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    index
}

fn identifier_at(source: &str, offset: usize) -> Option<String> {
    identifier_bounds_at(source, offset).map(|(_, _, identifier)| identifier)
}

fn identifier_bounds_at(source: &str, offset: usize) -> Option<(usize, usize, String)> {
    let bytes = source.as_bytes();
    if offset > bytes.len() {
        return None;
    }
    let mut start = offset.min(bytes.len());
    if start == bytes.len() || !is_identifier_continue(bytes[start]) {
        start = start.saturating_sub(1);
    }
    if !is_identifier_continue(bytes[start]) {
        return None;
    }
    while start > 0 && is_identifier_continue(bytes[start - 1]) {
        start -= 1;
    }
    if !is_identifier_start(bytes[start]) {
        return None;
    }
    let mut end = start + 1;
    while end < bytes.len() && is_identifier_continue(bytes[end]) {
        end += 1;
    }
    Some((start, end, source[start..end].to_string()))
}

fn namespace_member_alias_at(source: &str, identifier_start: usize) -> Option<String> {
    let bytes = source.as_bytes();
    if identifier_start == 0 {
        return None;
    }
    let dot = identifier_start - 1;
    if bytes.get(dot) != Some(&b'.') {
        return None;
    }
    let mut alias_start = dot;
    while alias_start > 0 && is_identifier_continue(bytes[alias_start - 1]) {
        alias_start -= 1;
    }
    if alias_start == dot || !is_identifier_start(bytes[alias_start]) {
        return None;
    }
    Some(source[alias_start..dot].to_string())
}

fn lexical_tokens(source: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'"' {
            let start = index + 1;
            index += 1;
            let mut literal = String::new();
            while index < bytes.len() {
                match bytes[index] {
                    b'"' => {
                        index += 1;
                        break;
                    }
                    b'\\' if index + 1 < bytes.len() => {
                        literal.push(bytes[index + 1] as char);
                        index += 2;
                    }
                    byte => {
                        literal.push(byte as char);
                        index += 1;
                    }
                }
            }
            if start <= index {
                tokens.push(literal);
            }
            continue;
        }
        if byte == b'/' && index + 1 < bytes.len() && bytes[index + 1] == b'/' {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if matches!(byte, b'{' | b'}') {
            tokens.push((byte as char).to_string());
            index += 1;
            continue;
        }
        if is_identifier_start(byte) {
            let start = index;
            index += 1;
            while index < bytes.len() && is_identifier_continue(bytes[index]) {
                index += 1;
            }
            tokens.push(source[start..index].to_string());
            continue;
        }
        index += 1;
    }
    tokens
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_valid_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && is_identifier_start(bytes[0])
        && bytes[1..].iter().copied().all(is_identifier_continue)
}

fn has_unsafe_rename_shadow(source: &str, symbol: &TopLevelSymbol) -> bool {
    top_level_symbols(source)
        .into_iter()
        .any(|candidate| candidate.name == symbol.name && candidate.offset != symbol.offset)
        || nested_declaration_names(source)
            .into_iter()
            .any(|name| name == symbol.name)
        || parameter_names(source)
            .into_iter()
            .any(|name| name == symbol.name)
}

fn nested_declaration_names(source: &str) -> Vec<String> {
    let tokens = lexical_tokens(source);
    let mut names = Vec::new();
    let mut index = 0;
    let mut depth = 0usize;
    while index < tokens.len() {
        match tokens[index].as_str() {
            "{" => depth += 1,
            "}" => depth = depth.saturating_sub(1),
            "fn" | "let" | "const" if depth > 0 && index + 1 < tokens.len() => {
                names.push(tokens[index + 1].clone());
                index += 1;
            }
            _ => {}
        }
        index += 1;
    }
    names
}

fn parameter_names(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    for symbol in top_level_symbols(source)
        .into_iter()
        .filter(|symbol| symbol.kind == 12)
    {
        let Some(open) = source[symbol.offset + symbol.name.len()..].find('(') else {
            continue;
        };
        let params_start = symbol.offset + symbol.name.len() + open + 1;
        let Some(close) = source[params_start..].find(')') else {
            continue;
        };
        let params = &source[params_start..params_start + close];
        for param in params.split(',') {
            let Some((name, _)) = param.trim().split_once(':') else {
                continue;
            };
            let name = name.trim();
            if is_valid_identifier(name) {
                names.push(name.to_string());
            }
        }
    }
    names
}

fn rename_identifier_ranges(source: &str, name: &str) -> Vec<(usize, usize)> {
    identifier_ranges(source, name)
        .into_iter()
        .filter(|(start, _)| namespace_member_alias_at(source, *start).is_none())
        .collect()
}

fn identifier_ranges(source: &str, name: &str) -> Vec<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut ranges = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\\' && index + 1 < bytes.len() {
                        index += 2;
                    } else if bytes[index] == b'"' {
                        index += 1;
                        break;
                    } else {
                        index += 1;
                    }
                }
            }
            b'/' if index + 1 < bytes.len() && bytes[index + 1] == b'/' => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            byte if is_identifier_start(byte) => {
                let start = index;
                index += 1;
                while index < bytes.len() && is_identifier_continue(bytes[index]) {
                    index += 1;
                }
                if &source[start..index] == name {
                    ranges.push((start, index));
                }
            }
            _ => index += 1,
        }
    }
    ranges
}

fn file_uri_base(uri: &str) -> Option<PathBuf> {
    let path = uri.strip_prefix("file://")?;
    Path::new(path).parent().map(Path::to_path_buf)
}

fn file_uri_path(uri: &str) -> Option<PathBuf> {
    uri.strip_prefix("file://").map(PathBuf::from)
}

fn path_to_file_uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

fn diagnostic_json(diagnostic: &Diagnostic, source: &str) -> String {
    let (start_line, start_character) = line_character(source, diagnostic.span.start);
    let (end_line, end_character) = line_character(source, diagnostic.span.end);
    let message = diagnostic_message_with_stack(diagnostic);
    let data = lsp_diagnostic_data(
        "error",
        diagnostic.code,
        &diagnostic.message,
        diagnostic.span,
        source,
        &diagnostic.stack_frames,
    );
    format!(
        "{{\"range\":{{\"start\":{{\"line\":{start_line},\"character\":{start_character}}},\"end\":{{\"line\":{end_line},\"character\":{end_character}}}}},\"severity\":1,\"source\":\"nox\",\"code\":\"{}\",\"message\":\"{}\"{data}}}",
        json_escape(diagnostic.code),
        json_escape(&message)
    )
}

fn diagnostic_message_with_stack(diagnostic: &Diagnostic) -> String {
    if diagnostic.stack_frames.is_empty() {
        return diagnostic.message.clone();
    }
    let mut message = diagnostic.message.clone();
    message.push_str("\nstack:");
    for frame in &diagnostic.stack_frames {
        let kind = frame.kind.as_str();
        if let Some(source) = &frame.source {
            message.push_str(&format!(
                "\n  at {} [{}] ({}:{}:{})",
                frame.name, kind, source.name, source.line, source.column
            ));
        } else {
            message.push_str(&format!(
                "\n  at {} [{}] ({}..{})",
                frame.name, kind, frame.span.start, frame.span.end
            ));
        }
    }
    message
}

fn lsp_diagnostic_data(
    kind: &str,
    code: &str,
    message: &str,
    span: Span,
    source: &str,
    stack_frames: &[nox_core::StackFrame],
) -> String {
    let trace_id = stable_lsp_trace_id(kind, code, message, span, source);
    let mut data = format!(",\"data\":{{\"trace_id\":\"{trace_id}\"");
    if !stack_frames.is_empty() {
        data.push_str(",\"stack_frames\":[");
    }
    for (index, frame) in stack_frames.iter().enumerate() {
        if index > 0 {
            data.push(',');
        }
        data.push_str(&format!(
            "{{\"name\":\"{}\",\"kind\":\"{}\",\"span\":{{\"start\":{},\"end\":{}}}",
            json_escape(&frame.name),
            frame.kind.as_str(),
            frame.span.start,
            frame.span.end
        ));
        if let Some(source) = &frame.source {
            data.push_str(&format!(
                ",\"source\":{{\"name\":\"{}\",\"line\":{},\"column\":{}}}",
                json_escape(&source.name),
                source.line,
                source.column
            ));
        } else {
            data.push_str(",\"source\":null");
        }
        data.push('}');
    }
    if !stack_frames.is_empty() {
        data.push(']');
    }
    data.push('}');
    data
}

fn stable_lsp_trace_id(kind: &str, code: &str, message: &str, span: Span, source: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for part in [
        kind,
        code,
        message,
        &span.start.to_string(),
        &span.end.to_string(),
        source,
    ] {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("lsp-{hash:016x}")
}

fn read_message(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }
    let Some(content_length) = content_length else {
        return Ok(None);
    };
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    Ok(Some(String::from_utf8_lossy(&body).into_owned()))
}

fn write_message(output: &mut impl Write, body: &str) -> io::Result<()> {
    write!(output, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    output.flush()
}

fn json_id(message: &str) -> Option<String> {
    let value = value_after_key(message, "id")?;
    if let Some(rest) = value.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some(format!("\"{}\"", json_escape(&rest[..end])));
    }
    let id = value
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

fn json_string_field(message: &str, key: &str) -> Option<String> {
    let value = value_after_key(message, key)?;
    decode_json_string(value)
}

fn json_last_string_field(message: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let index = message.rfind(&pattern)?;
    let after_key = &message[index + pattern.len()..];
    let colon = after_key.find(':')?;
    decode_json_string(after_key[colon + 1..].trim_start())
}

fn json_number_field(message: &str, key: &str) -> Option<usize> {
    let value = value_after_key(message, key)?;
    let number = value
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    number.parse().ok()
}

fn value_after_key<'a>(message: &'a str, key: &str) -> Option<&'a str> {
    let pattern = format!("\"{key}\"");
    let index = message.find(&pattern)?;
    let after_key = &message[index + pattern.len()..];
    let colon = after_key.find(':')?;
    Some(after_key[colon + 1..].trim_start())
}

fn decode_json_string(value: &str) -> Option<String> {
    let mut chars = value.strip_prefix('"')?.chars();
    let mut decoded = String::new();
    while let Some(character) = chars.next() {
        match character {
            '"' => return Some(decoded),
            '\\' => match chars.next()? {
                '"' => decoded.push('"'),
                '\\' => decoded.push('\\'),
                '/' => decoded.push('/'),
                'b' => decoded.push('\u{0008}'),
                'f' => decoded.push('\u{000c}'),
                'n' => decoded.push('\n'),
                'r' => decoded.push('\r'),
                't' => decoded.push('\t'),
                other => decoded.push(other),
            },
            character => decoded.push(character),
        }
    }
    None
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                write!(&mut escaped, "\\u{:04x}", character as u32)
                    .expect("writing to String cannot fail");
            }
            character => escaped.push(character),
        }
    }
    escaped
}

fn line_character(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 0;
    let mut character = 0;
    for (index, ch) in source.char_indices() {
        if index >= byte_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }
    (line, character)
}

fn byte_offset(source: &str, target_line: usize, target_character: usize) -> Option<usize> {
    let mut line = 0;
    let mut character = 0;
    for (index, ch) in source.char_indices() {
        if line == target_line && character == target_character {
            return Some(index);
        }
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }
    if line == target_line && character == target_character {
        Some(source.len())
    } else {
        None
    }
}
