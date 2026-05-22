use std::{
    collections::HashMap,
    fmt::Write as _,
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
};

use nox_core::{Diagnostic, Session, Span};

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
                    "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"capabilities\":{{\"textDocumentSync\":1,\"hoverProvider\":true,\"documentFormattingProvider\":true,\"completionProvider\":{{\"triggerCharacters\":[\".\"]}}}}}}}}"
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

    fn completion_response(&self, id: &str, uri: &str, line: usize, character: usize) -> String {
        let Some(source) = self.documents.get(uri) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[]}}");
        };
        let Some(offset) = byte_offset(source, line, character) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":[]}}");
        };
        let prefix_source = &source[..offset];
        if let Some(alias) = namespace_completion_alias(prefix_source) {
            let items = self
                .namespace_completion_members(uri, source, &alias)
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
        let Some(source) = self.documents.get(uri) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let Some(offset) = byte_offset(source, line, character) else {
            return format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}");
        };
        let overlay = self.documents_overlay();
        let result = if let Some(base) = file_uri_base(uri) {
            let session = self.sessions.entry(uri.to_string()).or_default();
            install_lsp_stdlib(session);
            check_session_loader(session, base.clone(), overlay.clone());
            session.hover_type(source, offset)
        } else {
            let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
            runtime.hover_type_source(source, offset)
        };
        match result {
            Ok(Some(ty)) => format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"contents\":{{\"kind\":\"plaintext\",\"value\":\"{}\"}}}}}}",
                json_escape(&ty.to_string())
            ),
            Ok(None) | Err(_) => {
                format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}")
            }
        }
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
        let result = if let Some(base) = file_uri_base(uri) {
            let session = self.sessions.entry(uri.to_string()).or_default();
            install_lsp_stdlib(session);
            check_session_loader(session, base, overlay);
            session.check_diagnostics(source)
        } else {
            let mut runtime = Runtime::with_permissions(RuntimePermissions::cli());
            runtime.check_source_diagnostics(source)
        };
        let diagnostics = result.err().unwrap_or_default();
        let diagnostics = diagnostics
            .iter()
            .map(|diagnostic| diagnostic_json(diagnostic, source))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/publishDiagnostics\",\"params\":{{\"uri\":\"{}\",\"diagnostics\":[{diagnostics}]}}}}",
            json_escape(uri)
        )
    }
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
    format!(
        "{{\"range\":{{\"start\":{{\"line\":{start_line},\"character\":{start_character}}},\"end\":{{\"line\":{end_line},\"character\":{end_character}}}}},\"severity\":1,\"source\":\"nox\",\"code\":\"{}\",\"message\":\"{}\"}}",
        json_escape(diagnostic.code),
        json_escape(&diagnostic.message)
    )
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
