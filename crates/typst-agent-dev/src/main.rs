//! Deterministic, model-free repository evidence commands.
//!
//! This crate deliberately has no dependency on the Typst compiler. It may
//! inspect source, Cargo metadata, and Git history, but it cannot mutate refs,
//! publish artifacts, or contact an AI service.

use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

const CONTRACT_VERSION: &str = "agent-contract/v1";
const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_SEMANTIC_EVIDENCE_BYTES: u64 = 1024 * 1024;
const MAX_SEMANTIC_COMMAND_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Parser)]
#[command(
    name = "typst-agent",
    bin_name = "cargo agent",
    about = "Model-free Typst Agent development control plane"
)]
struct Cli {
    #[arg(long, value_enum, global = true, default_value_t = OutputFormat::Human)]
    format: OutputFormat,
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
    Human,
    Json,
}

#[derive(Debug, Subcommand)]
enum CommandKind {
    /// Validate repository authorities, remotes, and contract files.
    Doctor,
    /// Emit scoped guidance and invariant records for changed paths.
    Context(ContextArgs),
    /// Report changed packages and reverse Cargo dependencies.
    Impact(ImpactArgs),
    /// Run deterministic verification lanes.
    Verify(VerifyArgs),
    /// Build bounded review evidence for a diff.
    ReviewPack(ReviewPackArgs),
    /// Check downstream policy and the upstream write boundary.
    PolicyCheck,
    /// Verify the upstream mirror and mirrored tags.
    UpstreamCheck,
    /// Run disposable deterministic control-plane checks.
    Eval,
    /// Emit the source and release identity manifest.
    ReleaseManifest,
}

#[derive(Debug, Args)]
struct ContextArgs {
    /// Paths to scope. When omitted, the current diff is used.
    #[arg(long = "paths", value_delimiter = ',', num_args = 1..)]
    paths: Vec<PathBuf>,
}

#[derive(Debug, Args)]
struct ImpactArgs {
    #[arg(long, default_value = "main")]
    base: String,
}

#[derive(Debug, Args)]
struct ReviewPackArgs {
    #[arg(long, default_value = "main")]
    base: String,
}

#[derive(Debug, Args)]
struct VerifyArgs {
    #[arg(long, value_enum, default_value_t = VerifyTier::Fast)]
    tier: VerifyTier,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum VerifyTier {
    Fast,
    Pr,
    Full,
}

#[derive(Debug)]
struct AppError {
    code: u8,
    message: String,
}

impl AppError {
    fn invalid(message: impl Into<String>) -> Self {
        Self { code: 2, message: message.into() }
    }

    fn policy(message: impl Into<String>) -> Self {
        Self { code: 3, message: message.into() }
    }

    fn verification(message: impl Into<String>) -> Self {
        Self { code: 4, message: message.into() }
    }

