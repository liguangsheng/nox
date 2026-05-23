use std::collections::BTreeMap;
use std::io::{self, BufRead, BufReader, Read, Write};

#[derive(Default)]
struct DapSession {
    breakpoints: BTreeMap<String, Vec<DapBreakpoint>>,
    exception_filters: Vec<String>,
    result: String,
    launch_error: Option<String>,
    condition_checks: usize,
    condition_matches: usize,
    seq: u64,
}

#[derive(Clone)]
struct DapBreakpoint {
    line: u64,
    condition: Option<String>,
}

pub fn run_stdio() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    run(stdin.lock(), stdout.lock())
}

fn run<R: Read, W: Write>(reader: R, mut writer: W) -> io::Result<()> {
    let mut reader = BufReader::new(reader);
    let mut session = DapSession::default();
    while let Some(message) = read_message(&mut reader)? {
        for response in handle_message(&mut session, &message) {
            write_message(&mut writer, &response)?;
        }
        if message.contains(r#""command":"disconnect""#) {
            break;
        }
    }
    Ok(())
}

fn read_message<R: BufRead>(reader: &mut R) -> io::Result<Option<String>> {
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

fn write_message<W: Write>(writer: &mut W, body: &str) -> io::Result<()> {
    write!(writer, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    writer.flush()
}

fn handle_message(session: &mut DapSession, message: &str) -> Vec<String> {
    let seq = number_field(message, "seq").unwrap_or(0);
    let Some(command) = string_field(message, "command") else {
        return Vec::new();
    };
    match command.as_str() {
        "initialize" => vec![
            response(
                session,
                seq,
                "initialize",
                r#""body":{"supportsConfigurationDoneRequest":true,"supportsSteppingGranularity":false,"supportsConditionalBreakpoints":true,"exceptionBreakpointFilters":[{"filter":"raised","label":"Raised errors","default":false}]}"#,
            ),
            event(session, "initialized", ""),
        ],
        "setBreakpoints" => {
            let source = nested_string_field(message, "source", "path")
                .unwrap_or_else(|| "<memory>".to_string());
            let breakpoints = parse_breakpoints(message);
            session.breakpoints.insert(source, breakpoints.clone());
            let body = format!(
                r#""body":{{"breakpoints":[{}]}}"#,
                breakpoints
                    .iter()
                    .map(|breakpoint| {
                        let condition = breakpoint
                            .condition
                            .as_ref()
                            .map(|condition| {
                                format!(r#","condition":"{}""#, escape_json(condition))
                            })
                            .unwrap_or_default();
                        format!(
                            r#"{{"verified":true,"line":{}{condition}}}"#,
                            breakpoint.line
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            );
            vec![response(session, seq, "setBreakpoints", &body)]
        }
        "setExceptionBreakpoints" => {
            session.exception_filters = string_array_field(message, "filters");
            vec![response(session, seq, "setExceptionBreakpoints", "")]
        }
        "launch" => {
            match launch_result(message) {
                Ok(result) => {
                    session.result = result;
                    session.launch_error = None;
                }
                Err(err) => {
                    session.result = "launch error".to_string();
                    session.launch_error = Some(err);
                }
            }
            vec![response(session, seq, "launch", "")]
        }
        "configurationDone" => {
            let mut responses = vec![response(session, seq, "configurationDone", "")];
            if session.launch_error.is_some()
                && session
                    .exception_filters
                    .iter()
                    .any(|filter| filter == "raised")
            {
                responses.push(event(
                    session,
                    "stopped",
                    r#""body":{"reason":"exception","threadId":1,"allThreadsStopped":true,"description":"raised error"}"#,
                ));
            } else {
                let matches = evaluate_breakpoints(session);
                if matches > 0 {
                    responses.push(event(
                        session,
                        "stopped",
                        &format!(
                            r#""body":{{"reason":"breakpoint","threadId":1,"allThreadsStopped":true,"hitBreakpointIds":[1],"description":"conditional breakpoint matched ({matches})"}}"#
                        ),
                    ));
                } else {
                    responses.push(event(session, "terminated", ""));
                }
            }
            responses
        }
        "threads" => vec![response(
            session,
            seq,
            "threads",
            r#""body":{"threads":[{"id":1,"name":"main"}]}"#,
        )],
        "stackTrace" => vec![response(
            session,
            seq,
            "stackTrace",
            r#""body":{"stackFrames":[{"id":1,"name":"main","line":1,"column":1}],"totalFrames":1}"#,
        )],
        "scopes" => vec![response(
            session,
            seq,
            "scopes",
            r#""body":{"scopes":[{"name":"Locals","variablesReference":1,"expensive":false}]}"#,
        )],
        "variables" => {
            let variables_reference = number_field(message, "variablesReference").unwrap_or(0);
            let depth = number_field(message, "depth")
                .or_else(|| number_field(message, "maxDepth"))
                .unwrap_or(1);
            vec![response(
                session,
                seq,
                "variables",
                &variables_body(session, variables_reference, depth),
            )]
        }
        "continue" | "next" | "stepIn" | "stepOut" => vec![
            response(
                session,
                seq,
                &command,
                r#""body":{"allThreadsContinued":true}"#,
            ),
            event(session, "terminated", ""),
        ],
        "disconnect" => vec![response(session, seq, "disconnect", "")],
        _ => vec![format!(
            r#"{{"seq":{},"type":"response","request_seq":{seq},"success":false,"command":"{}","message":"unsupported DAP command"}}"#,
            session.next_seq(),
            escape_json(&command)
        )],
    }
}

fn response(session: &mut DapSession, request_seq: u64, command: &str, extra: &str) -> String {
    let extra = if extra.is_empty() {
        String::new()
    } else {
        format!(",{extra}")
    };
    format!(
        r#"{{"seq":{},"type":"response","request_seq":{request_seq},"success":true,"command":"{command}"{extra}}}"#,
        session.next_seq()
    )
}

fn event(session: &mut DapSession, name: &str, extra: &str) -> String {
    let extra = if extra.is_empty() {
        String::new()
    } else {
        format!(",{extra}")
    };
    format!(
        r#"{{"seq":{},"type":"event","event":"{name}"{extra}}}"#,
        session.next_seq()
    )
}

impl DapSession {
    fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }
}

fn launch_result(message: &str) -> Result<String, String> {
    let Some(program) = string_field(message, "program") else {
        return Ok("launch prepared".to_string());
    };
    crate::Runtime::with_permissions(crate::RuntimePermissions::cli())
        .eval_file(&program)
        .map(|value| value.to_string())
        .map_err(|err| err.message)
}

fn evaluate_breakpoints(session: &mut DapSession) -> usize {
    let mut checks = 0usize;
    let mut matches = 0usize;
    for breakpoint in session.breakpoints.values().flatten() {
        checks += 1;
        if breakpoint
            .condition
            .as_deref()
            .map(|condition| evaluate_condition(condition, &session.result))
            .unwrap_or(true)
        {
            matches += 1;
        }
    }
    session.condition_checks = checks;
    session.condition_matches = matches;
    matches
}

fn evaluate_condition(condition: &str, result: &str) -> bool {
    let condition = condition.trim();
    if let Some(expected) = condition.strip_prefix("result == ") {
        return condition_value_equals(result, expected);
    }
    if let Some(expected) = condition.strip_prefix("result != ") {
        return !condition_value_equals(result, expected);
    }
    false
}

fn condition_value_equals(result: &str, expected: &str) -> bool {
    let expected = expected.trim();
    if let Some(quoted) = expected
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    {
        return result == quoted;
    }
    result == expected
}

fn variables_body(session: &DapSession, variables_reference: u64, depth: u64) -> String {
    match variables_reference {
        1 => {
            let debug_state_reference = if depth == 0 { 0 } else { 2 };
            let debug_state_value = if depth == 0 {
                "depth limit reached"
            } else {
                "expanded"
            };
            format!(
                r#""body":{{"variables":[{{"name":"result","value":"{}","variablesReference":0}},{{"name":"breakpoints","value":"{}","variablesReference":0}},{{"name":"exceptionFilters","value":"{}","variablesReference":0}},{{"name":"maxDepth","value":"{}","variablesReference":0}},{{"name":"debugState","value":"{}","variablesReference":{debug_state_reference}}}]}}"#,
                escape_json(&session.result),
                session.breakpoints.values().map(Vec::len).sum::<usize>(),
                session.exception_filters.len(),
                depth,
                debug_state_value
            )
        }
        2 if depth > 1 => format!(
            r#""body":{{"variables":[{{"name":"resultPreview","value":"{}","variablesReference":0}},{{"name":"breakpointSources","value":"{}","variablesReference":0}},{{"name":"conditionChecks","value":"{}","variablesReference":0}},{{"name":"conditionMatches","value":"{}","variablesReference":0}},{{"name":"exceptionFilters","value":"{}","variablesReference":0}},{{"name":"exceptionMessage","value":"{}","variablesReference":0}}]}}"#,
            escape_json(&session.result),
            session.breakpoints.len(),
            session.condition_checks,
            session.condition_matches,
            session.exception_filters.join(","),
            escape_json(session.launch_error.as_deref().unwrap_or(""))
        ),
        2 => {
            r#""body":{"variables":[{"name":"depthLimit","value":"reached","variablesReference":0}]}"#
                .to_string()
        }
        _ => r#""body":{"variables":[]}"#.to_string(),
    }
}

fn number_field(message: &str, name: &str) -> Option<u64> {
    let marker = format!("\"{name}\":");
    let start = message.find(&marker)? + marker.len();
    let rest = &message[start..];
    let end = rest
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn string_field(message: &str, name: &str) -> Option<String> {
    let marker = format!("\"{name}\":\"");
    let start = message.find(&marker)? + marker.len();
    let rest = &message[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn nested_string_field(message: &str, object: &str, name: &str) -> Option<String> {
    let object_start = message.find(&format!("\"{object}\":"))?;
    string_field(&message[object_start..], name)
}

fn parse_breakpoints(message: &str) -> Vec<DapBreakpoint> {
    let mut breakpoints = Vec::new();
    let mut rest = message;
    while let Some(index) = rest.find("\"line\":") {
        rest = &rest[index + 7..];
        let end = rest
            .find(|ch: char| !ch.is_ascii_digit())
            .unwrap_or(rest.len());
        if let Ok(line) = rest[..end].parse() {
            let segment = &rest[..rest[end..]
                .find("\"line\":")
                .map_or(rest.len(), |next| end + next)];
            breakpoints.push(DapBreakpoint {
                line,
                condition: string_field(segment, "condition"),
            });
        }
        rest = &rest[end..];
    }
    breakpoints
}

fn string_array_field(message: &str, name: &str) -> Vec<String> {
    let Some(start) = message.find(&format!("\"{name}\":[")) else {
        return Vec::new();
    };
    let mut values = Vec::new();
    let mut rest = &message[start + name.len() + 4..];
    while let Some(index) = rest.find('"') {
        rest = &rest[index + 1..];
        let Some(end) = rest.find('"') else {
            break;
        };
        values.push(rest[..end].to_string());
        rest = &rest[end + 1..];
        if rest.trim_start().starts_with(']') {
            break;
        }
    }
    values
}

fn escape_json(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
