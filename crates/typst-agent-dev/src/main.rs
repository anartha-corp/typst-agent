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
use std::io::Read;
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
    /// Emit a complete source, artifact, and release identity manifest.
    ReleaseManifest(ReleaseManifestArgs),
    /// Score the mined upstream golden backlog from a frozen snapshot.
    Backlog(BacklogArgs),
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

#[derive(Debug, Args)]
struct ReleaseManifestArgs {
    /// Strict preparation evidence produced by the release workflow.
    #[arg(long, default_value = ".tmp/agent/release/release-input.json")]
    input: PathBuf,
}

#[derive(Debug, Args)]
struct BacklogArgs {
    /// Snapshot directory produced by scripts/backlog-fetch.sh.
    #[arg(long, default_value = ".tmp/agent/backlog/raw")]
    snapshot: PathBuf,
    /// Annotated candidate registry.
    #[arg(long, default_value = ".agents/backlog/registry.toml")]
    registry: PathBuf,
    /// Run only the deterministic scoring self-check.
    #[arg(long)]
    self_check: bool,
    /// Build an investigation pack for one upstream issue instead of scoring.
    #[arg(long)]
    investigate: Option<u64>,
    /// Cross-check registry lifecycles against downstream git history.
    #[arg(long)]
    audit: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum VerifyTier {
    Fast,
    Pr,
    Full,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum ReleaseArtifactKind {
    Binary,
    CompilerImage,
    DevImage,
    Agentctl,
    Documentation,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseArtifactInput {
    path: PathBuf,
    kind: ReleaseArtifactKind,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseSmokeInput {
    subject: String,
    platform: String,
    evidence_path: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReproducibilityEvidence {
    target: String,
    first_sha256: String,
    second_sha256: String,
    identical: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseManifestInput {
    release_tag: String,
    artifacts: Vec<ReleaseArtifactInput>,
    sbom_path: PathBuf,
    sigstore_bundle_paths: Vec<PathBuf>,
    provenance_attestation_paths: Vec<PathBuf>,
    reproducibility: Vec<ReproducibilityEvidence>,
    smoke_results: Vec<ReleaseSmokeInput>,
}

#[derive(Debug, Serialize)]
struct ReleaseFileEvidence {
    name: String,
    sha256: String,
    size: u64,
}

#[derive(Debug, Serialize)]
struct ReleaseArtifact {
    name: String,
    kind: ReleaseArtifactKind,
    sha256: String,
    size: u64,
}

#[derive(Debug, Serialize)]
struct ReleaseSmokeResult {
    subject: String,
    platform: String,
    evidence: ReleaseFileEvidence,
    passed: bool,
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
    Backlog,
    BacklogInvestigate,
    BacklogAudit,
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
        CommandKind::ReleaseManifest(args) => {
            Ok(("release-manifest", release_manifest(&args.input)?))
        }
        CommandKind::Backlog(args) => Ok(("backlog", backlog(args)?)),
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
        CommandKind::ReleaseManifest(_) => "release-manifest",
        CommandKind::Backlog(_) => "backlog",
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
        "BacklogRecord",
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
            "contract schema must define exactly the nine v1 record kinds",
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
        ".agents/backlog/registry.toml",
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

// ---- golden backlog scoring -------------------------------------------------

#[derive(Debug, Deserialize)]
struct SnapshotProvenance {
    #[serde(default)]
    snapshot_date: String,
    #[serde(default)]
    upstream_sha: String,
}

#[derive(Debug, Deserialize)]
struct SnapshotIssue {
    number: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    reactions: u64,
    #[serde(default)]
    comments: u64,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotComment {
    #[serde(default)]
    author: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    body: String,
}

#[derive(Debug, Deserialize)]
struct SnapshotCrossref {
    #[serde(default)]
    references: Vec<u64>,
    #[serde(default)]
    closed_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SnapshotPull {
    number: u64,
    #[serde(default)]
    linked_issues: Vec<u64>,
    #[serde(default)]
    updated_at: String,
}

#[derive(Debug, Deserialize)]
struct SnapshotNotPlanned {
    number: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryFile {
    version: u32,
    calibration: Calibration,
    #[serde(default)]
    issues: Vec<RegistryIssue>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Calibration {
    #[serde(default)]
    reference_mines: Vec<u64>,
    #[serde(default)]
    known_bad: Vec<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryIssue {
    number: u64,
    title: String,
    status: String,
    #[serde(default)]
    stance: String,
    #[serde(default)]
    subsystem: String,
    confidence: u8,
    safety: u8,
    impact: u8,
    burden: u8,
    #[serde(default)]
    note: String,
    #[serde(default)]
    exclude_reason: String,
    #[serde(default)]
    human_override: String,
    #[serde(default)]
    curated_at: String,
    #[serde(default)]
    downstream_pr: Option<u64>,
    #[serde(default)]
    shipped_sha: Option<String>,
    #[serde(default)]
    upstream_equivalent: String,
}

#[derive(Debug, Serialize)]
struct BacklogEntry {
    #[serde(rename = "ref")]
    reference: String,
    number: u64,
    title: String,
    status: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    stance: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    subsystem: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    state: String,
    tier: &'static str,
    demand: u8,
    confidence: u8,
    safety: u8,
    impact: u8,
    burden: u8,
    score: u64,
    #[serde(skip_serializing_if = "String::is_empty")]
    curated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    downstream_pr: Option<u64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    upstream_equivalent: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    note: String,
}

#[derive(Debug, Serialize)]
struct BacklogExclusion {
    #[serde(rename = "ref")]
    reference: String,
    number: u64,
    title: String,
    reason: String,
}

#[derive(Debug, Serialize)]
struct BacklogRecord {
    snapshot_date: String,
    upstream_sha: String,
    scored: usize,
    excluded: usize,
    unscored: Vec<String>,
    upstream_closed: Vec<String>,
    stale: Vec<String>,
    tier_a: Vec<BacklogEntry>,
    tier_b: Vec<BacklogEntry>,
    tier_c: Vec<BacklogEntry>,
    excluded_entries: Vec<BacklogExclusion>,
}

#[derive(Debug, Serialize)]
struct BacklogSelfCheck {
    status: &'static str,
    snapshot_date: String,
    upstream_sha: String,
    scored: usize,
    excluded: usize,
    unscored: usize,
    upstream_closed: usize,
    stale: usize,
    tier_a: usize,
    tier_b: usize,
    tier_c: usize,
    reference_mines: Vec<String>,
    known_bad: Vec<String>,
}

#[derive(Debug, Serialize)]
struct InvestigateReport {
    #[serde(rename = "ref")]
    reference: String,
    number: u64,
    title: String,
    state: String,
    labels: Vec<String>,
    created_at: String,
    updated_at: String,
    demand: u8,
    registry_status: Option<String>,
    registry_stance: Option<String>,
    registry_subsystem: Option<String>,
    registry_note: Option<String>,
    maintainer_comments: Vec<SnapshotComment>,
    comments: Vec<SnapshotComment>,
    crossrefs: Vec<u64>,
    crossref_titles: Vec<String>,
    subsystem_notes: Vec<String>,
    area_guide: String,
}

#[derive(Debug, Serialize)]
struct AuditEntry {
    #[serde(rename = "ref")]
    reference: String,
    number: u64,
    status: String,
    ok: bool,
    detail: String,
}

#[derive(Debug, Serialize)]
struct AuditReport {
    status: &'static str,
    checked: usize,
    violations: Vec<AuditEntry>,
}

const VALID_STANCES: [&str; 5] = ["endorsing", "neutral", "skeptical", "planned", "none"];

fn demand_grade(reactions: u64, comments: u64) -> u8 {
    if reactions >= 100 || comments >= 30 {
        5
    } else if reactions >= 40 || comments >= 20 {
        4
    } else if reactions >= 15 || comments >= 10 {
        3
    } else if reactions >= 5 || comments >= 4 {
        2
    } else {
        1
    }
}

fn backlog_score(demand: u8, confidence: u8, safety: u8, impact: u8, burden: u8) -> u64 {
    (u64::from(demand) * u64::from(confidence) * u64::from(safety) * u64::from(impact))
        / u64::from(burden)
}

fn backlog_tier(score: u64) -> &'static str {
    if score >= 120 {
        "a"
    } else if score >= 48 {
        "b"
    } else {
        "c"
    }
}

fn ymd(text: &str) -> Option<(i64, u32, u32)> {
    let mut parts = text.split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

fn days_since(earlier: &str, later: &str) -> Option<i64> {
    let (early_year, early_month, early_day) = ymd(earlier)?;
    let (late_year, late_month, late_day) = ymd(later)?;
    let ordinal = |year: i64, month: u32, day: u32| -> i64 {
        let mut total =
            year * 365 + year.div_euclid(4) - year.div_euclid(100) + year.div_euclid(400);
        let cumulative = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
        total += i64::from(cumulative[(month - 1) as usize]) + i64::from(day);
        if month > 2 && year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
            total += 1;
        }
        total
    };
    Some(
        ordinal(late_year, late_month, late_day)
            - ordinal(early_year, early_month, early_day),
    )
}

fn backlog(args: &BacklogArgs) -> AppResult<Value> {
    let repo = root()?;
    let snapshot_dir = repo.join(&args.snapshot);
    let registry_path = repo.join(&args.registry);

    let read_json = |name: &str| -> AppResult<Value> {
        let path = snapshot_dir.join(name);
        let text = fs::read_to_string(&path).map_err(|error| {
            AppError::invalid(format!(
                "cannot read backlog snapshot {}: {} (run scripts/backlog-fetch.sh first)",
                path.display(),
                error
            ))
        })?;
        serde_json::from_str(&text).map_err(|error| {
            AppError::invalid(format!("backlog snapshot {name} is not JSON: {error}"))
        })
    };
    let parse = |name: &str| -> AppResult<Value> { read_json(name) };

    let provenance: SnapshotProvenance =
        serde_json::from_value(parse("provenance.json")?).map_err(|error| {
            AppError::invalid(format!("invalid backlog provenance: {error}"))
        })?;
    let issues: Vec<SnapshotIssue> = serde_json::from_value(parse("issues.json")?)
        .map_err(|error| AppError::invalid(format!("invalid backlog issues: {error}")))?;
    let pulls: Vec<SnapshotPull> = serde_json::from_value(parse("pulls.json")?)
        .map_err(|error| AppError::invalid(format!("invalid backlog pulls: {error}")))?;
    let not_planned: Vec<SnapshotNotPlanned> =
        serde_json::from_value(parse("closed-not-planned.json")?).map_err(|error| {
            AppError::invalid(format!("invalid backlog not-planned: {error}"))
        })?;

    let registry: RegistryFile = toml::from_str(&read_semantic_text(&registry_path)?)
        .map_err(|error| {
            AppError::invalid(format!("invalid backlog registry: {error}"))
        })?;
    if registry.version != 1 {
        return Err(AppError::invalid("backlog registry must have version 1"));
    }

    let demand_by_number = issues
        .iter()
        .map(|issue| (issue.number, issue))
        .collect::<BTreeMap<_, _>>();
    let not_planned_set =
        not_planned.iter().map(|entry| entry.number).collect::<BTreeSet<_>>();

    if let Some(number) = args.investigate {
        return backlog_investigate(
            &repo,
            &snapshot_dir,
            &registry,
            &demand_by_number,
            number,
        );
    }
    if args.audit {
        return backlog_audit(&repo, &registry);
    }

    let mut seen = BTreeSet::new();
    let mut tier_a = Vec::new();
    let mut tier_b = Vec::new();
    let mut tier_c = Vec::new();
    let mut excluded = Vec::new();
    let mut unscored = Vec::new();
    let mut upstream_closed = Vec::new();
    let mut stale = Vec::new();
    let mut scores = BTreeMap::new();

    for issue in &registry.issues {
        if !seen.insert(issue.number) {
            return Err(AppError::invalid(format!(
                "backlog registry repeats issue {}",
                issue.number
            )));
        }
        if issue.title.trim().is_empty() || issue.status.trim().is_empty() {
            return Err(AppError::invalid(format!(
                "backlog registry issue {} has an empty title or status",
                issue.number
            )));
        }
        if !issue.stance.is_empty() && !VALID_STANCES.contains(&issue.stance.as_str()) {
            return Err(AppError::invalid(format!(
                "backlog registry issue {} has an unknown stance {:?}",
                issue.number, issue.stance
            )));
        }
        for (name, factor) in [
            ("confidence", issue.confidence),
            ("safety", issue.safety),
            ("impact", issue.impact),
            ("burden", issue.burden),
        ] {
            if !(1..=5).contains(&factor) {
                return Err(AppError::invalid(format!(
                    "backlog registry issue {} has {name} {factor} outside 1..=5",
                    issue.number
                )));
            }
        }
        let reference = format!("#{}", issue.number);
        let Some(snapshot_issue) = demand_by_number.get(&issue.number) else {
            unscored.push(reference);
            continue;
        };
        let demand = demand_grade(snapshot_issue.reactions, snapshot_issue.comments);

        let mut reason = String::new();
        if issue.human_override.is_empty() {
            if !issue.exclude_reason.is_empty() {
                reason = issue.exclude_reason.clone();
            } else if not_planned_set.contains(&issue.number) {
                reason = "not-planned".into();
            } else if let Some(pull) = pulls.iter().find(|pull| {
                pull.linked_issues.contains(&issue.number)
                    && days_since(&pull.updated_at, &provenance.snapshot_date)
                        .is_none_or(|days| days <= 180)
            }) {
                reason = format!("upstream-pr-active:#{}", pull.number);
            }
        }
        if !reason.is_empty() {
            excluded.push(BacklogExclusion {
                reference,
                number: issue.number,
                title: issue.title.clone(),
                reason,
            });
            continue;
        }
        if snapshot_issue.state == "closed" {
            upstream_closed.push(reference);
            continue;
        }
        if !issue.curated_at.is_empty()
            && days_since(&issue.curated_at, &provenance.snapshot_date)
                .is_none_or(|days| days > 28)
        {
            stale.push(reference.clone());
        }

        let score = backlog_score(
            demand,
            issue.confidence,
            issue.safety,
            issue.impact,
            issue.burden,
        );
        let tier = backlog_tier(score);
        let entry = BacklogEntry {
            reference,
            number: issue.number,
            title: issue.title.clone(),
            status: issue.status.clone(),
            stance: issue.stance.clone(),
            subsystem: issue.subsystem.clone(),
            state: snapshot_issue.state.clone(),
            tier,
            demand,
            confidence: issue.confidence,
            safety: issue.safety,
            impact: issue.impact,
            burden: issue.burden,
            score,
            curated_at: issue.curated_at.clone(),
            downstream_pr: issue.downstream_pr,
            upstream_equivalent: issue.upstream_equivalent.clone(),
            note: issue.note.clone(),
        };
        scores.insert(issue.number, tier);
        match tier {
            "a" => tier_a.push(entry),
            "b" => tier_b.push(entry),
            _ => tier_c.push(entry),
        }
    }

    // Calibration: reference mines must stay mineable, known-bad must stay excluded.
    let excluded_numbers =
        excluded.iter().map(|entry| entry.number).collect::<BTreeSet<_>>();
    for reference in &registry.calibration.reference_mines {
        match scores.get(reference).copied() {
            Some("a" | "b") => {}
            _ => {
                return Err(AppError::verification(format!(
                    "backlog calibration reference #{reference} is not in tier a/b"
                )));
            }
        }
    }
    for bad in &registry.calibration.known_bad {
        if !excluded_numbers.contains(bad) {
            return Err(AppError::verification(format!(
                "backlog calibration known-bad #{bad} is not excluded"
            )));
        }
    }

    tier_a.sort_by(|left, right| {
        right.score.cmp(&left.score).then(left.number.cmp(&right.number))
    });
    tier_b.sort_by(|left, right| {
        right.score.cmp(&left.score).then(left.number.cmp(&right.number))
    });
    tier_c.sort_by(|left, right| {
        right.score.cmp(&left.score).then(left.number.cmp(&right.number))
    });
    excluded.sort_by_key(|entry| entry.number);
    tier_a.truncate(20);
    tier_b.truncate(40);
    tier_c.truncate(40);
    excluded.truncate(80);

    let reference_mines = registry
        .calibration
        .reference_mines
        .iter()
        .map(|number| format!("#{number}"))
        .collect();
    let known_bad = registry
        .calibration
        .known_bad
        .iter()
        .map(|number| format!("#{number}"))
        .collect();

    if args.self_check {
        return json_value(BacklogSelfCheck {
            status: "passed",
            snapshot_date: provenance.snapshot_date,
            upstream_sha: provenance.upstream_sha,
            scored: tier_a.len() + tier_b.len() + tier_c.len(),
            excluded: excluded.len(),
            unscored: unscored.len(),
            upstream_closed: upstream_closed.len(),
            stale: stale.len(),
            tier_a: tier_a.len(),
            tier_b: tier_b.len(),
            tier_c: tier_c.len(),
            reference_mines,
            known_bad,
        });
    }

    let record = BacklogRecord {
        snapshot_date: provenance.snapshot_date,
        upstream_sha: provenance.upstream_sha,
        scored: tier_a.len() + tier_b.len() + tier_c.len(),
        excluded: excluded.len(),
        unscored,
        upstream_closed,
        stale,
        tier_a,
        tier_b,
        tier_c,
        excluded_entries: excluded,
    };
    let pack = json!({
        "contract_version": CONTRACT_VERSION,
        "kind": "BacklogRecord",
        "payload": record,
    });
    let directory = repo.join(".tmp/agent");
    fs::create_dir_all(&directory)
        .map_err(|error| AppError::authority(error.to_string()))?;
    let path = directory.join("backlog.json");
    let bytes = serde_json::to_vec_pretty(&pack)
        .map_err(|error| AppError::invalid(error.to_string()))?;
    if bytes.len() as u64 > MAX_SEMANTIC_EVIDENCE_BYTES {
        return Err(AppError::authority(
            "backlog evidence exceeds the semantic evidence limit",
        ));
    }
    fs::write(&path, bytes).map_err(|error| AppError::authority(error.to_string()))?;
    Ok(json!({"path": ".tmp/agent/backlog.json", "record": pack}))
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

/// Build a deterministic investigation pack for one upstream issue.
///
/// The pack assembles snapshot data (issue meta, comments, maintainer
/// comments, cross-references), the registry entry if present, and the
/// knowledge left by earlier mines in the same subsystem. It is input for an
/// LLM or a human curator; the resulting annotation proposal must land in the
/// registry through a reviewed PR, after which `cargo agent backlog
/// --self-check` validates it. The scorer itself stays model-free.
fn backlog_investigate(
    repo: &Path,
    snapshot_dir: &Path,
    registry: &RegistryFile,
    demand_by_number: &BTreeMap<u64, &SnapshotIssue>,
    number: u64,
) -> AppResult<Value> {
    let snapshot_issue = demand_by_number.get(&number).copied().ok_or_else(|| {
        AppError::invalid(format!(
            "issue #{number} is not in the snapshot (run scripts/backlog-fetch.sh first)"
        ))
    })?;
    let registry_issue = registry.issues.iter().find(|issue| issue.number == number);

    let read_comments = |name: &str| -> AppResult<Vec<SnapshotComment>> {
        let path = snapshot_dir.join(name);
        let Ok(text) = fs::read_to_string(&path) else {
            return Ok(Vec::new());
        };
        serde_json::from_str(&text)
            .map_err(|error| AppError::invalid(format!("invalid {name}: {error}")))
    };
    let comments = read_comments(&format!("comments/{number}.json"))?;

    let maintainers: Vec<String> = {
        let path = snapshot_dir.join("maintainers.json");
        match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).map_err(|error| {
                AppError::invalid(format!("invalid maintainers.json: {error}"))
            })?,
            Err(_) => Vec::new(),
        }
    };
    let maintainer_comments = comments
        .iter()
        .filter(|comment| maintainers.iter().any(|login| &comment.author == login))
        .cloned()
        .collect::<Vec<_>>();

    let crossrefs: SnapshotCrossref = {
        let path = snapshot_dir.join(format!("crossrefs/{number}.json"));
        match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).map_err(|error| {
                AppError::invalid(format!("invalid crossrefs/{number}.json: {error}"))
            })?,
            Err(_) => SnapshotCrossref { references: Vec::new(), closed_reason: None },
        }
    };
    let crossref_titles = crossrefs
        .references
        .iter()
        .filter_map(|reference| {
            demand_by_number
                .get(reference)
                .map(|issue| format!("#{reference} {}", truncate_title(&issue.title)))
        })
        .collect();

    let subsystem = registry_issue
        .map(|issue| issue.subsystem.as_str())
        .filter(|subsystem| !subsystem.is_empty())
        .unwrap_or("unknown");
    let subsystem_notes = registry
        .issues
        .iter()
        .filter(|issue| issue.number != number && issue.subsystem == subsystem)
        .filter_map(|issue| {
            if issue.note.trim().is_empty() {
                None
            } else {
                Some(format!("#{} ({}): {}", issue.number, issue.status, issue.note))
            }
        })
        .collect();
    let area_guide = match subsystem {
        "layout" | "styling" => ".agents/areas/layout.md",
        "cli" => ".agents/areas/cli.md",
        "pdf" | "visualize" => ".agents/areas/output.md",
        "devops" => ".agents/areas/release.md",
        _ => ".agents/areas/evaluation.md",
    }
    .to_owned();

    let report = InvestigateReport {
        reference: format!("#{number}"),
        number,
        title: registry_issue
            .map(|issue| issue.title.clone())
            .or_else(|| {
                (!snapshot_issue.title.is_empty()).then(|| snapshot_issue.title.clone())
            })
            .unwrap_or_else(|| format!("untracked issue #{number}")),
        state: snapshot_issue.state.clone(),
        labels: snapshot_issue.labels.clone(),
        created_at: snapshot_issue.created_at.clone(),
        updated_at: snapshot_issue.updated_at.clone(),
        demand: demand_grade(snapshot_issue.reactions, snapshot_issue.comments),
        registry_status: registry_issue.map(|issue| issue.status.clone()),
        registry_stance: registry_issue.map(|issue| issue.stance.clone()),
        registry_subsystem: registry_issue.map(|issue| issue.subsystem.clone()),
        registry_note: registry_issue.map(|issue| issue.note.clone()),
        maintainer_comments,
        comments,
        crossrefs: crossrefs.references,
        crossref_titles,
        subsystem_notes,
        area_guide,
    };

    let directory = repo.join(".tmp/agent/backlog");
    fs::create_dir_all(&directory)
        .map_err(|error| AppError::authority(error.to_string()))?;
    let path = directory.join(format!("investigate-{number}.json"));
    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|error| AppError::invalid(error.to_string()))?;
    fs::write(&path, bytes).map_err(|error| AppError::authority(error.to_string()))?;

    let template = investigation_template(&report);
    let template_path = directory.join(format!("investigate-{number}.md"));
    fs::write(&template_path, template)
        .map_err(|error| AppError::authority(error.to_string()))?;

    Ok(json_value(report)?)
}

fn truncate_title(title: &str) -> String {
    let mut truncated = title.chars().take(60).collect::<String>();
    if title.chars().count() > 60 {
        truncated.push('…');
    }
    truncated
}

fn investigation_template(report: &InvestigateReport) -> String {
    let mut template = format!(
        "# Investigation pack: {}\n\n\
         - state: {}\n\
         - labels: {}\n\
         - created: {} | updated: {}\n\
         - demand grade: {}\n\
         - registry: status={} stance={} subsystem={}\n\
         - cross-references: {}\n\
         - area guide: {}\n\n",
        report.reference,
        if report.state.is_empty() { "unknown" } else { &report.state },
        report.labels.join(", "),
        report.created_at,
        report.updated_at,
        report.demand,
        report.registry_status.as_deref().unwrap_or("uncurated"),
        report.registry_stance.as_deref().unwrap_or("none"),
        report.registry_subsystem.as_deref().unwrap_or("unknown"),
        report
            .crossref_titles
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        report.area_guide,
    );
    if !report.maintainer_comments.is_empty() {
        template.push_str("## Maintainer comments\n\n");
        for comment in &report.maintainer_comments {
            template.push_str(&format!(
                "- {} ({}): {}\n",
                comment.author, comment.created_at, comment.body
            ));
        }
        template.push('\n');
    }
    if !report.subsystem_notes.is_empty() {
        template.push_str("## Earlier mines in this subsystem\n\n");
        for note in &report.subsystem_notes {
            template.push_str(&format!("- {note}\n"));
        }
        template.push('\n');
    }
    template.push_str(
        "## Annotation proposal (fill in, then PR to .agents/backlog/registry.toml)\n\n\
         ```toml\n\
         status = \"candidate\"  # candidate|mined|shipped|watch|excluded|upstream-shipped\n\
         stance = \"none\"       # endorsing|neutral|skeptical|planned|none\n\
         subsystem = \"\"\n\
         confidence = 0         # 1..=5\n\
         safety = 0             # 1..=5\n\
         impact = 0             # 1..=5\n\
         burden = 0             # 1..=5\n\
         note = \"\"\n\
         exclude_reason = \"\"   # only for hard exclusions\n\
         curated_at = \"YYYY-MM-DD\"\n\
         upstream_equivalent = \"\"\n\
         ```\n\n\
         Checklist: API shape, drop-when-upstream plan, testability,\n\
         failure cases. `cargo agent backlog --self-check` validates the result.\n",
    );
    template
}

/// Cross-check registry lifecycles against downstream git history.
///
/// `shipped` entries must have a downstream PR and a commit tagged `(#NNNN)`,
/// `mined` entries must have a downstream PR, and unworked statuses must not
/// carry one. Violations fail the command with exit code 4.
fn backlog_audit(repo: &Path, registry: &RegistryFile) -> AppResult<Value> {
    let mut violations = Vec::new();
    for issue in &registry.issues {
        let reference = format!("#{}", issue.number);
        let pr = issue.downstream_pr;
        let mut ok = true;
        let mut detail = String::new();
        match issue.status.as_str() {
            "shipped" => {
                let Some(pr) = pr else {
                    ok = false;
                    detail = "shipped without a downstream PR".into();
                    violations.push(AuditEntry {
                        reference: reference.clone(),
                        number: issue.number,
                        status: issue.status.clone(),
                        ok,
                        detail,
                    });
                    continue;
                };
                let tagged = run_command(
                    "git",
                    [
                        "log",
                        "--all",
                        "--format=%s",
                        &format!("--grep=(#{})", issue.number),
                    ],
                    Some(repo),
                );
                match tagged {
                    Ok(output)
                        if output.status == Some(0)
                            && !output.stdout.trim().is_empty() =>
                    {
                        detail = format!(
                            "commit tagged (#{}) found via PR #{pr}",
                            issue.number
                        );
                    }
                    _ => {
                        ok = false;
                        detail = format!(
                            "no commit tagged (#{}) in downstream history",
                            issue.number
                        );
                    }
                }
            }
            "mined" => match pr {
                Some(pr) => detail = format!("downstream PR #{pr}"),
                None => {
                    ok = false;
                    detail = "mined without a downstream PR".into();
                }
            },
            "candidate" | "watch" | "upstream-shipped" | "excluded" => {
                if let Some(pr) = pr {
                    ok = false;
                    detail = format!("unworked status carries downstream PR #{pr}");
                }
            }
            other => {
                ok = false;
                detail = format!("unknown status {other:?}");
            }
        }
        if !ok {
            violations.push(AuditEntry {
                reference,
                number: issue.number,
                status: issue.status.clone(),
                ok,
                detail,
            });
        }
    }
    if !violations.is_empty() {
        return Err(AppError::verification(format!(
            "backlog lifecycle audit failed: {}",
            violations
                .iter()
                .map(|entry| format!("{} {}", entry.reference, entry.detail))
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }
    Ok(json_value(AuditReport {
        status: "passed",
        checked: registry.issues.len(),
        violations,
    })?)
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
    if kinds != 9 || context.guides.is_empty() {
        return Err(AppError::verification(
            "control-plane self-check did not find the contract or scoped guide",
        ));
    }
    let required_tasks = [
        "backlog-audit-golden",
        "backlog-investigate-golden",
        "backlog-score-golden",
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
        EvalAgentCommand::Backlog => {
            args.extend([
                "backlog".into(),
                "--snapshot".into(),
                "evals/fixtures/backlog/golden".into(),
            ]);
            "backlog"
        }
        EvalAgentCommand::BacklogInvestigate => {
            args.extend([
                "backlog".into(),
                "--snapshot".into(),
                "evals/fixtures/backlog/golden".into(),
                "--investigate".into(),
                "2102".into(),
            ]);
            "backlog"
        }
        EvalAgentCommand::BacklogAudit => {
            args.extend([
                "backlog".into(),
                "--snapshot".into(),
                "evals/fixtures/backlog/golden".into(),
                "--audit".into(),
            ]);
            "backlog"
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

fn release_manifest(input_path: &Path) -> AppResult<Value> {
    let repo = root()?;
    let input_path = release_evidence_path(&repo, input_path)?;
    let input: ReleaseManifestInput =
        serde_json::from_str(&read_semantic_text(&input_path)?).map_err(|error| {
            AppError::invalid(format!("invalid release preparation evidence: {error}"))
        })?;
    if input.artifacts.is_empty()
        || input.sigstore_bundle_paths.is_empty()
        || input.provenance_attestation_paths.is_empty()
        || input.reproducibility.is_empty()
        || input.smoke_results.is_empty()
    {
        return Err(AppError::authority(
            "release preparation evidence must contain artifacts, signatures, provenance, reproducibility, and smoke results",
        ));
    }
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
    let upstream_version = value
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| AppError::invalid("workspace.package.version is missing"))?;
    let cli: toml::Value = read_semantic_text(&repo.join("crates/typst-cli/Cargo.toml"))?
        .parse()
        .map_err(|error| AppError::invalid(format!("invalid CLI Cargo.toml: {error}")))?;
    let downstream_version = cli
        .get("package")
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| AppError::invalid("typst-cli package.version is missing"))?;
    let expected_tag = format!("v{downstream_version}");
    if input.release_tag != expected_tag {
        return Err(AppError::verification(format!(
            "release tag must be {expected_tag}, got {}",
            input.release_tag
        )));
    }

    let mut names = BTreeSet::new();
    let mut artifacts = Vec::with_capacity(input.artifacts.len());
    for artifact in input.artifacts {
        let evidence = release_file_evidence(&repo, &artifact.path)?;
        if !names.insert(evidence.name.clone()) {
            return Err(AppError::invalid(format!(
                "duplicate release artifact name: {}",
                evidence.name
            )));
        }
        artifacts.push(ReleaseArtifact {
            name: evidence.name,
            kind: artifact.kind,
            sha256: evidence.sha256,
            size: evidence.size,
        });
    }
    artifacts.sort_by(|left, right| left.name.cmp(&right.name));

    for reproducibility in &input.reproducibility {
        if reproducibility.target.trim().is_empty()
            || !valid_sha256(&reproducibility.first_sha256)
            || !valid_sha256(&reproducibility.second_sha256)
            || !reproducibility.identical
            || reproducibility.first_sha256 != reproducibility.second_sha256
        {
            return Err(AppError::verification(format!(
                "invalid reproducibility evidence for {}",
                reproducibility.target
            )));
        }
    }

    let mut smoke_results = Vec::with_capacity(input.smoke_results.len());
    for smoke in input.smoke_results {
        if smoke.subject.trim().is_empty() || smoke.platform.trim().is_empty() {
            return Err(AppError::invalid(
                "release smoke subject and platform must be non-empty",
            ));
        }
        smoke_results.push(ReleaseSmokeResult {
            subject: smoke.subject,
            platform: smoke.platform,
            evidence: release_file_evidence(&repo, &smoke.evidence_path)?,
            passed: true,
        });
    }
    smoke_results.sort_by(|left, right| {
        (&left.subject, &left.platform).cmp(&(&right.subject, &right.platform))
    });

    let payload = json!({
        "product": "typst-agent",
        "downstream_version": downstream_version,
        "upstream_version": upstream_version,
        "release_tag": input.release_tag,
        "upstream_sha": upstream_sha,
        "downstream_sha": downstream_sha,
        "artifacts": artifacts,
        "sbom": release_file_evidence(&repo, &input.sbom_path)?,
        "sigstore_bundles": release_file_evidence_list(&repo, &input.sigstore_bundle_paths)?,
        "provenance_attestations": release_file_evidence_list(&repo, &input.provenance_attestation_paths)?,
        "reproducibility": input.reproducibility,
        "smoke_results": smoke_results,
    });
    let manifest = json!({
        "contract_version": CONTRACT_VERSION,
        "kind": "ReleaseManifest",
        "payload": payload,
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
    Ok(json!({"path": ".tmp/agent/release-manifest.json", "record": manifest}))
}

fn release_evidence_path(repo: &Path, path: &Path) -> AppResult<PathBuf> {
    let path = normalize_repo_path(&path.to_string_lossy())?;
    if !path.starts_with(".tmp/agent/release/") {
        return Err(AppError::invalid(format!(
            "release evidence must stay under .tmp/agent/release/: {path}"
        )));
    }
    Ok(repo.join(path))
}

fn release_file_evidence(repo: &Path, path: &Path) -> AppResult<ReleaseFileEvidence> {
    let full = release_evidence_path(repo, path)?;
    let metadata = fs::metadata(&full).map_err(|error| {
        AppError::authority(format!(
            "cannot inspect release evidence {}: {error}",
            full.display()
        ))
    })?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(AppError::authority(format!(
            "release evidence must be a non-empty file: {}",
            full.display()
        )));
    }
    let mut file = fs::File::open(&full).map_err(|error| {
        AppError::authority(format!(
            "cannot open release evidence {}: {error}",
            full.display()
        ))
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            AppError::authority(format!(
                "cannot hash release evidence {}: {error}",
                full.display()
            ))
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let normalized = normalize_repo_path(&path.to_string_lossy())?;
    let name = normalized
        .strip_prefix(".tmp/agent/release/")
        .unwrap_or(&normalized)
        .to_owned();
    Ok(ReleaseFileEvidence {
        name,
        sha256: digest_hex(&hasher.finalize()),
        size: metadata.len(),
    })
}

fn release_file_evidence_list(
    repo: &Path,
    paths: &[PathBuf],
) -> AppResult<Vec<ReleaseFileEvidence>> {
    let mut evidence = paths
        .iter()
        .map(|path| release_file_evidence(repo, path))
        .collect::<AppResult<Vec<_>>>()?;
    evidence.sort_by(|left, right| left.name.cmp(&right.name));
    let unique = evidence.iter().map(|item| &item.name).collect::<BTreeSet<_>>();
    if unique.len() != evidence.len() {
        return Err(AppError::invalid("duplicate release evidence file"));
    }
    Ok(evidence)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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
    fn release_input_is_strict_and_evidence_paths_are_contained() {
        let unknown = r#"{
            "release_tag":"v0.15.1-agent.0",
            "artifacts":[],
            "sbom_path":".tmp/agent/release/sbom.json",
            "sigstore_bundle_paths":[],
            "provenance_attestation_paths":[],
            "reproducibility":[],
            "smoke_results":[],
            "unexpected":true
        }"#;
        assert!(serde_json::from_str::<ReleaseManifestInput>(unknown).is_err());
        assert!(
            release_evidence_path(
                Path::new("/repository"),
                Path::new(".tmp/agent/release/artifact.tar.xz")
            )
            .is_ok()
        );
        assert!(
            release_evidence_path(
                Path::new("/repository"),
                Path::new(".tmp/agent/release/../../credential")
            )
            .is_err()
        );
        assert!(valid_sha256(&"a".repeat(64)));
        assert!(!valid_sha256(&"A".repeat(64)));
        assert!(!valid_sha256("placeholder"));
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

    #[test]
    fn backlog_demand_grade_follows_the_documented_rubric() {
        assert_eq!(demand_grade(0, 0), 1);
        assert_eq!(demand_grade(4, 3), 1);
        assert_eq!(demand_grade(0, 4), 2);
        assert_eq!(demand_grade(12, 0), 2);
        assert_eq!(demand_grade(15, 0), 3);
        assert_eq!(demand_grade(0, 10), 3);
        assert_eq!(demand_grade(40, 0), 4);
        assert_eq!(demand_grade(0, 20), 4);
        assert_eq!(demand_grade(100, 0), 5);
        assert_eq!(demand_grade(0, 30), 5);
    }

    #[test]
    fn backlog_score_matches_the_mining_formula_and_tiers() {
        // demand x confidence x safety x impact / burden
        assert_eq!(backlog_score(3, 4, 5, 4, 1), 240);
        assert_eq!(backlog_score(2, 4, 5, 4, 1), 160);
        assert_eq!(backlog_score(4, 3, 5, 3, 2), 90);
        assert_eq!(backlog_score(3, 4, 4, 4, 3), 64);
        assert_eq!(backlog_score(5, 5, 5, 5, 1), 625);
        assert_eq!(backlog_score(1, 1, 1, 1, 5), 0);
        assert_eq!(backlog_tier(120), "a");
        assert_eq!(backlog_tier(119), "b");
        assert_eq!(backlog_tier(48), "b");
        assert_eq!(backlog_tier(47), "c");
    }

    #[test]
    fn backlog_days_since_handles_leap_years_and_garbage() {
        assert_eq!(days_since("2026-02-10", "2026-08-16"), Some(187));
        assert_eq!(days_since("2026-08-16", "2026-08-16"), Some(0));
        assert_eq!(days_since("2024-02-28", "2024-03-01"), Some(2));
        assert_eq!(days_since("2023-02-28", "2023-03-01"), Some(1));
        assert_eq!(days_since("garbage", "2026-08-16"), None);
        assert_eq!(days_since("2026-13-01", "2026-08-16"), None);
    }

    #[test]
    fn backlog_stance_and_title_helpers_follow_the_contract() {
        assert!(VALID_STANCES.contains(&"endorsing"));
        assert!(VALID_STANCES.contains(&"none"));
        assert_eq!(VALID_STANCES.len(), 5);
        assert_eq!(truncate_title("short"), "short");
        let long = "x".repeat(80);
        let truncated = truncate_title(&long);
        assert_eq!(truncated.chars().count(), 61);
        assert!(truncated.ends_with('…'));
    }
}