    fn authority(message: impl Into<String>) -> Self {
        Self { code: 5, message: message.into() }
    }
}

type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Serialize)]
struct Envelope {
    contract_version: &'static str,
    command: &'static str,
    status: &'static str,
    payload: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InvariantFile {
    version: u32,
    records: Vec<InvariantRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InvariantRecord {
    id: String,
    scope: String,
    statement: String,
    rationale: String,
    authority_source: String,
    required_checks: Vec<String>,
    review_prompts: Vec<String>,
    upstream_anchor: String,
    upstream_sha: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AreaManifestEnvelope {
    contract_version: String,
    kind: String,
    payload: AreaManifest,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AreaManifest {
    manifest_version: u32,
    rules: Vec<AreaRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AreaRule {
    id: String,
    path_prefixes: Vec<String>,
    exact_paths: Vec<String>,
    authority_sources: Vec<String>,
    guide: String,
    required_checks: Vec<String>,
    invariant_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    contract_version: &'static str,
    invariant_count: usize,
    area_count: usize,
    repository: String,
    required_files: Vec<String>,
    upstream_push_url: String,
}

#[derive(Debug, Serialize)]
struct ContextReport {
    paths: Vec<String>,
    area_ids: Vec<String>,
    authority_sources: Vec<String>,
    guides: Vec<String>,
    required_checks: Vec<String>,
    invariants: Vec<InvariantRecord>,
}

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<Package>,
}

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
    manifest_path: String,
    #[serde(default)]
    dependencies: Vec<PackageDependency>,
}

#[derive(Debug, Deserialize)]
struct PackageDependency {
    path: Option<String>,
}

#[derive(Debug, Serialize)]
struct ImpactReport {
    base: String,
    base_sha: String,
    head_sha: String,
    changed_paths: Vec<String>,
    changed_packages: Vec<String>,
    reverse_dependencies: BTreeMap<String, Vec<String>>,
    scoped_guides: Vec<String>,
    invariant_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PolicyViolation {
    code: String,
    path: String,
    detail: String,
}

#[derive(Debug, Serialize)]
struct PolicyReport {
    checked: &'static str,
    inspected_files: usize,
    upstream_push_url: String,
    violations: Vec<PolicyViolation>,
}

#[derive(Debug, Serialize)]
struct VerificationEvidence {
    tier: String,
    checks: Vec<CheckEvidence>,
    status: String,
}

#[derive(Debug, Serialize)]
struct CheckEvidence {
    name: String,
    status: String,
    exit_code: Option<i32>,
    output: String,
}

fn main() {
    let cli = Cli::parse();
    let result = dispatch(&cli);
    match result {
        Ok((command, payload)) => emit(
            cli.format,
            Envelope {
                contract_version: CONTRACT_VERSION,
                command,
                status: "ok",
                payload,
            },
        ),
        Err(error) => {
            emit(
                cli.format,
                Envelope {
                    contract_version: CONTRACT_VERSION,
                    command: command_name(&cli.command),
                    status: "error",
                    payload: json!({"code": error.code, "message": error.message}),
                },
            );
            std::process::exit(error.code.into());
        }
    }
}

fn dispatch(cli: &Cli) -> AppResult<(&'static str, Value)> {
    match &cli.command {
        CommandKind::Doctor => Ok(("doctor", json_value(doctor()?)?)),
        CommandKind::Context(args) => Ok(("context", json_value(context(args)?)?)),
        CommandKind::Impact(args) => Ok(("impact", json_value(impact(&args.base)?)?)),
        CommandKind::Verify(args) => Ok((
            "verify",
            serde_json::to_value(verify(args.tier)?)
                .map_err(|error| AppError::invalid(error.to_string()))?,
        )),
        CommandKind::ReviewPack(args) => Ok(("review-pack", review_pack(&args.base)?)),
        CommandKind::PolicyCheck => Ok(("policy-check", json_value(policy_check()?)?)),
        CommandKind::UpstreamCheck => Ok(("upstream-check", upstream_check()?)),
        CommandKind::Eval => Ok(("eval", eval()?)),
        CommandKind::ReleaseManifest => Ok(("release-manifest", release_manifest()?)),
    }
}

fn json_value<T: Serialize>(value: T) -> AppResult<Value> {
    serde_json::to_value(value).map_err(|error| AppError::invalid(error.to_string()))
}

fn command_name(command: &CommandKind) -> &'static str {
    match command {
        CommandKind::Doctor => "doctor",
        CommandKind::Context(_) => "context",
        CommandKind::Impact(_) => "impact",
        CommandKind::Verify(_) => "verify",
        CommandKind::ReviewPack(_) => "review-pack",
        CommandKind::PolicyCheck => "policy-check",
        CommandKind::UpstreamCheck => "upstream-check",
        CommandKind::Eval => "eval",
        CommandKind::ReleaseManifest => "release-manifest",
    }
}

fn emit(format: OutputFormat, envelope: Envelope) {
    match format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&envelope).unwrap_or_else(|_| "{}".into())
        ),
        OutputFormat::Human => {
            println!("{}: {}", envelope.command, envelope.status);
            if let Some(object) = envelope.payload.as_object() {
                for (key, value) in object {
                    println!("  {key}: {}", human_value(value));
                }
            } else {
                println!("  {}", human_value(&envelope.payload));
            }
        }
    }
}

fn human_value(value: &Value) -> String {
    match value {
        Value::String(string) => string.clone(),
        _ => serde_json::to_string(value).unwrap_or_else(|_| "<unserializable>".into()),
    }
}

fn root() -> AppResult<PathBuf> {
    let output = run_command("git", ["rev-parse", "--show-toplevel"], None)?;
    Ok(PathBuf::from(output.stdout.trim()))
}

