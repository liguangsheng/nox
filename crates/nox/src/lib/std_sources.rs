use nox_core::{Diagnostic, Span};

pub(crate) fn std_module_source(specifier: &str) -> Result<Option<&'static str>, Diagnostic> {
    let source = match specifier {
        "std/fs.nox" => include_str!("std/fs.nox"),
        "std/path.nox" => include_str!("std/path.nox"),
        "std/env.nox" => include_str!("std/env.nox"),
        "std/process.nox" => include_str!("std/process.nox"),
        "std/time.nox" => include_str!("std/time.nox"),
        "std/string.nox" => include_str!("std/string.nox"),
        "std/json.nox" => include_str!("std/json.nox"),
        "std/jsonl.nox" => include_str!("std/jsonl.nox"),
        "std/csv.nox" => include_str!("std/csv.nox"),
        "std/tsv.nox" => include_str!("std/tsv.nox"),
        "std/hash.nox" => include_str!("std/hash.nox"),
        "std/traits.nox" => include_str!("std/traits.nox"),
        "std/array.nox" => include_str!("std/array.nox"),
        "std/map.nox" => include_str!("std/map.nox"),
        "std/option.nox" => include_str!("std/option.nox"),
        "std/result.nox" => include_str!("std/result.nox"),
        "std/term.nox" => include_str!("std/term.nox"),
        "std/bytes.nox" => include_str!("std/bytes.nox"),
        "std/encoding.nox" => include_str!("std/encoding.nox"),
        "std/dotenv.nox" => include_str!("std/dotenv.nox"),
        "std/ini.nox" => include_str!("std/ini.nox"),
        "std/toml.nox" => include_str!("std/toml.nox"),
        "std/yaml.nox" => include_str!("std/yaml.nox"),
        "std/xml.nox" => include_str!("std/xml.nox"),
        "std/test.nox" => include_str!("std/test.nox"),
        "std/task.nox" => include_str!("std/task.nox"),
        "std/http.nox" => include_str!("std/http.nox"),
        "std/random.nox" => include_str!("std/random.nox"),
        "std/url.nox" => include_str!("std/url.nox"),
        _ if specifier.starts_with("std/") => {
            return Err(Diagnostic::new(
                format!("standard module '{specifier}' is not provided by this runtime"),
                Span { start: 0, end: 0 },
            )
            .with_code("module.not-found"));
        }
        _ => return Ok(None),
    };
    Ok(Some(source))
}
