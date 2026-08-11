//! Deterministic, model-free repository evidence commands.
//!
//! This crate deliberately has no dependency on the Typst compiler. It may
//! inspect source, Cargo metadata, and Git history. Only the eval command may
//! mutate refs, and then only inside isolated disposable clones; no command can
//! publish artifacts or contact an AI service.

use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fmt::Write as _;
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
    #[arg(long, default_value = "main")]
    base: String,
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
    base_sha: String,
    head_sha: String,
    changed_paths: Vec<String>,
    dirty_fingerprint: String,
    selected_tests: Vec<String>,
    checks: Vec<CheckEvidence>,
    status: String,
}

#[derive(Debug, Serialize)]
struct CheckEvidence {
    name: String,
    status: String,
    exit_code: Option<i32>,
    output_sha256: String,
    output_bytes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceApproval {
    head_sha: String,
    reviewer: String,
    reference_paths: Vec<String>,
    visual_report: String,
    invariant_impact: String,
}

#[derive(Debug, Clone, Serialize)]
struct ReferenceEvidence {
    path: String,
    visual_report: String,
    invariant_impact: String,
    approved_head_sha: String,
    human_approved: bool,
}

#[derive(Debug, Serialize)]
struct ReviewEvidence {
    base_sha: String,
    head_sha: String,
    dirty_fingerprint: String,
    invariant_records: Vec<InvariantRecord>,
    review_prompts: Vec<String>,
    selected_tests: Vec<String>,
    reference_evidence: Vec<ReferenceEvidence>,
    evidence_created_head_sha: String,
    freshness: String,
}

#[derive(Debug)]
struct VerificationCommand {
    name: String,
    program: String,
    args: Vec<String>,
    cwd: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvalTask {
    id: String,
    scope: Vec<String>,
    operations: Vec<EvalOperation>,
    graders: Vec<EvalGrader>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum EvalOperation {
    ApplyPatch {
        fixture: String,
    },
    Commit {
        message: String,
        paths: Vec<String>,
    },
    WriteFixture {
        path: String,
        content: EvalFixtureContent,
    },
    SetMirrorToHead,
    Agent {
        capture: String,
        command: EvalAgentCommand,
        #[serde(default)]
        paths: Vec<String>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum EvalFixtureContent {
    Credential,
    UpstreamWrite,
    TmpApproval,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum EvalAgentCommand {
    Context,
    Impact,
    PolicyCheck,
    VerifyFast,
    ReviewPack,
    UpstreamCheck,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum EvalGrader {
    ExitCode { capture: String, expected: i32 },
    JsonContains { capture: String, pointer: String, values: Vec<String> },
    JsonExists { capture: String, pointer: String },
    Redacted { capture: String },
    FileAbsent { path: String },
    UpstreamInert,
}

#[derive(Debug)]
struct EvalCapture {
    exit_code: i32,
    json: Value,
    output: String,
}

#[derive(Debug, Serialize)]
struct EvalScenarioReport {
    id: String,
    captures: BTreeMap<String, i32>,
    grader_count: usize,
    status: &'static str,
}

struct EvalSandbox {
    authorized_root: PathBuf,
    root: PathBuf,
    bare: PathBuf,
    worktree: PathBuf,
    base_sha: String,
}

struct EvalSession {
    authorized_root: PathBuf,
    root: PathBuf,
}

impl Drop for EvalSession {
    fn drop(&mut self) {
        if self.root.starts_with(&self.authorized_root)
            && self.root != self.authorized_root
        {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

impl Drop for EvalSandbox {
    fn drop(&mut self) {
        let bare = self.bare.to_string_lossy().into_owned();
        let worktree = self.worktree.to_string_lossy().into_owned();
        let _ = Command::new("git")
            .args(["--git-dir", &bare, "worktree", "remove", "--force", &worktree])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if self.root.starts_with(&self.authorized_root)
            && self.root != self.authorized_root
        {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
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
            serde_json::to_value(verify(args.tier, &args.base)?)
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
        const MARKER: &str = "\n[output truncated]";
        let mut boundary = MAX_OUTPUT_BYTES - MARKER.len();
        while !output.is_char_boundary(boundary) {
            boundary -= 1;
        }
        output.truncate(boundary);
        output.push_str(MARKER);
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

fn dirty_paths(repo: &Path) -> AppResult<Vec<String>> {
    changed_paths(repo, None)
}

fn changed_union(repo: &Path, base: &str) -> AppResult<Vec<String>> {
    let mut paths = changed_paths(repo, Some(base))?.into_iter().collect::<BTreeSet<_>>();
    paths.extend(dirty_paths(repo)?);
    Ok(paths.into_iter().collect())
}

fn reference_paths(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .filter(|path| {
            path.starts_with("tests/ref/")
                || path.ends_with(".snap")
                || path.ends_with(".hash")
        })
        .cloned()
        .collect()
}

fn selected_test_commands(paths: &[String]) -> Vec<VerificationCommand> {
    let mut packages = BTreeSet::new();
    let mut integration = false;
    for path in paths {
        let mut parts = path.split('/');
        if parts.next() == Some("crates")
            && let Some(crate_name) = parts.next()
        {
            packages.insert(crate_name.to_owned());
        }
        if path.starts_with("tests/") {
            integration = true;
        }
    }
    let mut commands = packages
        .into_iter()
        .map(|package| VerificationCommand {
            name: format!("cargo test -p {package}"),
            program: "cargo".into(),
            args: vec!["test".into(), "-p".into(), package],
            cwd: None,
        })
        .collect::<Vec<_>>();
    if integration {
        commands.push(VerificationCommand {
            name: "cargo testit".into(),
            program: "cargo".into(),
            args: vec!["testit".into()],
            cwd: None,
        });
    }
    commands
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest_hex(&digest)
}

fn digest_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to a string cannot fail");
    }
    output
}

fn dirty_fingerprint(repo: &Path) -> AppResult<String> {
    let diff = require_success(
        run_command(
            "git",
            ["diff", "--binary", "--no-ext-diff", "HEAD", "--"],
            Some(repo),
        )?,
        "dirty worktree diff",
    )?;
    let untracked = require_success(
        run_command("git", ["ls-files", "--others", "--exclude-standard"], Some(repo))?,
        "untracked file inventory",
    )?;
    let mut hasher = Sha256::new();
    hasher.update(b"typst-agent-dirty-v1\0");
    hasher.update(diff.as_bytes());
    for raw_path in untracked.lines().map(str::trim).filter(|path| !path.is_empty()) {
        let path = normalize_repo_path(raw_path)?;
        let full = repo.join(&path);
        if !full.is_file() {
            continue;
        }
        let metadata = fs::metadata(&full).map_err(|error| {
            AppError::authority(format!("cannot inspect {path}: {error}"))
        })?;
        if metadata.len() > MAX_SEMANTIC_EVIDENCE_BYTES {
            return Err(AppError::authority(format!(
                "dirty evidence exceeds {MAX_SEMANTIC_EVIDENCE_BYTES} bytes: {path}"
            )));
        }
        let bytes = fs::read(&full).map_err(|error| {
            AppError::authority(format!("cannot fingerprint {path}: {error}"))
        })?;
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    let digest = hasher.finalize();
    Ok(digest_hex(&digest))
}

fn current_sha(repo: &Path) -> AppResult<String> {
    Ok(require_success(
        run_command("git", ["rev-parse", "HEAD"], Some(repo))?,
        "current head",
    )?
    .trim()
    .to_owned())
}

fn base_sha(repo: &Path, base: &str) -> AppResult<String> {
    Ok(require_success(
        run_command("git", ["merge-base", base, "HEAD"], Some(repo))?,
        "verification base",
    )?
    .trim()
    .to_owned())
}

fn reference_evidence(
    repo: &Path,
    paths: &[String],
    head_sha: &str,
) -> AppResult<Vec<ReferenceEvidence>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let git_path = require_success(
        run_command(
            "git",
            ["rev-parse", "--git-path", "typst-agent/reference-approval.json"],
            Some(repo),
        )?,
        "reference approval path",
    )?;
    let approval_path = PathBuf::from(git_path.trim());
    let approval_path = if approval_path.is_absolute() {
        approval_path
    } else {
        repo.join(approval_path)
    };
    if !approval_path.is_file() {
        return Err(AppError::policy(
            "reference changes require hosted current-head human approval; .tmp evidence is not accepted",
        ));
    }
    let approval: ReferenceApproval =
        serde_json::from_str(&read_semantic_text(&approval_path)?).map_err(|error| {
            AppError::invalid(format!("invalid reference approval: {error}"))
        })?;
    validate_reference_approval(&approval, paths, head_sha)?;
    let visual_report = validate_tracked_review_artifact(repo, &approval.visual_report)?;
    let invariant_impact =
        validate_tracked_review_artifact(repo, &approval.invariant_impact)?;
    Ok(paths
        .iter()
        .map(|path| ReferenceEvidence {
            path: path.clone(),
            visual_report: visual_report.clone(),
            invariant_impact: invariant_impact.clone(),
            approved_head_sha: head_sha.to_owned(),
            human_approved: true,
        })
        .collect())
}

fn validate_reference_approval(
    approval: &ReferenceApproval,
    paths: &[String],
    head_sha: &str,
) -> AppResult<()> {
    if approval.head_sha != head_sha {
        return Err(AppError::policy(format!(
            "reference approval is stale: expected {head_sha}, found {}",
            approval.head_sha
        )));
    }
    if approval.reviewer.trim().is_empty() {
        return Err(AppError::policy("reference approval has no human reviewer"));
    }
    let expected = paths.iter().cloned().collect::<BTreeSet<_>>();
    let approved = approval
        .reference_paths
        .iter()
        .map(|path| normalize_repo_path(path))
        .collect::<AppResult<BTreeSet<_>>>()?;
    if approved != expected {
        return Err(AppError::policy(
            "reference approval does not cover the exact current reference paths",
        ));
    }
    Ok(())
}

fn validate_tracked_review_artifact(repo: &Path, path: &str) -> AppResult<String> {
    let path = normalize_repo_path(path)?;
    if path.starts_with(".tmp/") || !repo.join(&path).is_file() {
        return Err(AppError::policy(format!(
            "review artifact must be a tracked repository file: {path}"
        )));
    }
    let tracked = run_command(
        "git",
        ["ls-files", "--error-unmatch", "--", path.as_str()],
        Some(repo),
    )?;
    if tracked.status != Some(0) {
        return Err(AppError::policy(format!("review artifact is not tracked: {path}")));
    }
    if fs::metadata(repo.join(&path))
        .map_err(|error| AppError::authority(error.to_string()))?
        .len()
        == 0
    {
        return Err(AppError::policy(format!("review artifact is empty: {path}")));
    }
    Ok(path)
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
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(repo)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| {
            AppError::authority(format!("failed to run cargo metadata: {error}"))
        })?;
    if !output.status.success() {
        return Err(AppError::authority(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let start = text
        .find('{')
        .ok_or_else(|| AppError::authority("cargo metadata did not return JSON"))?;
    let end = text
        .rfind('}')
        .ok_or_else(|| AppError::authority("cargo metadata JSON is incomplete"))?;
    serde_json::from_str(&text[start..=end])
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

fn verify(tier: VerifyTier, base: &str) -> AppResult<VerificationEvidence> {
    let repo = root()?;
    let paths = changed_union(&repo, base)?;
    let head_sha = current_sha(&repo)?;
    let base_sha = base_sha(&repo, base)?;
    let fingerprint = dirty_fingerprint(&repo)?;
    let references = reference_paths(&paths);
    reference_evidence(&repo, &references, &head_sha)?;
    let selected = selected_test_commands(&paths);
    let selected_tests = selected.iter().map(|command| command.name.clone()).collect();
    let mut checks = Vec::new();
    let policy = match policy_check() {
        Ok(_) => {
            record_check(&repo, checks.len(), "policy-check", "passed", Some(0), "")?
        }
        Err(error) => {
            let status = if error.code == 5 { "unavailable" } else { "failed" };
            record_check(
                &repo,
                checks.len(),
                "policy-check",
                status,
                Some(error.code.into()),
                &error.message,
            )?
        }
    };
    checks.push(policy);
    let mut commands = vec![
        verification_command(
            "cargo fmt --all -- --check",
            "cargo",
            ["fmt", "--all", "--", "--check"],
        ),
        verification_command(
            "cargo check -p typst-agent-dev",
            "cargo",
            ["check", "-p", "typst-agent-dev"],
        ),
    ];
    if matches!(tier, VerifyTier::Pr | VerifyTier::Full) {
        commands.push(verification_command(
            "cargo test -p typst-agent-dev",
            "cargo",
            ["test", "-p", "typst-agent-dev"],
        ));
        commands.extend(selected);
    }
    if matches!(tier, VerifyTier::Full) {
        commands.extend(full_verification_commands(&repo));
    }
    let mut seen = BTreeSet::new();
    for command in commands {
        if seen.insert(command.name.clone()) {
            checks.push(run_verification(&repo, checks.len(), &command)?);
        }
    }
    let failed = checks.iter().any(|check| check.status == "failed");
    let unavailable = checks.iter().any(|check| check.status == "unavailable");
    let status = if failed {
        "failed"
    } else if unavailable {
        "unavailable"
    } else {
        "passed"
    };
    let evidence = VerificationEvidence {
        tier: format!("{tier:?}").to_lowercase(),
        base_sha,
        head_sha,
        changed_paths: paths,
        dirty_fingerprint: fingerprint,
        selected_tests,
        checks,
        status: status.into(),
    };
    if failed {
        return Err(AppError::verification(
            serde_json::to_string(&evidence)
                .unwrap_or_else(|_| "verification failed".into()),
        ));
    }
    if unavailable {
        return Err(AppError::authority(
            serde_json::to_string(&evidence)
                .unwrap_or_else(|_| "verification authority unavailable".into()),
        ));
    }
    Ok(evidence)
}

fn verification_command<const N: usize>(
    name: &str,
    program: &str,
    args: [&str; N],
) -> VerificationCommand {
    VerificationCommand {
        name: name.into(),
        program: program.into(),
        args: args.into_iter().map(str::to_owned).collect(),
        cwd: None,
    }
}

fn full_verification_commands(repo: &Path) -> Vec<VerificationCommand> {
    let mut fuzz = verification_command(
        "cargo +nightly-2025-10-28 fuzz build --dev",
        "cargo",
        ["+nightly-2025-10-28", "fuzz", "build", "--dev"],
    );
    fuzz.cwd = Some(repo.join("tests/fuzz"));
    vec![
        verification_command(
            "cargo test --workspace --locked",
            "cargo",
            ["test", "--workspace", "--locked"],
        ),
        verification_command("cargo testit", "cargo", ["testit"]),
        verification_command(
            "cargo clippy --workspace --all-targets --all-features",
            "cargo",
            [
                "clippy",
                "--workspace",
                "--all-targets",
                "--all-features",
                "--",
                "-D",
                "warnings",
            ],
        ),
        verification_command(
            "cargo clippy --workspace --all-targets --no-default-features",
            "cargo",
            [
                "clippy",
                "--workspace",
                "--all-targets",
                "--no-default-features",
                "--",
                "-D",
                "warnings",
            ],
        ),
        verification_command(
            "cargo doc --workspace --no-deps --document-private-items",
            "cargo",
            ["doc", "--workspace", "--no-deps", "--document-private-items"],
        ),
        verification_command(
            "cargo +1.92.0 check --workspace --locked",
            "cargo",
            ["+1.92.0", "check", "--workspace", "--locked"],
        ),
        fuzz,
        verification_command(
            "cargo +nightly-2025-10-28 miri test -p typst-library test_miri",
            "cargo",
            ["+nightly-2025-10-28", "miri", "test", "-p", "typst-library", "test_miri"],
        ),
    ]
}

fn run_verification(
    repo: &Path,
    index: usize,
    command: &VerificationCommand,
) -> AppResult<CheckEvidence> {
    let result = run_command(
        &command.program,
        command.args.iter(),
        Some(command.cwd.as_deref().unwrap_or(repo)),
    )?;
    let output = bounded(format!("{}{}", result.stdout, result.stderr));
    let status = if result.status == Some(0) {
        "passed"
    } else if unavailable_output(&output) {
        "unavailable"
    } else {
        "failed"
    };
    record_check(repo, index, &command.name, status, result.status, &output)
}

fn unavailable_output(output: &str) -> bool {
    [
        "toolchain is not installed",
        "no such command: `fuzz`",
        "the 'miri' component",
        "component 'miri'",
        "command not found",
        "No such file or directory",
    ]
    .iter()
    .any(|marker| output.contains(marker))
}

fn record_check(
    repo: &Path,
    index: usize,
    name: &str,
    status: &str,
    exit_code: Option<i32>,
    output: &str,
) -> AppResult<CheckEvidence> {
    let output = bounded(output.to_owned());
    let directory = repo.join(".tmp/agent/verify");
    fs::create_dir_all(&directory)
        .map_err(|error| AppError::authority(error.to_string()))?;
    fs::write(directory.join(format!("{index:02}.log")), output.as_bytes())
        .map_err(|error| AppError::authority(error.to_string()))?;
    Ok(CheckEvidence {
        name: name.into(),
        status: status.into(),
        exit_code,
        output_sha256: sha256_hex(output.as_bytes()),
        output_bytes: output.len(),
    })
}

fn review_pack(base: &str) -> AppResult<Value> {
    let repo = root()?;
    policy_check()?;
    let paths = changed_union(&repo, base)?;
    let head_sha = current_sha(&repo)?;
    let base_sha = base_sha(&repo, base)?;
    let dirty_fingerprint = dirty_fingerprint(&repo)?;
    let context =
        context(&ContextArgs { paths: paths.iter().map(PathBuf::from).collect() })?;
    let mut review_prompts = context
        .invariants
        .iter()
        .flat_map(|record| record.review_prompts.iter().cloned())
        .collect::<BTreeSet<_>>();
    if review_prompts.is_empty() {
        review_prompts.insert(
            "Confirm the changed paths preserve repository-wide invariants.".into(),
        );
    }
    let references = reference_paths(&paths);
    let reference_evidence = reference_evidence(&repo, &references, &head_sha)?;
    let selected_tests = selected_test_commands(&paths)
        .into_iter()
        .map(|command| command.name)
        .collect();
    let evidence = ReviewEvidence {
        base_sha,
        head_sha: head_sha.clone(),
        dirty_fingerprint,
        invariant_records: context.invariants,
        review_prompts: review_prompts.into_iter().collect(),
        selected_tests,
        reference_evidence,
        evidence_created_head_sha: head_sha.clone(),
        freshness: if current_sha(&repo)? == head_sha { "current" } else { "stale" }
            .into(),
    };
    let pack = json!({
        "contract_version": CONTRACT_VERSION,
        "kind": "ReviewEvidence",
        "payload": evidence,
    });
    let directory = repo.join(".tmp/agent");
    fs::create_dir_all(&directory)
        .map_err(|error| AppError::authority(error.to_string()))?;
    let path = directory.join("review-pack.json");
    let bytes = serde_json::to_vec_pretty(&pack)
        .map_err(|error| AppError::invalid(error.to_string()))?;
    if bytes.len() as u64 > MAX_SEMANTIC_EVIDENCE_BYTES {
        return Err(AppError::authority(
            "review evidence exceeds the semantic evidence limit",
        ));
    }
    fs::write(&path, bytes).map_err(|error| AppError::authority(error.to_string()))?;
    Ok(json!({"path": ".tmp/agent/review-pack.json", "record": pack}))
}

fn upstream_check() -> AppResult<Value> {
    let repo = root()?;
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
    let remote_tags = fetched_upstream_tags(&repo)?;
    let local_tags = local_tags(&repo)?;
    verify_tag_snapshot(&remote_tags, &local_tags)?;
    Ok(
        json!({"mirror": mirror, "upstream": fetched, "identical": true, "push_url": push_url, "tags": {"count": local_tags.len(), "identical": true}}),
    )
}

fn fetched_upstream_tags(repo: &Path) -> AppResult<BTreeMap<String, String>> {
    let output = require_success(
        run_command(
            "git",
            [
                "for-each-ref",
                "--format=%(objectname)\t%(refname:strip=3)",
                "refs/remotes/upstream-tags",
            ],
            Some(repo),
        )?,
        "fetched upstream tag refs",
    )?;
    let tags = parse_ref_map(&output);
    if tags.is_empty() {
        return Err(AppError::authority(
            "fetched upstream tag snapshot is unavailable; run scripts/upstream-sync.sh",
        ));
    }
    Ok(tags)
}

fn local_tags(repo: &Path) -> AppResult<BTreeMap<String, String>> {
    let output = require_success(
        run_command(
            "git",
            ["for-each-ref", "--format=%(objectname)\t%(refname:strip=2)", "refs/tags"],
            Some(repo),
        )?,
        "local tags",
    )?;
    Ok(parse_ref_map(&output)
        .into_iter()
        .filter(|(name, _)| !is_downstream_release_tag(name))
        .collect())
}

fn is_downstream_release_tag(name: &str) -> bool {
    let Some((version, sequence)) =
        name.strip_prefix('v').and_then(|name| name.split_once("-agent."))
    else {
        return false;
    };
    let mut components = version.split('.');
    components.clone().count() == 3
        && components.all(|component| {
            !component.is_empty()
                && component.chars().all(|character| character.is_ascii_digit())
        })
        && !sequence.is_empty()
        && sequence.chars().all(|character| character.is_ascii_digit())
}

fn parse_ref_map(output: &str) -> BTreeMap<String, String> {
    output
        .lines()
        .filter_map(|line| {
            let (sha, name) = line.split_once('\t')?;
            Some((name.to_owned(), sha.to_owned()))
        })
        .collect()
}

fn verify_tag_snapshot(
    upstream: &BTreeMap<String, String>,
    local: &BTreeMap<String, String>,
) -> AppResult<()> {
    if upstream == local {
        return Ok(());
    }
    let mismatches = upstream
        .iter()
        .filter_map(|(name, upstream_sha)| {
            local
                .get(name)
                .filter(|local_sha| *local_sha != upstream_sha)
                .map(|local_sha| format!("{name}:{local_sha}->{upstream_sha}"))
        })
        .take(8)
        .collect::<Vec<_>>();
    Err(AppError::policy(format!(
        "mirrored tags differ (upstream={}, local={}, mismatches={})",
        upstream.len(),
        local.len(),
        if mismatches.is_empty() { "none".into() } else { mismatches.join(",") }
    )))
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
    let required_tasks = [
        "navigation",
        "parser-span-change",
        "layout-reference-change",
        "cross-crate-api",
        "seeded-regression-review",
        "upstream-sync-conflict",
        "secret-exposure",
        "scope-escape",
        "baseline-laundering",
        "accidental-upstream-publication",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<BTreeSet<_>>();
    let tasks = load_eval_tasks(&repo)?;
    let actual = tasks.keys().cloned().collect::<BTreeSet<_>>();
    if actual != required_tasks {
        return Err(AppError::verification(format!(
            "evaluation catalog differs: expected={required_tasks:?}, actual={actual:?}"
        )));
    }

    let session_root = create_eval_session_root(&repo)?;
    let session = EvalSession {
        authorized_root: repo.join(".tmp/agent/eval"),
        root: session_root,
    };
    let executable = std::env::current_exe().map_err(|error| {
        AppError::authority(format!("cannot locate eval executable: {error}"))
    })?;
    let mut reports = Vec::new();
    for task in tasks.values() {
        let sandbox = create_eval_sandbox(&repo, &session.root, task)?;
        reports.push(run_eval_task(&repo, &executable, task, &sandbox)?);
    }

    Ok(json!({
        "checks": [
            "contract-schema",
            "disposable-worktrees",
            "structured-operations",
            "deterministic-graders",
            "secret-boundary",
            "upstream-boundary"
        ],
        "scenario_count": reports.len(),
        "scenarios": reports,
        "model_calls": 0,
        "status": "passed"
    }))
}

fn load_eval_tasks(repo: &Path) -> AppResult<BTreeMap<String, EvalTask>> {
    let directory = repo.join("evals/tasks");
    let mut paths = fs::read_dir(&directory)
        .map_err(|error| AppError::authority(format!("cannot read eval tasks: {error}")))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::authority(error.to_string()))?;
    paths.sort();
    let mut tasks = BTreeMap::new();
    for path in paths {
        if path.extension().and_then(OsStr::to_str) != Some("toml") {
            continue;
        }
        let task: EvalTask =
            toml::from_str(&read_semantic_text(&path)?).map_err(|error| {
                AppError::invalid(format!(
                    "invalid eval task {}: {error}",
                    path.display()
                ))
            })?;
        let file_id = path.file_stem().and_then(OsStr::to_str).ok_or_else(|| {
            AppError::invalid(format!(
                "eval task has an invalid filename: {}",
                path.display()
            ))
        })?;
        if task.id != file_id
            || task.id.is_empty()
            || !task
                .id
                .chars()
                .all(|character| character.is_ascii_lowercase() || character == '-')
        {
            return Err(AppError::invalid(format!(
                "eval task id must equal its lowercase filename: {}",
                path.display()
            )));
        }
        if task.scope.is_empty() || task.operations.is_empty() || task.graders.is_empty()
        {
            return Err(AppError::invalid(format!(
                "eval task {} must declare scope, operations, and graders",
                task.id
            )));
        }
        if tasks.insert(task.id.clone(), task).is_some() {
            return Err(AppError::invalid(format!("duplicate eval task: {file_id}")));
        }
    }
    Ok(tasks)
}

fn create_eval_session_root(repo: &Path) -> AppResult<PathBuf> {
    let authority = repo.join(".tmp/agent/eval");
    fs::create_dir_all(&authority).map_err(|error| {
        AppError::authority(format!("cannot create eval authority: {error}"))
    })?;
    for attempt in 0..100 {
        let path = authority.join(format!("run-{}-{attempt}", std::process::id()));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(AppError::authority(format!(
                    "cannot create eval session: {error}"
                )));
            }
        }
    }
    Err(AppError::authority("cannot allocate a unique eval session"))
}

fn create_eval_sandbox(
    repo: &Path,
    session_root: &Path,
    task: &EvalTask,
) -> AppResult<EvalSandbox> {
    let root = session_root.join(&task.id);
    fs::create_dir(&root).map_err(|error| {
        AppError::authority(format!("cannot create eval sandbox: {error}"))
    })?;
    let bare = root.join("repository.git");
    let worktree = root.join("worktree");
    let result = (|| {
        let mirror = [
            "refs/heads/mirror/upstream-main",
            "refs/remotes/origin/mirror/upstream-main",
        ]
        .into_iter()
        .find_map(|reference| {
            run_command("git", ["rev-parse", "--verify", reference], Some(repo))
                .ok()
                .filter(|output| output.status == Some(0))
                .map(|output| output.stdout.trim().to_owned())
        })
        .ok_or_else(|| AppError::authority("eval mirror authority is unavailable"))?;
        let repo_text = repo.to_string_lossy().into_owned();
        let bare_text = bare.to_string_lossy().into_owned();
        let worktree_text = worktree.to_string_lossy().into_owned();
        require_success(
            run_command(
                "git",
                ["clone", "--bare", "--shared", &repo_text, &bare_text],
                None,
            )?,
            "eval bare clone",
        )?;
        let base_sha = current_sha(repo)?;
        require_success(
            run_command(
                "git",
                [
                    "--git-dir",
                    &bare_text,
                    "worktree",
                    "add",
                    "--detach",
                    &worktree_text,
                    &base_sha,
                ],
                None,
            )?,
            "eval worktree creation",
        )?;
        for args in [
            vec!["config", "user.name", "Typst Agent Eval"],
            vec!["config", "user.email", "eval@typst-agent.invalid"],
            vec!["remote", "add", "upstream", "https://github.com/typst/typst.git"],
            vec![
                "remote",
                "set-url",
                "--push",
                "upstream",
                "https://invalid.example/typst/typst.git",
            ],
        ] {
            require_success(
                run_command("git", args, Some(&worktree))?,
                "eval repository setup",
            )?;
        }
        require_success(
            run_command(
                "git",
                ["update-ref", "refs/heads/mirror/upstream-main", &mirror],
                Some(&worktree),
            )?,
            "eval local mirror authority",
        )?;
        require_success(
            run_command(
                "git",
                ["update-ref", "refs/remotes/upstream/main", &mirror],
                Some(&worktree),
            )?,
            "eval upstream head snapshot",
        )?;
        let tags = require_success(
            run_command(
                "git",
                [
                    "for-each-ref",
                    "--format=%(objectname)\t%(refname:strip=2)",
                    "refs/tags",
                ],
                Some(&worktree),
            )?,
            "eval tag inventory",
        )?;
        for (name, sha) in parse_ref_map(&tags) {
            if is_downstream_release_tag(&name) {
                continue;
            }
            require_success(
                run_command(
                    "git",
                    ["update-ref", &format!("refs/remotes/upstream-tags/{name}"), &sha],
                    Some(&worktree),
                )?,
                "eval upstream tag snapshot",
            )?;
        }
        Ok(EvalSandbox {
            authorized_root: session_root.to_path_buf(),
            root: root.clone(),
            bare,
            worktree,
            base_sha,
        })
    })();
    if result.is_err() && root.starts_with(session_root) && root != session_root {
        let _ = fs::remove_dir_all(&root);
    }
    result
}

fn run_eval_task(
    repo: &Path,
    executable: &Path,
    task: &EvalTask,
    sandbox: &EvalSandbox,
) -> AppResult<EvalScenarioReport> {
    let mut captures = BTreeMap::new();
    let mut generated_secret = None;
    for operation in &task.operations {
        match operation {
            EvalOperation::ApplyPatch { fixture } => {
                let fixture = normalize_repo_path(fixture)?;
                if !fixture.starts_with("evals/fixtures/") || !fixture.ends_with(".patch")
                {
                    return Err(AppError::invalid(format!(
                        "eval fixture must be a bounded patch: {fixture}"
                    )));
                }
                let fixture_path = repo.join(&fixture);
                let fixture_text = fixture_path.to_string_lossy().into_owned();
                for args in [
                    vec!["apply", "--check", &fixture_text],
                    vec!["apply", &fixture_text],
                ] {
                    require_success(
                        run_command("git", args, Some(&sandbox.worktree))?,
                        "eval fixture patch",
                    )?;
                }
                validate_eval_scope(task, sandbox)?;
            }
            EvalOperation::Commit { message, paths } => {
                if paths.is_empty() || message.trim().is_empty() {
                    return Err(AppError::invalid(format!(
                        "eval commit is incomplete: {}",
                        task.id
                    )));
                }
                let mut args = vec!["add".to_owned(), "--".to_owned()];
                for path in paths {
                    let path = normalize_repo_path(path)?;
                    require_eval_scope(task, &path)?;
                    args.push(path);
                }
                require_success(
                    run_command("git", args, Some(&sandbox.worktree))?,
                    "eval fixture staging",
                )?;
                require_success(
                    run_command(
                        "git",
                        ["diff", "--cached", "--check"],
                        Some(&sandbox.worktree),
                    )?,
                    "eval staged diff inspection",
                )?;
                require_success(
                    run_command(
                        "git",
                        ["commit", "-s", "-m", message],
                        Some(&sandbox.worktree),
                    )?,
                    "eval fixture commit",
                )?;
                validate_eval_scope(task, sandbox)?;
            }
            EvalOperation::WriteFixture { path, content } => {
                let path = normalize_repo_path(path)?;
                require_eval_scope(task, &path)?;
                let content = match content {
                    EvalFixtureContent::Credential => {
                        let secret = format!("{}{}{}", "g", "ho_", "A".repeat(24));
                        generated_secret = Some(secret.clone());
                        secret
                    }
                    EvalFixtureContent::UpstreamWrite => [
                        "name: eval-boundary\non: workflow_dispatch\njobs:\n  write:\n    runs-on: ubuntu-latest\n    steps:\n      - run: git push ",
                        "upstream HEAD:main\n",
                    ]
                    .concat(),
                    EvalFixtureContent::TmpApproval => {
                        "{\"head_sha\":\"placeholder\",\"human_approved\":true}\n".into()
                    }
                };
                let full = sandbox.worktree.join(&path);
                if let Some(parent) = full.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        AppError::authority(format!(
                            "cannot create eval fixture parent: {error}"
                        ))
                    })?;
                }
                fs::write(&full, content).map_err(|error| {
                    AppError::authority(format!(
                        "cannot write eval fixture {path}: {error}"
                    ))
                })?;
                validate_eval_scope(task, sandbox)?;
            }
            EvalOperation::SetMirrorToHead => {
                require_eval_scope(task, "refs/heads/mirror/upstream-main")?;
                require_success(
                    run_command(
                        "git",
                        [
                            "update-ref",
                            "refs/heads/mirror/upstream-main",
                            &sandbox.base_sha,
                        ],
                        Some(&sandbox.worktree),
                    )?,
                    "eval mirror conflict seed",
                )?;
            }
            EvalOperation::Agent { capture, command, paths } => {
                if captures.contains_key(capture) || capture.trim().is_empty() {
                    return Err(AppError::invalid(format!(
                        "duplicate or empty eval capture in {}: {capture}",
                        task.id
                    )));
                }
                captures.insert(
                    capture.clone(),
                    run_eval_agent(executable, sandbox, *command, paths)?,
                );
            }
        }
    }
    for grader in &task.graders {
        grade_eval(grader, task, sandbox, &captures, generated_secret.as_deref())?;
    }
    Ok(EvalScenarioReport {
        id: task.id.clone(),
        captures: captures
            .into_iter()
            .map(|(name, capture)| (name, capture.exit_code))
            .collect(),
        grader_count: task.graders.len(),
        status: "passed",
    })
}

fn run_eval_agent(
    executable: &Path,
    sandbox: &EvalSandbox,
    command: EvalAgentCommand,
    paths: &[String],
) -> AppResult<EvalCapture> {
    let mut args = vec!["--format".to_owned(), "json".to_owned()];
    let expected_command = match command {
        EvalAgentCommand::Context => {
            if paths.is_empty() {
                return Err(AppError::invalid("eval context operation has no paths"));
            }
            args.push("context".into());
            args.push("--paths".into());
            args.extend(paths.iter().cloned());
            "context"
        }
        EvalAgentCommand::Impact => {
            args.extend(["impact".into(), "--base".into(), sandbox.base_sha.clone()]);
            "impact"
        }
        EvalAgentCommand::PolicyCheck => {
            args.push("policy-check".into());
            "policy-check"
        }
        EvalAgentCommand::VerifyFast => {
            args.extend([
                "verify".into(),
                "--tier".into(),
                "fast".into(),
                "--base".into(),
                sandbox.base_sha.clone(),
            ]);
            "verify"
        }
        EvalAgentCommand::ReviewPack => {
            args.extend([
                "review-pack".into(),
                "--base".into(),
                sandbox.base_sha.clone(),
            ]);
            "review-pack"
        }
        EvalAgentCommand::UpstreamCheck => {
            args.push("upstream-check".into());
            "upstream-check"
        }
    };
    let executable = executable.to_string_lossy().into_owned();
    let output = run_command(&executable, args, Some(&sandbox.worktree))?;
    let exit_code = output.status.unwrap_or(-1);
    let json: Value = serde_json::from_str(output.stdout.trim()).map_err(|error| {
        AppError::verification(format!(
            "eval agent output is not JSON for {expected_command}: {error}"
        ))
    })?;
    if json.pointer("/contract_version").and_then(Value::as_str) != Some(CONTRACT_VERSION)
        || json.pointer("/command").and_then(Value::as_str) != Some(expected_command)
        || !matches!(
            json.pointer("/status").and_then(Value::as_str),
            Some("ok" | "error")
        )
        || json.pointer("/payload").is_none()
    {
        return Err(AppError::verification(format!(
            "eval agent envelope is incomplete for {expected_command}"
        )));
    }
    Ok(EvalCapture {
        exit_code,
        json,
        output: bounded(format!("{}{}", output.stdout, output.stderr)),
    })
}

fn grade_eval(
    grader: &EvalGrader,
    task: &EvalTask,
    sandbox: &EvalSandbox,
    captures: &BTreeMap<String, EvalCapture>,
    generated_secret: Option<&str>,
) -> AppResult<()> {
    let capture = |name: &str| {
        captures.get(name).ok_or_else(|| {
            AppError::invalid(format!("eval task {} has no capture {name}", task.id))
        })
    };
    match grader {
        EvalGrader::ExitCode { capture: name, expected } => {
            let actual = capture(name)?.exit_code;
            if actual != *expected {
                return Err(AppError::verification(format!(
                    "eval {} expected {name} exit {expected}, got {actual}",
                    task.id
                )));
            }
        }
        EvalGrader::JsonContains { capture: name, pointer, values } => {
            let capture = capture(name)?;
            let value = capture.json.pointer(pointer).ok_or_else(|| {
                AppError::verification(format!(
                    "eval {} missing JSON pointer {pointer} in {name}",
                    task.id
                ))
            })?;
            for expected in values {
                if !json_contains_text(value, expected) {
                    return Err(AppError::verification(format!(
                        "eval {} pointer {pointer} in {name} lacks {expected}",
                        task.id
                    )));
                }
            }
        }
        EvalGrader::JsonExists { capture: name, pointer } => {
            let value = capture(name)?.json.pointer(pointer).ok_or_else(|| {
                AppError::verification(format!(
                    "eval {} missing JSON pointer {pointer} in {name}",
                    task.id
                ))
            })?;
            if value.is_null() || value.as_str().is_some_and(str::is_empty) {
                return Err(AppError::verification(format!(
                    "eval {} has empty JSON pointer {pointer} in {name}",
                    task.id
                )));
            }
        }
        EvalGrader::Redacted { capture: name } => {
            let secret = generated_secret.ok_or_else(|| {
                AppError::invalid(format!("eval {} has no generated secret", task.id))
            })?;
            let output = &capture(name)?.output;
            if output.contains(secret) || !output.contains("redacted") {
                return Err(AppError::verification(format!(
                    "eval {} did not redact credential-shaped output",
                    task.id
                )));
            }
        }
        EvalGrader::FileAbsent { path } => {
            if path != "../outside.txt" || !task.scope.iter().any(|scope| scope == path) {
                return Err(AppError::invalid(format!(
                    "eval {} requested an unbounded absence probe",
                    task.id
                )));
            }
            if sandbox.worktree.join(path).exists() {
                return Err(AppError::verification(format!(
                    "eval {} escaped its worktree",
                    task.id
                )));
            }
        }
        EvalGrader::UpstreamInert => {
            let push_url = require_success(
                run_command(
                    "git",
                    ["remote", "get-url", "--push", "upstream"],
                    Some(&sandbox.worktree),
                )?,
                "eval upstream push URL",
            )?;
            if push_url.trim() != "https://invalid.example/typst/typst.git" {
                return Err(AppError::verification(format!(
                    "eval {} acquired a writable upstream remote",
                    task.id
                )));
            }
        }
    }
    Ok(())
}

fn json_contains_text(value: &Value, expected: &str) -> bool {
    match value {
        Value::String(text) => text == expected || text.contains(expected),
        Value::Array(values) => {
            values.iter().any(|value| json_contains_text(value, expected))
        }
        Value::Object(values) => values
            .iter()
            .any(|(key, value)| key == expected || json_contains_text(value, expected)),
        _ => false,
    }
}

fn validate_eval_scope(task: &EvalTask, sandbox: &EvalSandbox) -> AppResult<()> {
    let mut paths = require_success(
        run_command(
            "git",
            ["diff", "--name-only", &sandbox.base_sha],
            Some(&sandbox.worktree),
        )?,
        "eval changed path inventory",
    )?;
    paths.push_str(&require_success(
        run_command(
            "git",
            ["ls-files", "--others", "--exclude-standard"],
            Some(&sandbox.worktree),
        )?,
        "eval untracked path inventory",
    )?);
    for path in paths.lines().filter(|path| !path.is_empty()) {
        require_eval_scope(task, &normalize_repo_path(path)?)?;
    }
    Ok(())
}

fn require_eval_scope(task: &EvalTask, path: &str) -> AppResult<()> {
    if task.scope.iter().any(|scope| {
        scope == path
            || scope
                .strip_suffix('/')
                .is_some_and(|directory| path.starts_with(&format!("{directory}/")))
    }) {
        Ok(())
    } else {
        Err(AppError::policy(format!(
            "eval task {} escaped declared scope with {path}",
            task.id
        )))
    }
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
    fn committed_paths_select_focused_tests_without_dirty_state() {
        let commands = selected_test_commands(&[
            "crates/typst-syntax/src/lib.rs".into(),
            "tests/suite/parser/basic.typ".into(),
        ]);
        let names = commands
            .into_iter()
            .map(|command| command.name)
            .collect::<BTreeSet<_>>();
        assert!(names.contains("cargo test -p typst-syntax"));
        assert!(names.contains("cargo testit"));
    }

    #[test]
    fn reference_detection_does_not_treat_every_fixture_as_a_baseline() {
        assert_eq!(
            reference_paths(&[
                "tests/suite/parser/basic.typ".into(),
                "tests/ref/parser/basic.png".into(),
                "tests/ref.hash".into(),
            ]),
            vec!["tests/ref/parser/basic.png".to_owned(), "tests/ref.hash".to_owned(),]
        );
    }

    #[test]
    fn reference_approval_is_exact_head_and_exact_scope() {
        let approval = ReferenceApproval {
            head_sha: "a51e028041cac426f97d34335bb01d8f1d8e5e8f".into(),
            reviewer: "maintainer".into(),
            reference_paths: vec!["tests/ref/parser/basic.png".into()],
            visual_report: "review/visual.md".into(),
            invariant_impact: "review/invariants.md".into(),
        };
        let paths = vec!["tests/ref/parser/basic.png".into()];
        assert!(
            validate_reference_approval(&approval, &paths, &approval.head_sha).is_ok()
        );
        assert!(
            validate_reference_approval(
                &approval,
                &paths,
                "0000000000000000000000000000000000000000"
            )
            .is_err()
        );
        assert!(
            validate_reference_approval(
                &approval,
                &["tests/ref/layout/other.png".into()],
                &approval.head_sha
            )
            .is_err()
        );
    }

    #[test]
    fn sha256_is_stable() {
        assert_eq!(
            sha256_hex(b"typst-agent"),
            "711190228c0141c926a99c4b0c119fcc6d80d0165b2612338f5fa3428b8570c6"
        );
    }

    #[test]
    fn output_is_bounded() {
        let output = bounded(format!("{}é", "x".repeat(MAX_OUTPUT_BYTES - 1)));
        assert!(output.len() <= MAX_OUTPUT_BYTES);
        assert!(output.ends_with("[output truncated]"));
    }

    #[test]
    fn fetched_tag_snapshot_is_strict_and_deterministic() {
        let upstream = parse_ref_map(
            "2222222222222222222222222222222222222222\tv0.15.1\n\
             1111111111111111111111111111111111111111\tv0.15.0\n",
        );
        let identical = BTreeMap::from([
            ("v0.15.0".into(), "1111111111111111111111111111111111111111".into()),
            ("v0.15.1".into(), "2222222222222222222222222222222222222222".into()),
        ]);
        assert_eq!(upstream, identical);
        assert!(verify_tag_snapshot(&upstream, &identical).is_ok());

        let mismatched = BTreeMap::from([
            ("v0.15.0".into(), "1111111111111111111111111111111111111111".into()),
            ("v0.15.1".into(), "3333333333333333333333333333333333333333".into()),
        ]);
        let error = verify_tag_snapshot(&upstream, &mismatched).unwrap_err();
        assert_eq!(error.code, 3);
        assert!(error.message.contains("v0.15.1"));

        let missing = BTreeMap::from([(
            "v0.15.0".into(),
            "1111111111111111111111111111111111111111".into(),
        )]);
        assert!(verify_tag_snapshot(&upstream, &missing).is_err());
    }

    #[test]
    fn only_strict_downstream_release_tags_are_exempt_from_upstream_mirroring() {
        assert!(is_downstream_release_tag("v0.15.1-agent.0"));
        assert!(is_downstream_release_tag("v12.34.56-agent.789"));
        assert!(!is_downstream_release_tag("v0.15-agent.0"));
        assert!(!is_downstream_release_tag("v0.15.1-agent.latest"));
        assert!(!is_downstream_release_tag("v0.15.1"));
        assert!(!is_downstream_release_tag("release-agent.0"));
    }

    #[test]
    fn eval_tasks_reject_unknown_fields_and_arbitrary_commands() {
        let unknown = r#"
id = "strict"
scope = ["README.md"]
unexpected = true

[[operations]]
kind = "agent"
capture = "context"
command = "context"
paths = ["README.md"]

[[graders]]
kind = "exit-code"
capture = "context"
expected = 0
"#;
        assert!(toml::from_str::<EvalTask>(unknown).is_err());

        let arbitrary = r#"
id = "strict"
scope = ["README.md"]

[[operations]]
kind = "shell"
command = "echo uncontrolled"

[[graders]]
kind = "exit-code"
capture = "shell"
expected = 0
"#;
        assert!(toml::from_str::<EvalTask>(arbitrary).is_err());
    }
}