fn run_command<I, S>(
    program: &str,
    args: I,
    cwd: Option<&Path>,
) -> AppResult<CommandOutput>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command.stdin(Stdio::null()).output().map_err(|error| {
        AppError::authority(format!("failed to run {program}: {error}"))
    })?;
    if output.stdout.len() > MAX_SEMANTIC_COMMAND_BYTES
        || output.stderr.len() > MAX_SEMANTIC_COMMAND_BYTES
    {
        return Err(AppError::authority(format!(
            "semantic command output exceeded {MAX_SEMANTIC_COMMAND_BYTES} bytes: {program}"
        )));
    }
    Ok(CommandOutput {
        status: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

#[derive(Debug)]
struct CommandOutput {
    status: Option<i32>,
    stdout: String,
    stderr: String,
}

fn bounded(mut output: String) -> String {
    if output.len() > MAX_OUTPUT_BYTES {
        let mut boundary = MAX_OUTPUT_BYTES;
        while !output.is_char_boundary(boundary) {
            boundary -= 1;
        }
        output.truncate(boundary);
        output.push_str("\n[output truncated]");
    }
    output
}

fn require_success(output: CommandOutput, label: &str) -> AppResult<String> {
    if output.status == Some(0) {
        Ok(output.stdout)
    } else {
        Err(AppError::authority(bounded(format!(
            "{label} failed: {}",
            output.stderr.trim()
        ))))
    }
}

fn read_semantic_text(path: &Path) -> AppResult<String> {
    let metadata = fs::metadata(path).map_err(|error| {
        AppError::authority(format!("cannot inspect {}: {error}", path.display()))
    })?;
    if metadata.len() > MAX_SEMANTIC_EVIDENCE_BYTES {
        return Err(AppError::authority(format!(
            "semantic evidence exceeds {} bytes: {}",
            MAX_SEMANTIC_EVIDENCE_BYTES,
            path.display()
        )));
    }
    fs::read_to_string(path).map_err(|error| {
        AppError::authority(format!("cannot read {}: {error}", path.display()))
    })
}

fn read_invariants(repo: &Path) -> AppResult<InvariantFile> {
    let path = repo.join(".agents/invariants.yml");
    let content = read_semantic_text(&path)?;
    let file: InvariantFile = serde_yaml::from_str(&content).map_err(|error| {
        AppError::invalid(format!("invalid invariant registry: {error}"))
    })?;
    if file.version != 1 || file.records.is_empty() {
        return Err(AppError::invalid(
            "invariant registry must have version 1 and records",
        ));
    }
    Ok(file)
}

fn read_area_manifest(repo: &Path) -> AppResult<AreaManifest> {
    let path = repo.join(".agents/area-manifest.json");
    let content = read_semantic_text(&path)?;
    let envelope: AreaManifestEnvelope = serde_json::from_str(&content)
        .map_err(|error| AppError::invalid(format!("invalid area manifest: {error}")))?;
    if envelope.contract_version != CONTRACT_VERSION || envelope.kind != "AreaManifest" {
        return Err(AppError::invalid(
            "area manifest must be an agent-contract/v1 AreaManifest record",
        ));
    }
    if envelope.payload.manifest_version != 1 || envelope.payload.rules.is_empty() {
        return Err(AppError::invalid(
            "area manifest must have version 1 and at least one rule",
        ));
    }
    Ok(envelope.payload)
}

fn validate_contract_schema(repo: &Path) -> AppResult<()> {
    let path = repo.join("agent-contract/v1/schema.json");
    let schema: Value =
        serde_json::from_str(&read_semantic_text(&path)?).map_err(|error| {
            AppError::invalid(format!("contract schema is not JSON: {error}"))
        })?;
    let schema_id = schema.get("$id").and_then(Value::as_str).unwrap_or_default();
    if !schema_id.ends_with("/agent-contract/v1/schema.json") {
        return Err(AppError::invalid("contract schema has an unexpected $id"));
    }
    let expected = BTreeSet::from([
        "AreaManifest",
        "ImpactReport",
        "InvariantRecord",
        "ReleaseManifest",
        "ReviewEvidence",
        "TaskContract",
        "UpstreamProvenance",
        "VerificationEvidence",
    ]);
    let actual = schema
        .pointer("/properties/kind/enum")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(AppError::invalid(
            "contract schema must define exactly the eight v1 record kinds",
        ));
    }
    let definitions = schema
        .get("$defs")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::invalid("contract schema has no $defs object"))?;
    for kind in expected {
        let definition = definitions
            .get(kind)
            .and_then(Value::as_object)
            .ok_or_else(|| AppError::invalid(format!("contract schema has no {kind}")))?;
        if definition.get("additionalProperties") != Some(&Value::Bool(false)) {
            return Err(AppError::invalid(format!(
                "contract record {kind} must reject unknown fields"
            )));
        }
        if definition
            .get("required")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
        {
            return Err(AppError::invalid(format!(
                "contract record {kind} must require its fields"
            )));
        }
    }
    Ok(())
}

fn doctor() -> AppResult<DoctorReport> {
    let repo = root()?;
    let required = [
        "AGENTS.md",
        "agent-contract/v1/schema.json",
        ".agents/INDEX.md",
        ".agents/area-manifest.json",
        ".agents/invariants.yml",
        ".github/AGENTS.md",
        "crates/AGENTS.md",
        "tests/AGENTS.md",
        "crates/typst-agent-dev/Cargo.toml",
    ];
    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|path| !repo.join(path).is_file())
        .collect();
    if !missing.is_empty() {
        return Err(AppError::authority(format!(
            "missing authority files: {}",
            missing.join(", ")
        )));
    }
    validate_contract_schema(&repo)?;
    let invariants = read_invariants(&repo)?;
    let manifest = read_area_manifest(&repo)?;
    let invariant_ids = invariants
        .records
        .iter()
        .map(|record| record.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut rule_ids = BTreeSet::new();
    for rule in &manifest.rules {
        if !rule_ids.insert(rule.id.as_str()) {
            return Err(AppError::invalid(format!("duplicate area rule: {}", rule.id)));
        }
        normalize_repo_path(&rule.guide)?;
        if !repo.join(&rule.guide).is_file() {
            return Err(AppError::authority(format!(
                "area guide is unavailable: {}",
                rule.guide
            )));
        }
        for path in rule.path_prefixes.iter().chain(&rule.exact_paths) {
            normalize_repo_path(path)?;
        }
        for invariant in &rule.invariant_ids {
            if !invariant_ids.contains(invariant.as_str()) {
                return Err(AppError::invalid(format!(
                    "area {} names unknown invariant {invariant}",
                    rule.id
                )));
            }
        }
    }
    let push_url =
        run_command("git", ["remote", "get-url", "--push", "upstream"], Some(&repo))?;
    let push_url = push_url.stdout.trim().to_string();
    if push_url.is_empty() || push_url.contains("github.com/typst/typst") {
        return Err(AppError::policy(
            "upstream must have an invalid fetch-only push URL",
        ));
    }
    Ok(DoctorReport {
        contract_version: CONTRACT_VERSION,
        invariant_count: invariants.records.len(),
        area_count: manifest.rules.len(),
        repository: repo.display().to_string(),
        required_files: required.into_iter().map(str::to_owned).collect(),
        upstream_push_url: push_url,
    })
}

