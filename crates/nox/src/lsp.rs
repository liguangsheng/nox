use std::{
    collections::HashMap,
    fmt::Write as _,
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
};

use nox_core::{Diagnostic, HostFunctionSignature, LintWarning, Session, Span};

use crate::{
    install_lsp_stdlib, manifest::Manifest, std_module_source, Runtime, RuntimePermissions,
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
    shutdown: bool,
    exited: bool,
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
                vec![format!(
                    "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"capabilities\":{{\"textDocumentSync\":1,\"hoverProvider\":true,\"definitionProvider\":true,\"documentSymbolProvider\":true,\"documentFormattingProvider\":true,\"completionProvider\":{{\"triggerCharacters\":[\".\"]}},\"signatureHelpProvider\":{{\"triggerCharacters\":[\"(\",\",\"]}},\"codeActionProvider\":true,\"codeLensProvider\":{{\"resolveProvider\":false}}}}}}}}"
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

    fn publish_all_diagnostics(&mut self) -> Vec<String> {
        let documents = self
            .documents
            .iter()
            .map(|(uri, text)| (uri.clone(), text.clone()))
            .collect::<Vec<_>>();
        documents
            .into_iter()
            .map(|(uri, text)| self.publish_diagnostics(&uri, &text))
            .collect()
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

    fn definition_response(&self, id: &str, uri: &str, line: usize, character: usize) -> String {
        let Some(source) = self.documents.get(uri) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let Some(offset) = byte_offset(source, line, character) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let Some(identifier) = identifier_at(source, offset) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let Some(symbol) = top_level_symbols(source)
            .into_iter()
            .find(|symbol| symbol.name == identifier)
        else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let (line, character) = line_character(source, symbol.offset);
        let end_character = character + symbol.name.len();
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"uri\":\"{}\",\"range\":{{\"start\":{{\"line\":{line},\"character\":{character}}},\"end\":{{\"line\":{line},\"character\":{end_character}}}}}}}}}",
            json_escape(uri)
        )
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
        if let Some(alias) = namespace_completion_alias(prefix_source) {
            let items = self
                .namespace_completion_members(uri, &source, &alias)
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
            "let", "fn", "return", "if", "else", "while", "for", "in", "break", "continue",
            "import", "export", "record", "true", "false", "null",
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
        let doc_comment = identifier
            .as_deref()
            .and_then(|name| doc_comment_for_top_level(&source, name));
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
                None => format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}"),
            },
        }
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
        if source.contains("TODO") {
            actions.push(format!(
                "{{\"title\":\"Remove TODO marker\",\"kind\":\"quickfix\",\"edit\":{{\"changes\":{{\"{}\":[{{\"range\":{{\"start\":{{\"line\":0,\"character\":0}},\"end\":{{\"line\":0,\"character\":0}}}},\"newText\":\"\"}}]}}}}}}",
                json_escape(uri)
            ));
        } else {
            actions.push(format!(
                "{{\"title\":\"Run nox check\",\"kind\":\"source\",\"command\":{{\"title\":\"Run nox check\",\"command\":\"nox.check\",\"arguments\":[\"{}\"]}}}}",
                json_escape(uri)
            ));
        }
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

fn declaration_matches(trimmed: &str, name: &str) -> bool {
    let pattern_pairs = [
        ("export fn ", "("),
        ("fn ", "("),
        ("export record ", " {"),
        ("record ", " {"),
        ("export enum ", " {"),
        ("enum ", " {"),
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

fn module_surface_members(source: &str) -> Vec<String> {
    let tokens = lexical_tokens(source);
    let mut declarations = Vec::new();
    let mut has_exports = false;
    let mut index = 0;
    let mut depth = 0usize;
    while index < tokens.len() {
        if tokens[index] == "{" {
            depth += 1;
            index += 1;
            continue;
        }
        if tokens[index] == "}" {
            depth = depth.saturating_sub(1);
            index += 1;
            continue;
        }
        if depth != 0 {
            index += 1;
            continue;
        }
        let mut exported = false;
        if tokens[index] == "export" {
            exported = true;
            has_exports = true;
            index += 1;
        }
        if index + 1 < tokens.len()
            && matches!(tokens[index].as_str(), "fn" | "let" | "const" | "record")
        {
            declarations.push((tokens[index + 1].clone(), exported));
            index += 2;
            continue;
        }
        index += 1;
    }
    declarations
        .into_iter()
        .filter_map(|(name, exported)| {
            if !has_exports || exported {
                Some(name)
            } else {
                None
            }
        })
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

struct TopLevelSymbol {
    name: String,
    kind: u8,
    offset: usize,
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
    let needle = format!("fn {name}(");
    let start = source.find(&needle)? + "fn ".len();
    let rest = &source[start..];
    let open = rest.find('(')?;
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
    Some(FunctionSignature {
        label: format!("fn {name}({params}) -> {return_type}"),
        parameters,
    })
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
                if keyword == "export" {
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
                        });
                    }
                }
            }
            _ => index += 1,
        }
    }
    symbols
}

fn skip_ws(source: &str, mut index: usize) -> usize {
    let bytes = source.as_bytes();
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    index
}

fn identifier_at(source: &str, offset: usize) -> Option<String> {
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
    Some(source[start..end].to_string())
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

fn file_uri_base(uri: &str) -> Option<PathBuf> {
    let path = uri.strip_prefix("file://")?;
    Path::new(path).parent().map(Path::to_path_buf)
}

fn file_uri_path(uri: &str) -> Option<PathBuf> {
    uri.strip_prefix("file://").map(PathBuf::from)
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
