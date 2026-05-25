use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

use nox_core::{Diagnostic, Span};

pub const MANIFEST_FILE_NAME: &str = "nox.toml";
pub const LOCK_FILE_NAME: &str = "nox.lock";

#[derive(Debug, Clone)]
pub struct Manifest {
    pub root: PathBuf,
    pub package: PackageInfo,
    pub entrypoints: Entrypoints,
    pub modules: Modules,
    pub dependencies: Vec<DependencyDecl>,
    pub codegen: Vec<CodegenArtifact>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyDecl {
    pub name: String,
    pub source: DependencySource,
    pub pin: DependencyPin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodegenArtifact {
    pub name: String,
    pub generated: PathBuf,
    pub generator: Option<String>,
    pub template: Option<PathBuf>,
    pub input_hash: Option<String>,
    pub source_map: Option<PathBuf>,
    pub source_map_hash: Option<String>,
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencySource {
    GitHub(String),
    Git(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyPin {
    Rev(String),
    Tag(String),
}

impl DependencySource {
    pub fn kind(&self) -> &'static str {
        match self {
            DependencySource::GitHub(_) => "github",
            DependencySource::Git(_) => "git",
        }
    }

    pub fn value(&self) -> &str {
        match self {
            DependencySource::GitHub(value) | DependencySource::Git(value) => value,
        }
    }
}

impl DependencyPin {
    pub fn kind(&self) -> &'static str {
        match self {
            DependencyPin::Rev(_) => "rev",
            DependencyPin::Tag(_) => "tag",
        }
    }

    pub fn value(&self) -> &str {
        match self {
            DependencyPin::Rev(value) | DependencyPin::Tag(value) => value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lockfile {
    pub dependencies: Vec<LockedDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedDependency {
    pub name: String,
    pub source: DependencySource,
    pub pin: DependencyPin,
    pub resolved: String,
    pub content_hash: String,
    pub cache_key: String,
    pub tool: String,
}

#[derive(Debug, Clone)]
pub struct LockfileValidation {
    pub path: PathBuf,
    pub ok: bool,
    pub status: &'static str,
    pub diagnostics: Vec<Diagnostic>,
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
    ProcessRun,
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
        validate_manifest_schema(&document)?;

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

        let dependencies = match document.section("dependencies") {
            Some(section) => parse_dependencies(section)?,
            None => Vec::new(),
        };

        let codegen = match document.section("codegen") {
            Some(section) => parse_codegen(section)?,
            None => Vec::new(),
        };

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
            dependencies,
            codegen,
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

impl Lockfile {
    pub fn load(path: &Path) -> Result<Lockfile, Diagnostic> {
        let source = fs::read_to_string(path).map_err(|err| {
            lockfile_error(format!(
                "failed to read lockfile '{}': {err}",
                path.display()
            ))
        })?;
        Lockfile::parse(&source)
    }

    pub fn parse(source: &str) -> Result<Lockfile, Diagnostic> {
        let document = parse_toml(source)
            .map_err(|err| lockfile_error(format!("failed to parse lockfile: {}", err.message)))?;
        let lock = document
            .section("lock")
            .ok_or_else(|| lockfile_error("lockfile is missing required section [lock]"))?;
        let version = lock
            .require_string("version")
            .map_err(|err| lockfile_error(err.message))?
            .ok_or_else(|| lockfile_error("lockfile [lock] is missing required key 'version'"))?;
        if version != "1" {
            return Err(lockfile_error(format!(
                "lockfile version '{version}' is not supported"
            )));
        }

        let mut dependencies = Vec::new();
        for section in &document.sections {
            let Some(name) = section.name.strip_prefix("dependencies.") else {
                if section.name != "lock" {
                    return Err(lockfile_error(format!(
                        "lockfile section [{}] is not supported",
                        section.name
                    )));
                }
                continue;
            };
            dependencies.push(parse_locked_dependency(name, section)?);
        }
        Ok(Lockfile { dependencies })
    }

    pub fn to_source(&self) -> String {
        let mut source = String::from("[lock]\nversion = \"1\"\n");
        for dependency in &self.dependencies {
            source.push_str("\n[dependencies.");
            source.push_str(&dependency.name);
            source.push_str("]\n");
            source.push_str(&format!(
                "source_kind = \"{}\"\nsource = \"{}\"\npin_kind = \"{}\"\npin = \"{}\"\nresolved = \"{}\"\ncontent_hash = \"{}\"\ncache_key = \"{}\"\ntool = \"{}\"\n",
                dependency.source.kind(),
                dependency.source.value(),
                dependency.pin.kind(),
                dependency.pin.value(),
                dependency.resolved,
                dependency.content_hash,
                dependency.cache_key,
                dependency.tool
            ));
        }
        source
    }
}

pub fn validate_lockfile_for_manifest(manifest: &Manifest) -> LockfileValidation {
    let path = manifest.root.join(LOCK_FILE_NAME);
    if manifest.dependencies.is_empty() {
        return LockfileValidation {
            path,
            ok: true,
            status: "not_required",
            diagnostics: Vec::new(),
        };
    }
    if !path.is_file() {
        return LockfileValidation {
            path,
            ok: false,
            status: "missing",
            diagnostics: vec![lockfile_error(format!(
                "{LOCK_FILE_NAME} is required when [dependencies] is present"
            ))],
        };
    }
    match Lockfile::load(&path) {
        Ok(lockfile) => {
            let diagnostics = compare_lockfile(manifest, &lockfile);
            LockfileValidation {
                path,
                ok: diagnostics.is_empty(),
                status: if diagnostics.is_empty() {
                    "ok"
                } else {
                    "drift"
                },
                diagnostics,
            }
        }
        Err(err) => LockfileValidation {
            path,
            ok: false,
            status: "invalid",
            diagnostics: vec![err],
        },
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
        "process_run" => Ok(RuntimePermissionDecl::ProcessRun),
        _ => Err(manifest_error(format!(
            "manifest key 'runtime.permissions' contains unknown permission '{value}'"
        ))),
    }
}

fn manifest_error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(message, Span { start: 0, end: 0 }).with_code("manifest.invalid")
}

fn lockfile_error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(message, Span { start: 0, end: 0 }).with_code("lockfile.invalid")
}

fn validate_manifest_schema(document: &TomlDocument) -> Result<(), Diagnostic> {
    for section in &document.sections {
        match section.name.as_str() {
            "package" => validate_known_keys(
                section,
                &["name", "version", "description"],
                "manifest section [package]",
            )?,
            "entrypoints" => {}
            "modules" => validate_known_keys(
                section,
                &["source_dirs", "test_dirs"],
                "manifest section [modules]",
            )?,
            "dependencies" => {}
            "codegen" => {}
            "runtime" => {
                validate_known_keys(section, &["permissions"], "manifest section [runtime]")?
            }
            name => {
                return Err(manifest_error(format!(
                    "manifest section [{name}] is not supported"
                )));
            }
        }
    }
    Ok(())
}

fn validate_known_keys(
    section: &TomlSection,
    allowed: &[&str],
    label: &str,
) -> Result<(), Diagnostic> {
    for (key, _) in &section.entries {
        if !allowed.iter().any(|allowed_key| allowed_key == key) {
            return Err(manifest_error(format!(
                "{label} contains unsupported key '{key}'"
            )));
        }
    }
    Ok(())
}

fn parse_locked_dependency(
    name: &str,
    section: &TomlSection,
) -> Result<LockedDependency, Diagnostic> {
    validate_known_keys(
        section,
        &[
            "source_kind",
            "source",
            "pin_kind",
            "pin",
            "resolved",
            "content_hash",
            "cache_key",
            "tool",
        ],
        &format!("lockfile section [dependencies.{name}]"),
    )
    .map_err(|err| lockfile_error(err.message))?;

    let source_kind = required_lock_string(section, name, "source_kind")?;
    let source_value = required_lock_string(section, name, "source")?;
    let source = match source_kind.as_str() {
        "github" => {
            validate_github_source(name, &source_value)
                .map_err(|err| lockfile_error(err.message))?;
            DependencySource::GitHub(source_value)
        }
        "git" => {
            validate_git_source(name, &source_value).map_err(|err| lockfile_error(err.message))?;
            DependencySource::Git(source_value)
        }
        _ => {
            return Err(lockfile_error(format!(
                "lockfile dependency '{name}' has unsupported source_kind '{source_kind}'"
            )));
        }
    };

    let pin_kind = required_lock_string(section, name, "pin_kind")?;
    let pin_value = required_lock_string(section, name, "pin")?;
    let pin = match pin_kind.as_str() {
        "rev" => {
            validate_rev_pin(name, &pin_value).map_err(|err| lockfile_error(err.message))?;
            DependencyPin::Rev(pin_value)
        }
        "tag" => {
            validate_tag_pin(name, &pin_value).map_err(|err| lockfile_error(err.message))?;
            DependencyPin::Tag(pin_value)
        }
        _ => {
            return Err(lockfile_error(format!(
                "lockfile dependency '{name}' has unsupported pin_kind '{pin_kind}'"
            )));
        }
    };

    let resolved = required_lock_string(section, name, "resolved")?;
    validate_rev_pin(name, &resolved).map_err(|err| {
        lockfile_error(format!(
            "lockfile dependency '{name}' resolved commit is invalid: {}",
            err.message
        ))
    })?;
    let content_hash = required_lock_string(section, name, "content_hash")?;
    validate_content_hash(name, &content_hash)?;
    let cache_key = required_lock_string(section, name, "cache_key")?;
    if cache_key.is_empty() {
        return Err(lockfile_error(format!(
            "lockfile dependency '{name}' cache_key must be non-empty"
        )));
    }
    let tool = required_lock_string(section, name, "tool")?;
    if tool.is_empty() {
        return Err(lockfile_error(format!(
            "lockfile dependency '{name}' tool must be non-empty"
        )));
    }

    Ok(LockedDependency {
        name: name.to_string(),
        source,
        pin,
        resolved,
        content_hash,
        cache_key,
        tool,
    })
}

fn required_lock_string(
    section: &TomlSection,
    name: &str,
    key: &str,
) -> Result<String, Diagnostic> {
    section
        .require_string(key)
        .map_err(|err| lockfile_error(err.message))?
        .ok_or_else(|| {
            lockfile_error(format!(
                "lockfile dependency '{name}' is missing required key '{key}'"
            ))
        })
}

fn validate_content_hash(name: &str, value: &str) -> Result<(), Diagnostic> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(lockfile_error(format!(
            "lockfile dependency '{name}' content_hash must use 'sha256:<hex>'"
        )));
    };
    if hex.len() != 64 || !hex.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(lockfile_error(format!(
            "lockfile dependency '{name}' content_hash must contain a 64-character SHA-256 hex digest"
        )));
    }
    Ok(())
}

fn compare_lockfile(manifest: &Manifest, lockfile: &Lockfile) -> Vec<Diagnostic> {
    let manifest_names = manifest
        .dependencies
        .iter()
        .map(|dependency| dependency.name.as_str())
        .collect::<BTreeSet<_>>();
    let lock_names = lockfile
        .dependencies
        .iter()
        .map(|dependency| dependency.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut diagnostics = Vec::new();

    for dependency in &manifest.dependencies {
        let Some(locked) = lockfile
            .dependencies
            .iter()
            .find(|locked| locked.name == dependency.name)
        else {
            diagnostics.push(lockfile_error(format!(
                "lockfile is missing dependency '{}'",
                dependency.name
            )));
            continue;
        };
        if locked.source != dependency.source {
            diagnostics.push(lockfile_error(format!(
                "lockfile dependency '{}' source does not match manifest",
                dependency.name
            )));
        }
        if locked.pin != dependency.pin {
            diagnostics.push(lockfile_error(format!(
                "lockfile dependency '{}' pin does not match manifest",
                dependency.name
            )));
        }
    }

    for extra in lock_names.difference(&manifest_names) {
        diagnostics.push(lockfile_error(format!(
            "lockfile contains dependency '{extra}' that is not declared in manifest"
        )));
    }

    diagnostics
}

fn parse_codegen(section: &TomlSection) -> Result<Vec<CodegenArtifact>, Diagnostic> {
    let mut artifacts = Vec::new();
    for (name, value) in &section.entries {
        let TomlValue::InlineTable(table) = value else {
            return Err(manifest_error(format!(
                "manifest key 'codegen.{name}' must be an inline table"
            )));
        };
        artifacts.push(parse_codegen_artifact(name, table)?);
    }
    Ok(artifacts)
}

fn parse_codegen_artifact(
    name: &str,
    table: &BTreeMap<String, TomlValue>,
) -> Result<CodegenArtifact, Diagnostic> {
    validate_codegen_keys(name, table)?;
    let generated = codegen_required_path(name, table, "generated")?;
    let generator = codegen_optional_string(name, table, "generator")?;
    let template = codegen_optional_path(name, table, "template")?;
    let input_hash = codegen_optional_string(name, table, "input_hash")?;
    if let Some(hash) = &input_hash {
        validate_codegen_sha256_hash(name, "input_hash", hash)?;
    }
    let source_map = codegen_optional_path(name, table, "source_map")?;
    let source_map_hash = codegen_optional_string(name, table, "source_map_hash")?;
    if let Some(hash) = &source_map_hash {
        validate_codegen_sha256_hash(name, "source_map_hash", hash)?;
        if source_map.is_none() {
            return Err(manifest_error(format!(
                "manifest codegen artifact '{name}' source_map_hash requires source_map"
            )));
        }
    }
    let command = codegen_optional_string(name, table, "command")?;
    Ok(CodegenArtifact {
        name: name.to_string(),
        generated,
        generator,
        template,
        input_hash,
        source_map,
        source_map_hash,
        command,
    })
}

fn validate_codegen_keys(
    name: &str,
    table: &BTreeMap<String, TomlValue>,
) -> Result<(), Diagnostic> {
    for key in table.keys() {
        if ![
            "generated",
            "generator",
            "template",
            "input_hash",
            "source_map",
            "source_map_hash",
            "command",
        ]
        .contains(&key.as_str())
        {
            return Err(manifest_error(format!(
                "manifest codegen artifact '{name}' contains unsupported key '{key}'"
            )));
        }
    }
    Ok(())
}

fn codegen_required_path(
    name: &str,
    table: &BTreeMap<String, TomlValue>,
    key: &str,
) -> Result<PathBuf, Diagnostic> {
    let Some(value) = codegen_optional_string(name, table, key)? else {
        return Err(manifest_error(format!(
            "manifest codegen artifact '{name}' is missing required key '{key}'"
        )));
    };
    validate_relative_path(&format!("codegen.{name}.{key}"), value)
}

fn codegen_optional_path(
    name: &str,
    table: &BTreeMap<String, TomlValue>,
    key: &str,
) -> Result<Option<PathBuf>, Diagnostic> {
    match codegen_optional_string(name, table, key)? {
        Some(value) => validate_relative_path(&format!("codegen.{name}.{key}"), value).map(Some),
        None => Ok(None),
    }
}

fn codegen_optional_string(
    name: &str,
    table: &BTreeMap<String, TomlValue>,
    key: &str,
) -> Result<Option<String>, Diagnostic> {
    match table.get(key) {
        None => Ok(None),
        Some(TomlValue::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(manifest_error(format!(
            "manifest codegen artifact '{name}' key '{key}' must be a string"
        ))),
    }
}

fn validate_codegen_sha256_hash(name: &str, key: &str, value: &str) -> Result<(), Diagnostic> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(manifest_error(format!(
            "manifest codegen artifact '{name}' {key} must use 'sha256:<hex>'"
        )));
    };
    if hex.len() != 64 || !hex.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(manifest_error(format!(
            "manifest codegen artifact '{name}' {key} must contain a 64-character SHA-256 hex digest"
        )));
    }
    Ok(())
}

fn parse_dependencies(section: &TomlSection) -> Result<Vec<DependencyDecl>, Diagnostic> {
    let mut dependencies = Vec::new();
    for (name, value) in &section.entries {
        let TomlValue::InlineTable(table) = value else {
            return Err(manifest_error(format!(
                "manifest key 'dependencies.{name}' must be an inline table"
            )));
        };
        dependencies.push(parse_dependency(name, table)?);
    }
    Ok(dependencies)
}

fn parse_dependency(
    name: &str,
    table: &BTreeMap<String, TomlValue>,
) -> Result<DependencyDecl, Diagnostic> {
    validate_dependency_keys(name, table)?;
    let github = dependency_string(name, table, "github")?;
    let git = dependency_string(name, table, "git")?;
    let source = match (github, git) {
        (Some(github), None) => {
            validate_github_source(name, &github)?;
            DependencySource::GitHub(github)
        }
        (None, Some(git)) => {
            validate_git_source(name, &git)?;
            DependencySource::Git(git)
        }
        (Some(_), Some(_)) => {
            return Err(manifest_error(format!(
                "manifest dependency '{name}' must specify only one source: github or git"
            )));
        }
        (None, None) => {
            return Err(manifest_error(format!(
                "manifest dependency '{name}' is missing source key 'github' or 'git'"
            )));
        }
    };

    let rev = dependency_string(name, table, "rev")?;
    let tag = dependency_string(name, table, "tag")?;
    let pin = match (rev, tag) {
        (Some(rev), None) => {
            validate_rev_pin(name, &rev)?;
            DependencyPin::Rev(rev)
        }
        (None, Some(tag)) => {
            validate_tag_pin(name, &tag)?;
            DependencyPin::Tag(tag)
        }
        (Some(_), Some(_)) => {
            return Err(manifest_error(format!(
                "manifest dependency '{name}' must specify only one pin: rev or tag"
            )));
        }
        (None, None) => {
            return Err(manifest_error(format!(
                "manifest dependency '{name}' is missing required pin 'rev' or 'tag'"
            )));
        }
    };

    Ok(DependencyDecl {
        name: name.to_string(),
        source,
        pin,
    })
}

fn validate_dependency_keys(
    name: &str,
    table: &BTreeMap<String, TomlValue>,
) -> Result<(), Diagnostic> {
    for key in table.keys() {
        if !["github", "git", "rev", "tag"].contains(&key.as_str()) {
            return Err(manifest_error(format!(
                "manifest dependency '{name}' contains unsupported key '{key}'"
            )));
        }
    }
    Ok(())
}

fn dependency_string(
    name: &str,
    table: &BTreeMap<String, TomlValue>,
    key: &str,
) -> Result<Option<String>, Diagnostic> {
    match table.get(key) {
        None => Ok(None),
        Some(TomlValue::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(manifest_error(format!(
            "manifest dependency '{name}' key '{key}' must be a string"
        ))),
    }
}

fn validate_github_source(name: &str, value: &str) -> Result<(), Diagnostic> {
    let parts = value.split('/').collect::<Vec<_>>();
    if parts.len() != 2 || parts.iter().any(|part| part.is_empty()) || value.contains("://") {
        return Err(manifest_error(format!(
            "manifest dependency '{name}' github source must use 'owner/repo'"
        )));
    }
    Ok(())
}

fn validate_git_source(name: &str, value: &str) -> Result<(), Diagnostic> {
    if !(value.starts_with("https://")
        || value.starts_with("ssh://")
        || value.starts_with("file://"))
    {
        return Err(manifest_error(format!(
            "manifest dependency '{name}' git source must be an https://, ssh://, or file:// URL"
        )));
    }
    Ok(())
}

fn validate_rev_pin(name: &str, value: &str) -> Result<(), Diagnostic> {
    if value.len() != 40 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(manifest_error(format!(
            "manifest dependency '{name}' rev pin must be a full 40-character commit hash"
        )));
    }
    Ok(())
}

fn validate_tag_pin(name: &str, value: &str) -> Result<(), Diagnostic> {
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return Err(manifest_error(format!(
            "manifest dependency '{name}' tag pin must be a non-empty tag without whitespace"
        )));
    }
    Ok(())
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

fn validate_relative_path(key: &str, value: String) -> Result<PathBuf, Diagnostic> {
    let mut paths = validate_relative_paths(key, vec![value])?;
    Ok(paths.remove(0))
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
    InlineTable(BTreeMap<String, TomlValue>),
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
        if !is_toml_key(key) {
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
    if let Some(rest) = text.strip_prefix('{') {
        let Some(inner) = rest.strip_suffix('}') else {
            return Err(manifest_error(format!(
                "manifest line {line_number}: inline table missing '}}'"
            )));
        };
        return parse_inline_table(inner.trim(), line_number);
    }

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

fn parse_inline_table(text: &str, line_number: usize) -> Result<TomlValue, Diagnostic> {
    let mut table = BTreeMap::new();
    if text.is_empty() {
        return Ok(TomlValue::InlineTable(table));
    }
    for entry in split_array_entries(text, line_number)? {
        let Some((key, value)) = entry.split_once('=') else {
            return Err(manifest_error(format!(
                "manifest line {line_number}: expected 'key = value' in inline table"
            )));
        };
        let key = key.trim();
        if key.is_empty() || !is_toml_key(key) {
            return Err(manifest_error(format!(
                "manifest line {line_number}: inline table key '{key}' contains unsupported characters"
            )));
        }
        if table.contains_key(key) {
            return Err(manifest_error(format!(
                "manifest line {line_number}: duplicate inline table key '{key}'"
            )));
        }
        table.insert(key.to_string(), parse_value(value.trim(), line_number)?);
    }
    Ok(TomlValue::InlineTable(table))
}

fn is_toml_key(key: &str) -> bool {
    key.chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-')
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
        assert!(manifest.dependencies.is_empty());
        assert!(manifest.codegen.is_empty());
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
    fn parses_pinned_git_dependencies() {
        let source = r#"
[package]
name = "demo"
version = "0.0.1"

[dependencies]
mathx = { github = "owner/mathx", rev = "0123456789abcdef0123456789abcdef01234567" }
tools = { git = "https://github.com/owner/tools.git", tag = "v0.2.0" }
"#;
        let manifest = Manifest::parse(source, root()).unwrap();
        assert_eq!(
            manifest.dependencies,
            vec![
                DependencyDecl {
                    name: "mathx".to_string(),
                    source: DependencySource::GitHub("owner/mathx".to_string()),
                    pin: DependencyPin::Rev("0123456789abcdef0123456789abcdef01234567".to_string()),
                },
                DependencyDecl {
                    name: "tools".to_string(),
                    source: DependencySource::Git("https://github.com/owner/tools.git".to_string()),
                    pin: DependencyPin::Tag("v0.2.0".to_string()),
                }
            ]
        );
    }

    #[test]
    fn rejects_unpinned_or_ambiguous_dependencies() {
        for (source, message) in [
            (
                r#"
[package]
name = "demo"
version = "0.0.1"

[dependencies]
mathx = { github = "owner/mathx" }
"#,
                "missing required pin",
            ),
            (
                r#"
[package]
name = "demo"
version = "0.0.1"

[dependencies]
mathx = { github = "owner/mathx", git = "https://github.com/owner/mathx.git", rev = "0123456789abcdef0123456789abcdef01234567" }
"#,
                "only one source",
            ),
            (
                r#"
[package]
name = "demo"
version = "0.0.1"

[dependencies]
mathx = { github = "owner/mathx", rev = "main" }
"#,
                "full 40-character commit hash",
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
    fn parses_codegen_artifacts() {
        let source = r#"
[package]
name = "demo"
version = "0.0.1"

[codegen]
api = { generated = "src/generated/api.nox", generator = "tools/gen-api", template = "schemas/api.tpl", input_hash = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", source_map = "src/generated/api.nox.map", source_map_hash = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", command = "tools/gen-api schemas/api.tpl > src/generated/api.nox" }
"#;
        let manifest = Manifest::parse(source, root()).unwrap();
        assert_eq!(
            manifest.codegen,
            vec![CodegenArtifact {
                name: "api".to_string(),
                generated: PathBuf::from("src/generated/api.nox"),
                generator: Some("tools/gen-api".to_string()),
                template: Some(PathBuf::from("schemas/api.tpl")),
                input_hash: Some(
                    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        .to_string()
                ),
                source_map: Some(PathBuf::from("src/generated/api.nox.map")),
                source_map_hash: Some(
                    "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                        .to_string()
                ),
                command: Some("tools/gen-api schemas/api.tpl > src/generated/api.nox".to_string()),
            }]
        );
    }

    #[test]
    fn rejects_invalid_codegen_artifacts() {
        for (source, message) in [
            (
                r#"
[package]
name = "demo"
version = "0.0.1"

[codegen]
api = { generator = "tools/gen-api" }
"#,
                "missing required key 'generated'",
            ),
            (
                r#"
[package]
name = "demo"
version = "0.0.1"

[codegen]
api = { generated = "../generated.nox" }
"#,
                "must stay within the project root",
            ),
            (
                r#"
[package]
name = "demo"
version = "0.0.1"

[codegen]
api = { generated = "src/generated.nox", input_hash = "sha256:short" }
"#,
                "64-character SHA-256",
            ),
            (
                r#"
[package]
name = "demo"
version = "0.0.1"

[codegen]
api = { generated = "src/generated.nox", source_map = "../generated.map" }
"#,
                "must stay within the project root",
            ),
            (
                r#"
[package]
name = "demo"
version = "0.0.1"

[codegen]
api = { generated = "src/generated.nox", source_map_hash = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" }
"#,
                "source_map_hash requires source_map",
            ),
            (
                r#"
[package]
name = "demo"
version = "0.0.1"

[codegen]
api = { generated = "src/generated.nox", mode = "auto" }
"#,
                "unsupported key 'mode'",
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
    fn parses_and_renders_lockfile() {
        let source = r#"
[lock]
version = "1"

[dependencies.mathx]
source_kind = "github"
source = "owner/mathx"
pin_kind = "rev"
pin = "0123456789abcdef0123456789abcdef01234567"
resolved = "0123456789abcdef0123456789abcdef01234567"
content_hash = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
cache_key = "github-owner-mathx-0123456789abcdef0123456789abcdef01234567"
tool = "nox 0.0.4"
"#;
        let lockfile = Lockfile::parse(source).unwrap();
        assert_eq!(
            lockfile.dependencies,
            vec![LockedDependency {
                name: "mathx".to_string(),
                source: DependencySource::GitHub("owner/mathx".to_string()),
                pin: DependencyPin::Rev("0123456789abcdef0123456789abcdef01234567".to_string()),
                resolved: "0123456789abcdef0123456789abcdef01234567".to_string(),
                content_hash:
                    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        .to_string(),
                cache_key: "github-owner-mathx-0123456789abcdef0123456789abcdef01234567"
                    .to_string(),
                tool: "nox 0.0.4".to_string(),
            }]
        );
        assert_eq!(Lockfile::parse(&lockfile.to_source()).unwrap(), lockfile);
    }

    #[test]
    fn validates_lockfile_against_manifest_dependencies() {
        let manifest = Manifest::parse(
            r#"
[package]
name = "demo"
version = "0.0.1"

[dependencies]
mathx = { github = "owner/mathx", rev = "0123456789abcdef0123456789abcdef01234567" }
"#,
            root(),
        )
        .unwrap();
        let lockfile = Lockfile::parse(
            r#"
[lock]
version = "1"

[dependencies.mathx]
source_kind = "github"
source = "owner/mathx"
pin_kind = "rev"
pin = "fedcba9876543210fedcba9876543210fedcba98"
resolved = "fedcba9876543210fedcba9876543210fedcba98"
content_hash = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
cache_key = "github-owner-mathx-fedcba9876543210fedcba9876543210fedcba98"
tool = "nox 0.0.4"
"#,
        )
        .unwrap();
        let diagnostics = compare_lockfile(&manifest, &lockfile);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "lockfile.invalid");
        assert!(diagnostics[0].message.contains("pin does not match"));
    }

    #[test]
    fn rejects_invalid_lockfile_content_hash() {
        let err = Lockfile::parse(
            r#"
[lock]
version = "1"

[dependencies.mathx]
source_kind = "github"
source = "owner/mathx"
pin_kind = "rev"
pin = "0123456789abcdef0123456789abcdef01234567"
resolved = "0123456789abcdef0123456789abcdef01234567"
content_hash = "sha256:short"
cache_key = "cache"
tool = "nox 0.0.4"
"#,
        )
        .unwrap_err();
        assert_eq!(err.code, "lockfile.invalid");
        assert!(err.message.contains("64-character SHA-256"));
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
    fn rejects_unknown_manifest_section() {
        let source = r#"
[package]
name = "demo"
version = "0.0.1"

[schema]
version = "1"
"#;
        let err = Manifest::parse(source, root()).unwrap_err();
        assert_eq!(err.code, "manifest.invalid");
        assert!(err.message.contains("section [schema] is not supported"));
    }

    #[test]
    fn rejects_unknown_manifest_key() {
        let source = r#"
[package]
name = "demo"
version = "0.0.1"
license = "MIT"
"#;
        let err = Manifest::parse(source, root()).unwrap_err();
        assert_eq!(err.code, "manifest.invalid");
        assert!(err
            .message
            .contains("manifest section [package] contains unsupported key 'license'"));
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