fn changed_paths(repo: &Path, base: Option<&str>) -> AppResult<Vec<String>> {
    let mut paths = BTreeSet::new();
    if let Some(base) = base {
        let output = run_command(
            "git",
            ["diff", "--name-only", "--diff-filter=ACDMRTUXB", &format!("{base}...HEAD")],
            Some(repo),
        )?;
        collect_paths(&output, &mut paths)?;
    } else {
        let tracked = run_command(
            "git",
            ["diff", "--name-only", "--diff-filter=ACDMRTUXB", "HEAD"],
            Some(repo),
        )?;
        collect_paths(&tracked, &mut paths)?;
        let untracked = run_command(
            "git",
            ["ls-files", "--others", "--exclude-standard"],
            Some(repo),
        )?;
        collect_paths(&untracked, &mut paths)?;
    }
    Ok(paths.into_iter().collect())
}

fn collect_paths(output: &CommandOutput, paths: &mut BTreeSet<String>) -> AppResult<()> {
    if output.status != Some(0) {
        return Err(AppError::authority(format!(
            "cannot enumerate changed paths: {}",
            output.stderr.trim()
        )));
    }
    for line in output.stdout.lines() {
        let path = line.trim();
        if !path.is_empty() {
            paths.insert(normalize_repo_path(path)?);
        }
    }
    Ok(())
}

fn normalize_repo_path(path: &str) -> AppResult<String> {
    if path.is_empty() {
        return Err(AppError::invalid("repository path cannot be empty"));
    }
    let portable = path.replace('\\', "/");
    let candidate = Path::new(&portable);
    let windows_absolute = portable.starts_with("//")
        || portable.as_bytes().get(1) == Some(&b':')
            && portable.as_bytes().first().is_some_and(u8::is_ascii_alphabetic);
    if candidate.is_absolute()
        || windows_absolute
        || candidate.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(AppError::invalid(format!(
            "repository path must be relative and contained: {path}"
        )));
    }
    let normalized = candidate
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy()),
            Component::CurDir => None,
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    if normalized.is_empty() {
        return Err(AppError::invalid("repository path cannot resolve to empty"));
    }
    Ok(normalized)
}

