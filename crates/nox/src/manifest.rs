use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};

use nox_core::{Diagnostic, Span};

pub const MANIFEST_FILE_NAME: &str = "nox.toml";

#[derive(Debug, Clone)]
pub struct Manifest {
    pub root: PathBuf,
    pub package: PackageInfo,
    pub entrypoints: Entrypoints,
    pub modules: Modules,
    pub runtime: RuntimeDecl,
}

#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Entrypoints {
    pub main: Option<PathBuf>,
    pub named: Vec<(String, PathBuf)>,
}

#[derive(Debug, Clone, Default)]
pub struct Modules {
    pub source_dirs: Vec<PathBuf>,
    pub test_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeDecl {
    pub permissions: Vec<RuntimePermissionDecl>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimePermissionDecl {
    FilesystemRead,
    FilesystemWrite,
    Network,
    Timers,
    Environment,
    AsyncTasks,
}

impl Manifest {
    pub fn discover(start: &Path) -> Result<Option<Manifest>, Diagnostic> {
        let mut current = if start.is_absolute() {
            start.to_path_buf()
        } else {
            match std::env::current_dir() {
                Ok(cwd) => cwd.join(start),
                Err(_) => start.to_path_buf(),
            }
        };

        if current.is_file() {
            current = current
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
        }

        loop {
            let candidate = current.join(MANIFEST_FILE_NAME);
            if candidate.is_file() {
                return Manifest::load(&candidate).map(Some);
            }
            if !current.pop() {
                return Ok(None);
            }
        }
    }

    pub fn load(path: &Path) -> Result<Manifest, Diagnostic> {
        let source = fs::read_to_string(path).map_err(|err| {
            Diagnostic::new(
                format!("failed to read manifest '{}': {err}", path.display()),
                Span { start: 0, end: 0 },
            )
        })?;
        let root = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        Manifest::parse(&source, root)
    }

    pub fn parse(source: &str, root: PathBuf) -> Result<Manifest, Diagnostic> {
        let document = parse_toml(source)?;

        let package = document
            .section("package")
            .ok_or_else(|| manifest_error("missing required section [package]"))?;
        let name = package
            .require_string("name")?
            .ok_or_else(|| manifest_error("[package] is missing required key 'name'"))?;
        let version = package
            .require_string("version")?
            .ok_or_else(|| manifest_error("[package] is missing required key 'version'"))?;
        let description = package.require_string("description")?;

        let mut entrypoints = Entrypoints::default();
        if let Some(section) = document.section("entrypoints") {
            for (key, value) in &section.entries {
                match value {
                    TomlValue::String(path) => {
                        if key == "main" {
                            entrypoints.main = Some(PathBuf::from(path));
                        } else {
                            entrypoints.named.push((key.clone(), PathBuf::from(path)));
                        }
                    }
                    _ => {
                        return Err(manifest_error(format!(
                            "manifest key 'entrypoints.{key}' must be a string"
                        )));
                    }
                }
            }
        }

        let mut modules = Modules::default();
        if let Some(section) = document.section("modules") {
            if let Some(values) = section.require_string_array("source_dirs")? {
                modules.source_dirs = validate_relative_paths("modules.source_dirs", values)?;
            }
            if let Some(values) = section.require_string_array("test_dirs")? {
                modules.test_dirs = validate_relative_paths("modules.test_dirs", values)?;
            }
        }

        let mut runtime = RuntimeDecl::default();
        if let Some(section) = document.section("runtime") {
            if let Some(values) = section.require_string_array("permissions")? {
                runtime.permissions = values
                    .into_iter()
                    .map(|permission| parse_runtime_permission(&permission))
                    .collect::<Result<Vec<_>, _>>()?;
            }
        }

        Ok(Manifest {
            root,
            package: PackageInfo {
                name,
                version,
                description,
            },
            entrypoints,
            modules,
            runtime,
        })
    }

    pub fn main_path(&self) -> Option<PathBuf> {
        self.entrypoints
            .main
            .as_ref()
            .map(|path| self.root.join(path))
    }

    pub fn source_dirs(&self) -> Vec<PathBuf> {
        self.modules
            .source_dirs
            .iter()
            .map(|path| self.root.join(path))
            .collect()
    }

    pub fn test_dirs(&self) -> Vec<PathBuf> {
        self.modules
            .test_dirs
            .iter()
            .map(|path| self.root.join(path))
            .collect()
    }
}

fn parse_runtime_permission(value: &str) -> Result<RuntimePermissionDecl, Diagnostic> {
    match value {
        "filesystem.read" => Ok(RuntimePermissionDecl::FilesystemRead),
        "filesystem.write" => Ok(RuntimePermissionDecl::FilesystemWrite),
        "network" => Ok(RuntimePermissionDecl::Network),
        "timers" => Ok(RuntimePermissionDecl::Timers),
        "environment" => Ok(RuntimePermissionDecl::Environment),
        "async_tasks" => Ok(RuntimePermissionDecl::AsyncTasks),
        _ => Err(manifest_error(format!(
            "manifest key 'runtime.permissions' contains unknown permission '{value}'"
        ))),
    }
}

fn manifest_error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(message, Span { start: 0, end: 0 }).with_code("manifest.invalid")
}

fn validate_relative_paths(key: &str, values: Vec<String>) -> Result<Vec<PathBuf>, Diagnostic> {
    let mut seen = BTreeSet::new();
    let mut paths = Vec::new();
    for value in values {
        let path = PathBuf::from(&value);
        if path.as_os_str().is_empty() {
            return Err(manifest_error(format!(
                "manifest key '{key}' contains an empty path"
            )));
        }
        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => normalized.push(part),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(manifest_error(format!(
                        "manifest key '{key}' path '{value}' must stay within the project root"
                    )));
                }
            }
        }
        if path.is_absolute() || normalized.as_os_str().is_empty() {
            return Err(manifest_error(format!(
                "manifest key '{key}' path '{value}' must stay within the project root"
            )));
        }
        if !seen.insert(normalized.clone()) {
            return Err(manifest_error(format!(
                "manifest key '{key}' contains duplicate path '{}'",
                normalized.display()
            )));
        }
        paths.push(normalized);
    }
    Ok(paths)
}