fn context(args: &ContextArgs) -> AppResult<ContextReport> {
    let repo = root()?;
    let paths = if args.paths.is_empty() {
        changed_paths(&repo, None)?
    } else {
        args.paths
            .iter()
            .map(|path| normalize_repo_path(&path.to_string_lossy()))
            .collect::<AppResult<Vec<_>>>()?
    };
    let invariants = read_invariants(&repo)?;
    let manifest = read_area_manifest(&repo)?;
    let invariant_by_id = invariants
        .records
        .into_iter()
        .map(|record| (record.id.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let mut area_ids = BTreeSet::new();
    let mut authorities = BTreeSet::new();
    let mut guides = BTreeSet::new();
    let mut checks = BTreeSet::new();
    let mut record_ids = BTreeSet::new();
    for path in &paths {
        for rule in &manifest.rules {
            if rule_matches_path(rule, path) {
                area_ids.insert(rule.id.clone());
                guides.insert(rule.guide.clone());
                authorities.extend(rule.authority_sources.iter().cloned());
                checks.extend(rule.required_checks.iter().cloned());
                record_ids.extend(rule.invariant_ids.iter().cloned());
            }
        }
    }
    if paths.is_empty() {
        guides.insert(".agents/INDEX.md".into());
        authorities.insert(".agents/area-manifest.json".into());
    }
    let records = record_ids
        .into_iter()
        .filter_map(|id| invariant_by_id.get(&id).cloned())
        .collect();
    Ok(ContextReport {
        paths,
        area_ids: area_ids.into_iter().collect(),
        authority_sources: authorities.into_iter().collect(),
        guides: guides.into_iter().collect(),
        required_checks: checks.into_iter().collect(),
        invariants: records,
    })
}

fn rule_matches_path(rule: &AreaRule, path: &str) -> bool {
    rule.exact_paths.iter().any(|exact| exact == path)
        || rule.path_prefixes.iter().any(|prefix| path.starts_with(prefix))
}

fn impact(base: &str) -> AppResult<ImpactReport> {
    let repo = root()?;
    let changed = changed_paths(&repo, Some(base))?;
    let metadata = cargo_metadata(&repo)?;
    let mut direct_changed_packages = BTreeSet::new();
    let mut package_by_path = BTreeMap::new();
    for package in &metadata.packages {
        let manifest = PathBuf::from(&package.manifest_path);
        let directory = manifest.parent().unwrap_or(Path::new(""));
        let relative = directory
            .strip_prefix(&repo)
            .unwrap_or(directory)
            .to_string_lossy()
            .replace('\\', "/");
        package_by_path.insert(relative.clone(), package.name.clone());
        let manifest_relative = manifest
            .strip_prefix(&repo)
            .unwrap_or(&manifest)
            .to_string_lossy()
            .replace('\\', "/");
        if changed.iter().any(|path| {
            path == &manifest_relative
                || (!relative.is_empty() && path.starts_with(&format!("{relative}/")))
        }) {
            direct_changed_packages.insert(package.name.clone());
        }
    }
    let mut reverse = BTreeMap::<String, BTreeSet<String>>::new();
    for package in &metadata.packages {
        for dependency in &package.dependencies {
            if let Some(path) = &dependency.path {
                let dep = PathBuf::from(path);
                let relative = dep
                    .strip_prefix(&repo)
                    .unwrap_or(&dep)
                    .to_string_lossy()
                    .replace('\\', "/");
                if let Some(name) = package_by_path.get(&relative) {
                    reverse.entry(name.clone()).or_default().insert(package.name.clone());
                }
            }
        }
    }
    let mut affected = direct_changed_packages.clone();
    let mut reverse_dependencies = BTreeMap::new();
    for package in &direct_changed_packages {
        let dependents = transitive_dependents(package, &reverse);
        affected.extend(dependents.iter().cloned());
        reverse_dependencies.insert(package.clone(), dependents.into_iter().collect());
    }
    let scoped =
        context(&ContextArgs { paths: changed.iter().map(PathBuf::from).collect() })?;
    let invariant_ids =
        scoped.invariants.iter().map(|record| record.id.clone()).collect();
    let base_sha = require_success(
        run_command("git", ["merge-base", base, "HEAD"], Some(&repo))?,
        "git merge-base",
    )?
    .trim()
    .to_owned();
    let head_sha = require_success(
        run_command("git", ["rev-parse", "HEAD"], Some(&repo))?,
        "git rev-parse",
    )?
    .trim()
    .to_owned();
    Ok(ImpactReport {
        base: base.to_owned(),
        base_sha,
        head_sha,
        changed_paths: changed,
        changed_packages: affected.into_iter().collect(),
        reverse_dependencies,
        scoped_guides: scoped.guides,
        invariant_ids,
    })
}

fn transitive_dependents(
    package: &str,
    reverse: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut dependents = BTreeSet::new();
    let mut queue = vec![package.to_owned()];
    while let Some(name) = queue.pop() {
        for dependent in reverse.get(&name).into_iter().flatten() {
            if dependents.insert(dependent.clone()) {
                queue.push(dependent.clone());
            }
        }
    }
    dependents.remove(package);
    dependents
}

fn cargo_metadata(repo: &Path) -> AppResult<CargoMetadata> {
    let output = run_command(
        "cargo",
        ["metadata", "--format-version", "1", "--no-deps"],
        Some(repo),
    )?;
    let text = require_success(output, "cargo metadata")?;
    serde_json::from_str(&text)
        .map_err(|error| AppError::authority(format!("invalid cargo metadata: {error}")))
}

fn policy_check() -> AppResult<PolicyReport> {
    let repo = root()?;
    let mut violations = Vec::new();
    let push_url =
        run_command("git", ["remote", "get-url", "--push", "upstream"], Some(&repo))?;
    let push_url = push_url.stdout.trim().to_string();
    if push_url.is_empty() || push_url.contains("github.com/typst/typst") {
        violations.push(PolicyViolation {
            code: "upstream-push-url".into(),
            path: ".git/config".into(),
            detail: "upstream has a writable or missing push URL".into(),
        });
    }
    let remotes =
        require_success(run_command("git", ["remote"], Some(&repo))?, "git remote")?;
    for remote in remotes.lines().map(str::trim).filter(|name| !name.is_empty()) {
        let remote_push = require_success(
            run_command("git", ["remote", "get-url", "--push", remote], Some(&repo))?,
            "remote push URL",
        )?;
        if remote_push.contains("github.com/typst/typst") {
            violations.push(PolicyViolation {
                code: "upstream-write-remote".into(),
                path: ".git/config".into(),
                detail: format!("remote {remote} can write to the upstream repository"),
            });
        }
    }
    let files =
        run_command("git", ["ls-files", "-co", "--exclude-standard"], Some(&repo))?;
    let files = require_success(files, "tracked and untracked file inventory")?;
    let mut inspected_files = 0;
    for raw_path in files.lines().map(str::trim).filter(|path| !path.is_empty()) {
        let path = normalize_repo_path(raw_path)?;
        if path.starts_with(".git/")
            || path.starts_with("target/")
            || path.starts_with(".tmp/")
            || path.contains("node_modules/")
            || is_binary_evidence_path(&path)
        {
            continue;
        }
        let full = repo.join(&path);
        if !full.is_file() {
            continue;
        }
        let metadata = fs::metadata(&full).map_err(|error| {
            AppError::authority(format!("cannot inspect {path}: {error}"))
        })?;
        if metadata.len() > MAX_SEMANTIC_EVIDENCE_BYTES {
            return Err(AppError::authority(format!(
                "semantic policy evidence exceeds {MAX_SEMANTIC_EVIDENCE_BYTES} bytes: {path}"
            )));
        }
        let bytes = fs::read(&full).map_err(|error| {
            AppError::authority(format!("cannot read policy evidence {path}: {error}"))
        })?;
        if bytes.contains(&0) {
            continue;
        }
        let Ok(text) = std::str::from_utf8(&bytes) else { continue };
        inspected_files += 1;
        if contains_credential_shape(text) {
            violations.push(PolicyViolation {
                code: "credential-shaped-content".into(),
                path: path.clone(),
                detail: "possible credential content was redacted".into(),
            });
        }
        if contains_upstream_write(text) {
            violations.push(PolicyViolation {
                code: "upstream-write-operation".into(),
                path: path.clone(),
                detail: "push or API operation targets typst/typst".into(),
            });
        }
        if is_runtime_path(&path) && contains_model_runtime(text, &path) {
            violations.push(PolicyViolation {
                code: "model-runtime-dependency".into(),
                path,
                detail:
                    "compiler/runtime contains an agent, MCP, model, or LLM integration"
                        .into(),
            });
        }
    }
    let required = ["AI_DISCLOSURE.md", "TRADEMARKS.md", "DCO", ".github/CODEOWNERS"];
    for path in required {
        if !repo.join(path).is_file() {
            violations.push(PolicyViolation {
                code: "missing-governance-file".into(),
                path: path.into(),
                detail: "required governance authority is missing".into(),
            });
        }
    }
    if !violations.is_empty() {
        let summary = violations
            .iter()
            .map(|violation| {
                format!("{} in {}: {}", violation.code, violation.path, violation.detail)
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err(AppError::policy(bounded(summary)));
    }
    Ok(PolicyReport {
        checked: "tracked-and-untracked-files",
        inspected_files,
        upstream_push_url: push_url,
        violations,
    })
}

fn is_binary_evidence_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(OsStr::to_str)
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some(
            "7z" | "avif"
                | "bin"
                | "bmp"
                | "bz2"
                | "gif"
                | "gz"
                | "ico"
                | "jpeg"
                | "jpg"
                | "otf"
                | "pdf"
                | "png"
                | "tar"
                | "ttf"
                | "wasm"
                | "webp"
                | "woff"
                | "woff2"
                | "xz"
                | "zip"
        )
    )
}

fn contains_credential_shape(text: &str) -> bool {
    let private_key_marker = ["-----BEGIN ", "PRIVATE KEY-----"].concat();
    let github_token_marker = ["g", "ho_"].concat();
    let openai_marker = ["s", "k-"].concat();
    let aws_marker = ["AK", "IA"].concat();
    let slack_marker = ["xox", "b-"].concat();
    text.contains(&private_key_marker)
        || contains_token(text, &github_token_marker, 20)
        || contains_token(text, &openai_marker, 20)
        || contains_token(text, &aws_marker, 16)
        || contains_token(text, &slack_marker, 20)
}

fn contains_upstream_write(text: &str) -> bool {
    let markers = [
        ["git push ", "upstream"].concat(),
        ["git push https://github.com/", "typst/typst"].concat(),
        ["git push git@github.com:", "typst/typst"].concat(),
        ["gh api repos/", "typst/typst"].concat(),
        ["api.github.com/repos/", "typst/typst"].concat(),
    ];
    markers.iter().any(|marker| text.contains(marker))
}

fn is_runtime_path(path: &str) -> bool {
    path.starts_with("crates/") && !path.starts_with("crates/typst-agent-dev/")
}

fn contains_model_runtime(text: &str, path: &str) -> bool {
    if path.ends_with("Cargo.toml") {
        let dependencies = [
            ["async", "-openai"].concat(),
            ["anthropic", "-sdk"].concat(),
            ["mcp", "-client"].concat(),
            ["ollama", "-rs"].concat(),
            ["rig", "-core"].concat(),
            ["ll", "m"].concat(),
        ];
        if dependencies.iter().any(|dependency| {
            text.lines().any(|line| {
                let trimmed = line.trim_start();
                trimmed.starts_with(dependency)
                    && trimmed[dependency.len()..].trim_start().starts_with('=')
            })
        }) {
            return true;
        }
    }
    let endpoints = [
        ["api.", "openai.com"].concat(),
        ["api.", "anthropic.com"].concat(),
        ["mcp", "://"].concat(),
    ];
    endpoints.iter().any(|endpoint| text.contains(endpoint))
}

fn contains_token(text: &str, marker: &str, minimum_suffix: usize) -> bool {
    let mut offset = 0;
    while let Some(found) = text[offset..].find(marker) {
        let start = offset + found;
        let before_is_boundary =
            start == 0 || !text.as_bytes()[start - 1].is_ascii_alphanumeric();
        let suffix = &text[start + marker.len()..];
        let suffix_len = suffix
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric())
            .count();
        if before_is_boundary && suffix_len >= minimum_suffix {
            return true;
        }
        offset = start + marker.len();
    }
    false
}

fn verify(tier: VerifyTier) -> AppResult<VerificationEvidence> {
    let mut checks = Vec::new();
    let policy = match policy_check() {
        Ok(_) => CheckEvidence {
            name: "policy-check".into(),
            status: "passed".into(),
            exit_code: Some(0),
            output: String::new(),
        },
        Err(error) => CheckEvidence {
            name: "policy-check".into(),
            status: "failed".into(),
            exit_code: Some(error.code.into()),
            output: error.message,
        },
    };
    let policy_failed = policy.status == "failed";
    checks.push(policy);
    let mut commands: Vec<(&str, Vec<&str>)> = vec![
        ("cargo fmt --check", vec!["fmt", "--all", "--check"]),
        ("cargo check -p typst-agent-dev", vec!["check", "-p", "typst-agent-dev"]),
    ];
    if matches!(tier, VerifyTier::Pr | VerifyTier::Full) {
        commands.push((
            "cargo test -p typst-agent-dev",
            vec!["test", "-p", "typst-agent-dev"],
        ));
    }
    if matches!(tier, VerifyTier::Full) {
        commands.push(("cargo test --workspace", vec!["test", "--workspace"]));
    }
    let repo = root()?;
    for (name, args) in commands {
        let result = run_command("cargo", args, Some(&repo))?;
        let passed = result.status == Some(0);
        checks.push(CheckEvidence {
            name: name.into(),
            status: if passed { "passed" } else { "failed" }.into(),
            exit_code: result.status,
            output: bounded(format!("{}{}", result.stdout, result.stderr)),
        });
        if !passed {
            break;
        }
    }
    let failed = policy_failed || checks.iter().any(|check| check.status == "failed");
    let evidence = VerificationEvidence {
        tier: format!("{tier:?}").to_lowercase(),
        checks,
        status: if failed { "failed".into() } else { "passed".into() },
    };
    if failed {
        return Err(AppError::verification(
            serde_json::to_string(&evidence)
                .unwrap_or_else(|_| "verification failed".into()),
        ));
    }
    Ok(evidence)
}

fn review_pack(base: &str) -> AppResult<Value> {
    let repo = root()?;
    let impact_report = impact(base)?;
    let policy = policy_check()?;
    let pack = json!({
        "contract_version": CONTRACT_VERSION,
        "base": base,
        "impact": impact_report,
        "policy": policy,
        "human_approval_required": true,
        "reference_updates_allowed": false,
    });
    let directory = repo.join(".tmp/agent");
    fs::create_dir_all(&directory)
        .map_err(|error| AppError::authority(error.to_string()))?;
    let path = directory.join("review-pack.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&pack)
            .map_err(|error| AppError::invalid(error.to_string()))?,
    )
    .map_err(|error| AppError::authority(error.to_string()))?;
    Ok(json!({"path": ".tmp/agent/review-pack.json", "pack": pack}))
}

fn upstream_check() -> AppResult<Value> {
    let repo = root()?;
    let fetch = run_command("git", ["fetch", "--tags", "upstream", "main"], Some(&repo))?;
    if fetch.status != Some(0) {
        return Err(AppError::authority(format!(
            "upstream fetch failed: {}",
            fetch.stderr.trim()
        )));
    }
    let mirror = require_success(
        run_command(
            "git",
            ["rev-parse", "refs/heads/mirror/upstream-main"],
            Some(&repo),
        )?,
        "mirror ref",
    )?
    .trim()
    .to_string();
    let fetched = require_success(
        run_command("git", ["rev-parse", "refs/remotes/upstream/main"], Some(&repo))?,
        "upstream ref",
    )?
    .trim()
    .to_string();
    if mirror != fetched {
        return Err(AppError::policy(format!(
            "mirror/upstream-main {mirror} differs from upstream/main {fetched}"
        )));
    }
    let push_url = require_success(
        run_command("git", ["remote", "get-url", "--push", "upstream"], Some(&repo))?,
        "upstream push URL",
    )?
    .trim()
    .to_string();
    if push_url.contains("github.com/typst/typst") {
        return Err(AppError::policy("upstream push URL is writable"));
    }
    Ok(
        json!({"mirror": mirror, "upstream": fetched, "identical": true, "push_url": push_url, "tags": "fetched-and-preserved"}),
    )
}

fn eval() -> AppResult<Value> {
    let repo = root()?;
    let context = context(&ContextArgs {
        paths: vec![PathBuf::from("crates/typst-syntax/src/parser.rs")],
    })?;
    let schema: Value = serde_json::from_str(
        &fs::read_to_string(repo.join("agent-contract/v1/schema.json"))
            .map_err(|e| AppError::authority(e.to_string()))?,
    )
    .map_err(|e| AppError::invalid(e.to_string()))?;
    let kinds = schema
        .get("properties")
        .and_then(|p| p.get("kind"))
        .and_then(|k| k.get("enum"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if kinds != 8 || context.guides.is_empty() {
        return Err(AppError::verification(
            "control-plane self-check did not find the contract or scoped guide",
        ));
    }
    Ok(
        json!({"checks": ["contract-schema", "scoped-context", "secret-boundary"], "model_calls": 0, "status": "passed"}),
    )
}

fn release_manifest() -> AppResult<Value> {
    let repo = root()?;
    let downstream_sha = require_success(
        run_command("git", ["rev-parse", "HEAD"], Some(&repo))?,
        "downstream ref",
    )?
    .trim()
    .to_string();
    let upstream_sha = require_success(
        run_command(
            "git",
            ["rev-parse", "refs/heads/mirror/upstream-main"],
            Some(&repo),
        )?,
        "upstream mirror",
    )?
    .trim()
    .to_string();
    let cargo = fs::read_to_string(repo.join("Cargo.toml"))
        .map_err(|error| AppError::authority(error.to_string()))?;
    let value: toml::Value = cargo
        .parse()
        .map_err(|error| AppError::invalid(format!("invalid Cargo.toml: {error}")))?;
    let version = value
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| AppError::invalid("workspace.package.version is missing"))?;
    let manifest = json!({
        "contract_version": CONTRACT_VERSION,
        "product": "typst-agent",
        "upstream_version": version,
        "release_tag": format!("v{version}-agent.0"),
        "upstream_sha": upstream_sha,
        "downstream_sha": downstream_sha,
        "checksums": [],
        "sbom": null,
        "sigstore": null,
        "provenance": null,
        "reproducibility": {"required": true, "verified": false},
    });
    let directory = repo.join(".tmp/agent");
    fs::create_dir_all(&directory)
        .map_err(|error| AppError::authority(error.to_string()))?;
    fs::write(
        directory.join("release-manifest.json"),
        serde_json::to_vec_pretty(&manifest)
            .map_err(|error| AppError::invalid(error.to_string()))?,
    )
    .map_err(|error| AppError::authority(error.to_string()))?;
    Ok(json!({"path": ".tmp/agent/release-manifest.json", "manifest": manifest}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_detector_requires_a_boundary_and_length() {
        assert!(!contains_token("task-owned", "sk-", 20));
        assert!(!contains_token("sk-short", "sk-", 20));
        let token = ["s", "k-12345678901234567890"].concat();
        assert!(contains_token(&token, &["s", "k-"].concat(), 20));
        let embedded = ["a", &token].concat();
        assert!(!contains_token(&embedded, &["s", "k-"].concat(), 20));
    }

    #[test]
    fn context_paths_reject_absolute_and_parent_components() {
        assert!(normalize_repo_path("/tmp/outside").is_err());
        assert!(normalize_repo_path("../outside").is_err());
        assert!(normalize_repo_path("crate/../../outside").is_err());
        assert!(normalize_repo_path("C:\\outside").is_err());
        assert_eq!(
            normalize_repo_path("./crates/typst-syntax/src/lib.rs").unwrap(),
            "crates/typst-syntax/src/lib.rs"
        );
    }

    #[test]
    fn area_rule_matches_only_declared_paths() {
        let rule = AreaRule {
            id: "syntax".into(),
            path_prefixes: vec!["crates/typst-syntax/".into()],
            exact_paths: vec!["Cargo.toml".into()],
            authority_sources: vec!["crates/typst-syntax/src/".into()],
            guide: ".agents/areas/parser-spans.md".into(),
            required_checks: vec!["cargo test -p typst-syntax".into()],
            invariant_ids: vec!["syntax-parse-total".into()],
        };
        assert!(rule_matches_path(&rule, "crates/typst-syntax/src/lib.rs"));
        assert!(rule_matches_path(&rule, "Cargo.toml"));
        assert!(!rule_matches_path(&rule, "crates/typst-cli/src/main.rs"));
    }

    #[test]
    fn invariant_records_reject_unknown_fields() {
        let record = "
id: test
scope: tests
statement: statement
rationale: rationale
authority_source: tests/src/tests.rs
required_checks: [test]
review_prompts: [review]
upstream_anchor: v0.15.1
upstream_sha: a51e028041cac426f97d34335bb01d8f1d8e5e8f
unexpected: rejected
";
        assert!(serde_yaml::from_str::<InvariantRecord>(record).is_err());
    }

    #[test]
    fn reverse_dependencies_are_transitive() {
        let reverse = BTreeMap::from([
            ("syntax".into(), BTreeSet::from(["eval".into(), "ide".into()])),
            ("eval".into(), BTreeSet::from(["compiler".into()])),
            ("compiler".into(), BTreeSet::from(["cli".into()])),
        ]);
        assert_eq!(
            transitive_dependents("syntax", &reverse),
            BTreeSet::from([
                "cli".into(),
                "compiler".into(),
                "eval".into(),
                "ide".into(),
            ])
        );
    }

    #[test]
    fn upstream_operations_are_detected_without_echoing_credentials() {
        let push = ["git push ", "upstream HEAD:main"].concat();
        let api = ["gh api repos/", "typst/typst/hooks"].concat();
        assert!(contains_upstream_write(&push));
        assert!(contains_upstream_write(&api));
        let secret = ["g", "ho_123456789012345678901234"].concat();
        assert!(contains_credential_shape(&secret));
    }

    #[test]
    fn output_is_bounded() {
        let output = bounded(format!("{}é", "x".repeat(MAX_OUTPUT_BYTES - 1)));
        assert!(output.len() <= MAX_OUTPUT_BYTES + "\n[output truncated]".len());
        assert!(output.ends_with("[output truncated]"));
    }
}