struct TomlDocument {
    sections: Vec<TomlSection>,
}

impl TomlDocument {
    fn section(&self, name: &str) -> Option<&TomlSection> {
        self.sections.iter().find(|section| section.name == name)
    }
}

struct TomlSection {
    name: String,
    entries: Vec<(String, TomlValue)>,
}

impl TomlSection {
    fn get(&self, key: &str) -> Option<&TomlValue> {
        self.entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    fn require_string(&self, key: &str) -> Result<Option<String>, Diagnostic> {
        match self.get(key) {
            None => Ok(None),
            Some(TomlValue::String(value)) => Ok(Some(value.clone())),
            Some(_) => Err(manifest_error(format!(
                "manifest key '{}.{}' must be a string",
                self.name, key
            ))),
        }
    }

    fn require_string_array(&self, key: &str) -> Result<Option<Vec<String>>, Diagnostic> {
        match self.get(key) {
            None => Ok(None),
            Some(TomlValue::Array(values)) => {
                let mut collected = Vec::with_capacity(values.len());
                for value in values {
                    match value {
                        TomlValue::String(string) => collected.push(string.clone()),
                        _ => {
                            return Err(manifest_error(format!(
                                "manifest key '{}.{}' must be a string array",
                                self.name, key
                            )));
                        }
                    }
                }
                Ok(Some(collected))
            }
            Some(_) => Err(manifest_error(format!(
                "manifest key '{}.{}' must be a string array",
                self.name, key
            ))),
        }
    }
}

enum TomlValue {
    String(String),
    Array(Vec<TomlValue>),
}

fn parse_toml(source: &str) -> Result<TomlDocument, Diagnostic> {
    let mut sections: Vec<TomlSection> = Vec::new();
    let mut current: Option<TomlSection> = None;

    for (line_index, raw_line) in source.lines().enumerate() {
        let line_number = line_index + 1;
        let trimmed = strip_comment(raw_line).trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix('[') {
            let Some(name) = rest.strip_suffix(']') else {
                return Err(manifest_error(format!(
                    "manifest line {line_number}: section header missing ']'"
                )));
            };
            let name = name.trim();
            if name.is_empty() {
                return Err(manifest_error(format!(
                    "manifest line {line_number}: section header is empty"
                )));
            }
            if let Some(finished) = current.take() {
                sections.push(finished);
            }
            if sections.iter().any(|section| section.name == name) {
                return Err(manifest_error(format!(
                    "manifest line {line_number}: section [{name}] appears more than once"
                )));
            }
            current = Some(TomlSection {
                name: name.to_string(),
                entries: Vec::new(),
            });
            continue;
        }

        let Some((key, value)) = trimmed.split_once('=') else {
            return Err(manifest_error(format!(
                "manifest line {line_number}: expected 'key = value'"
            )));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(manifest_error(format!(
                "manifest line {line_number}: empty key"
            )));
        }
        if !key.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        }) {
            return Err(manifest_error(format!(
                "manifest line {line_number}: key '{key}' contains unsupported characters"
            )));
        }
        let Some(section) = current.as_mut() else {
            return Err(manifest_error(format!(
                "manifest line {line_number}: key '{key}' is outside any [section]"
            )));
        };
        if section.entries.iter().any(|(name, _)| name == key) {
            return Err(manifest_error(format!(
                "manifest line {line_number}: duplicate key '{}.{key}'",
                section.name
            )));
        }
        let value = parse_value(value.trim(), line_number)?;
        section.entries.push((key.to_string(), value));
    }

    if let Some(finished) = current.take() {
        sections.push(finished);
    }

    Ok(TomlDocument { sections })
}

fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    for (index, character) in line.char_indices() {
        match character {
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..index],
            _ => {}
        }
    }
    line
}

fn parse_value(text: &str, line_number: usize) -> Result<TomlValue, Diagnostic> {
    if let Some(rest) = text.strip_prefix('[') {
        let Some(inner) = rest.strip_suffix(']') else {
            return Err(manifest_error(format!(
                "manifest line {line_number}: array missing ']'"
            )));
        };
        let inner = inner.trim();
        if inner.is_empty() {
            return Ok(TomlValue::Array(Vec::new()));
        }
        let mut values = Vec::new();
        for entry in split_array_entries(inner, line_number)? {
            let trimmed = entry.trim();
            if trimmed.is_empty() {
                return Err(manifest_error(format!(
                    "manifest line {line_number}: empty array entry"
                )));
            }
            let value = parse_value(trimmed, line_number)?;
            values.push(value);
        }
        return Ok(TomlValue::Array(values));
    }

    if let Some(rest) = text.strip_prefix('"') {
        let Some(string) = rest.strip_suffix('"') else {
            return Err(manifest_error(format!(
                "manifest line {line_number}: string missing closing '\"'"
            )));
        };
        if string.contains('"') {
            return Err(manifest_error(format!(
                "manifest line {line_number}: embedded '\"' in string is not supported"
            )));
        }
        return Ok(TomlValue::String(string.to_string()));
    }

    Err(manifest_error(format!(
        "manifest line {line_number}: only string or string-array values are supported"
    )))
}

fn split_array_entries(text: &str, line_number: usize) -> Result<Vec<String>, Diagnostic> {
    let mut entries = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    for character in text.chars() {
        match character {
            '"' => {
                in_string = !in_string;
                current.push(character);
            }
            ',' if !in_string => {
                entries.push(std::mem::take(&mut current));
            }
            _ => current.push(character),
        }
    }
    if in_string {
        return Err(manifest_error(format!(
            "manifest line {line_number}: unterminated string in array"
        )));
    }
    if !current.trim().is_empty() {
        entries.push(current);
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from("/tmp/nox-test")
    }

    #[test]
    fn parses_minimal_manifest() {
        let source = r#"
[package]
name = "demo"
version = "0.0.1"
"#;
        let manifest = Manifest::parse(source, root()).unwrap();
        assert_eq!(manifest.package.name, "demo");
        assert_eq!(manifest.package.version, "0.0.1");
        assert!(manifest.entrypoints.main.is_none());
        assert!(manifest.modules.source_dirs.is_empty());
    }

    #[test]
    fn parses_entrypoints_and_source_dirs() {
        let source = r#"
[package]
name = "demo"
version = "0.0.1"
description = "demo package"

[entrypoints]
main = "src/main.nox"
admin = "src/admin.nox"

[modules]
source_dirs = ["src", "lib"]
test_dirs = ["tests"]

[runtime]
permissions = ["filesystem.read", "environment", "async_tasks"]
"#;
        let manifest = Manifest::parse(source, root()).unwrap();
        assert_eq!(
            manifest.package.description.as_deref(),
            Some("demo package")
        );
        assert_eq!(
            manifest.entrypoints.main.as_deref(),
            Some(Path::new("src/main.nox"))
        );
        assert_eq!(
            manifest.entrypoints.named,
            vec![("admin".to_string(), PathBuf::from("src/admin.nox"))]
        );
        assert_eq!(
            manifest.modules.source_dirs,
            vec![PathBuf::from("src"), PathBuf::from("lib")]
        );
        assert_eq!(manifest.modules.test_dirs, vec![PathBuf::from("tests")]);
        assert_eq!(manifest.main_path(), Some(root().join("src/main.nox")));
        assert_eq!(
            manifest.source_dirs(),
            vec![root().join("src"), root().join("lib")]
        );
        assert_eq!(manifest.test_dirs(), vec![root().join("tests")]);
        assert_eq!(
            manifest.runtime.permissions,
            vec![
                RuntimePermissionDecl::FilesystemRead,
                RuntimePermissionDecl::Environment,
                RuntimePermissionDecl::AsyncTasks
            ]
        );
    }

    #[test]
    fn skips_blank_and_comment_lines() {
        let source = r#"
# leading comment

[package]
name = "demo" # trailing comment
version = "0.0.1"
"#;
        let manifest = Manifest::parse(source, root()).unwrap();
        assert_eq!(manifest.package.name, "demo");
    }

    #[test]
    fn rejects_missing_package_section() {
        let source = r#"
[entrypoints]
main = "src/main.nox"
"#;
        let err = Manifest::parse(source, root()).unwrap_err();
        assert!(err.message.contains("[package]"));
    }

    #[test]
    fn rejects_missing_required_keys() {
        let source = r#"
[package]
name = "demo"
"#;
        let err = Manifest::parse(source, root()).unwrap_err();
        assert_eq!(err.code, "manifest.invalid");
        assert!(err.message.contains("version"));
    }

    #[test]
    fn rejects_duplicate_section() {
        let source = r#"
[package]
name = "demo"
version = "0.0.1"

[package]
name = "second"
version = "0.0.2"
"#;
        let err = Manifest::parse(source, root()).unwrap_err();
        assert!(err.message.contains("more than once"));
    }

    #[test]
    fn rejects_value_outside_section() {
        let source = "name = \"demo\"\n";
        let err = Manifest::parse(source, root()).unwrap_err();
        assert!(err.message.contains("outside any [section]"));
    }

    #[test]
    fn rejects_non_string_main() {
        let source = r#"
[package]
name = "demo"
version = "0.0.1"

[entrypoints]
main = ["src/main.nox"]
"#;
        let err = Manifest::parse(source, root()).unwrap_err();
        assert!(err.message.contains("entrypoints.main"));
    }

    #[test]
    fn rejects_non_string_array_source_dirs() {
        let source = r#"
[package]
name = "demo"
version = "0.0.1"

[modules]
source_dirs = "src"
"#;
        let err = Manifest::parse(source, root()).unwrap_err();
        assert!(err.message.contains("modules.source_dirs"));
    }

    #[test]
    fn rejects_non_string_array_test_dirs() {
        let source = r#"
[package]
name = "demo"
version = "0.0.1"

[modules]
test_dirs = "tests"
"#;
        let err = Manifest::parse(source, root()).unwrap_err();
        assert!(err.message.contains("modules.test_dirs"));
    }

    #[test]
    fn rejects_module_dirs_outside_project_root() {
        for (source, message) in [
            (
                r#"
[package]
name = "demo"
version = "0.0.1"

[modules]
source_dirs = ["/tmp/nox-src"]
"#,
                "must stay within the project root",
            ),
            (
                r#"
[package]
name = "demo"
version = "0.0.1"

[modules]
test_dirs = ["../tests"]
"#,
                "must stay within the project root",
            ),
        ] {
            let err = Manifest::parse(source, root()).unwrap_err();
            assert_eq!(err.code, "manifest.invalid");
            assert!(
                err.message.contains(message),
                "expected {message:?}, got {:?}",
                err.message
            );
        }
    }

    #[test]
    fn rejects_duplicate_module_dirs() {
        let source = r#"
[package]
name = "demo"
version = "0.0.1"

[modules]
source_dirs = ["src", "./src"]
"#;
        let err = Manifest::parse(source, root()).unwrap_err();
        assert_eq!(err.code, "manifest.invalid");
        assert!(err.message.contains("duplicate path 'src'"));
    }

    #[test]
    fn rejects_unknown_runtime_permission() {
        let source = r#"
[package]
name = "demo"
version = "0.0.1"

[runtime]
permissions = ["shell"]
"#;
        let err = Manifest::parse(source, root()).unwrap_err();
        assert_eq!(err.code, "manifest.invalid");
        assert!(err.message.contains("unknown permission 'shell'"));
    }

    #[test]
    fn discover_finds_manifest_in_parent() {
        let dir = std::env::temp_dir().join(format!(
            "nox-manifest-discover-{}-{}",
            std::process::id(),
            line!()
        ));
        let nested = dir.join("src").join("nested");
        fs::create_dir_all(&nested).unwrap();
        let manifest_path = dir.join(MANIFEST_FILE_NAME);
        fs::write(
            &manifest_path,
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n",
        )
        .unwrap();

        let entry = nested.join("main.nox");
        fs::write(&entry, "0;").unwrap();
        let manifest = Manifest::discover(&entry).unwrap().unwrap();
        assert_eq!(manifest.root, dir);
        assert_eq!(manifest.package.name, "demo");
    }

    #[test]
    fn discover_returns_none_when_no_manifest_found() {
        let dir = std::env::temp_dir().join(format!(
            "nox-manifest-none-{}-{}",
            std::process::id(),
            line!()
        ));
        fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("solo.nox");
        fs::write(&entry, "0;").unwrap();
        // No manifest written; discover walks up to /tmp etc and finds none.
        let result = Manifest::discover(&entry).unwrap();
        assert!(
            result.is_none(),
            "unexpected manifest discovered at {:?}",
            result.map(|manifest| manifest.root)
        );
    }
}
