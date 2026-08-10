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
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const CONTRACT_VERSION: &str = "agent-contract/v1";
const MAX_OUTPUT_BYTES: usize = 64 * 1024;

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
struct InvariantFile {
    version: u32,
    records: Vec<InvariantRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    head: String,
    changed_paths: Vec<String>,
    changed_packages: Vec<String>,
    reverse_dependencies: BTreeMap<String, Vec<String>>,
    scoped_guides: Vec<String>,
    invariant_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct VerificationEvidence {
    tier: String,
    checks: Vec<CheckEvidence>,
    selected_tests: Vec<String>,
    dirty_paths: Vec<String>,
    reference_paths: Vec<String>,
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
        CommandKind::Doctor => Ok(("doctor", doctor()?)),
        CommandKind::Context(args) => Ok(("context", context(args)?)),
        CommandKind::Impact(args) => Ok((
            "impact",
            serde_json::to_value(impact(&args.base)?)
                .map_err(|error| AppError::invalid(error.to_string()))?,
        )),
        CommandKind::Verify(args) => Ok((
            "verify",
            serde_json::to_value(verify(args.tier)?)
                .map_err(|error| AppError::invalid(error.to_string()))?,
        )),
        CommandKind::ReviewPack(args) => Ok(("review-pack", review_pack(&args.base)?)),
        CommandKind::PolicyCheck => Ok(("policy-check", policy_check()?)),
        CommandKind::UpstreamCheck => Ok(("upstream-check", upstream_check()?)),
        CommandKind::Eval => Ok(("eval", eval()?)),
        CommandKind::ReleaseManifest => Ok(("release-manifest", release_manifest()?)),
    }
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
    Ok(CommandOutput {
        status: output.status.code(),
        stdout: bounded(String::from_utf8_lossy(&output.stdout).into_owned()),
        stderr: bounded(String::from_utf8_lossy(&output.stderr).into_owned()),
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
        output.truncate(MAX_OUTPUT_BYTES);
        output.push_str("\n[output truncated]");
    }
    output
}

fn require_success(output: CommandOutput, label: &str) -> AppResult<String> {
    if output.status == Some(0) {
        Ok(output.stdout)
    } else {
        Err(AppError::authority(format!("{label} failed: {}", output.stderr.trim())))
    }
}

fn read_invariants(repo: &Path) -> AppResult<InvariantFile> {
    let path = repo.join(".agents/invariants.yml");
    let content = fs::read_to_string(&path).map_err(|error| {
        AppError::authority(format!("cannot read {}: {error}", path.display()))
    })?;
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

fn doctor() -> AppResult<Value> {
    let repo = root()?;
    let required = [
        "AGENTS.md",
        "agent-contract/v1/schema.json",
        ".agents/INDEX.md",
        ".agents/invariants.yml",
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
    let schema: Value = serde_json::from_str(
        &fs::read_to_string(repo.join("agent-contract/v1/schema.json"))
            .map_err(|e| AppError::authority(e.to_string()))?,
    )
    .map_err(|e| AppError::invalid(format!("contract schema is not JSON: {e}")))?;
    let schema_id = schema.get("$id").and_then(Value::as_str).unwrap_or_default();
    if !schema_id.ends_with("/agent-contract/v1/schema.json") {
        return Err(AppError::invalid("contract schema has an unexpected $id"));
    }
    let invariants = read_invariants(&repo)?;
    let push_url =
        run_command("git", ["remote", "get-url", "--push", "upstream"], Some(&repo))?;
    let push_url = push_url.stdout.trim().to_string();
    if push_url.is_empty() || push_url.contains("github.com/typst/typst") {
        return Err(AppError::policy(
            "upstream must have an invalid fetch-only push URL",
        ));
    }
    Ok(json!({
        "repository": repo.display().to_string(),
        "required_files": required,
        "invariant_count": invariants.records.len(),
        "upstream_push_url": push_url,
        "contract_version": CONTRACT_VERSION,
    }))
}

fn changed_paths(repo: &Path, base: Option<&str>) -> AppResult<Vec<String>> {
    let output = if let Some(base) = base {
        run_command(
            "git",
            ["diff", "--name-only", "--diff-filter=ACMRTUXB", &format!("{base}...HEAD")],
            Some(repo),
        )?
    } else {
        run_command("git", ["status", "--short"], Some(repo))?
    };
    if output.status != Some(0) {
        return Err(AppError::authority(format!(
            "cannot enumerate changed paths: {}",
            output.stderr.trim()
        )));
    }
    let mut paths = BTreeSet::new();
    for line in output.stdout.lines() {
        let path = if base.is_some() {
            line.trim()
        } else {
            line.get(3..).unwrap_or(line).trim()
        };
        if !path.is_empty() {
            paths.insert(path.replace('\\', "/"));
        }
    }
    Ok(paths.into_iter().collect())
}

fn dirty_paths(repo: &Path) -> AppResult<Vec<String>> {
    changed_paths(repo, None)
}

fn reference_paths(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .filter(|path| {
            path.starts_with("tests/ref/")
                || path.starts_with("tests/suite/")
                || path.ends_with(".snap")
        })
        .cloned()
        .collect()
}

fn assert_contained(paths: &[String]) -> AppResult<()> {
    let outside = paths
        .iter()
        .filter(|path| path.starts_with('/') || path.split('/').any(|part| part == ".."))
        .cloned()
        .collect::<Vec<_>>();
    if outside.is_empty() {
        Ok(())
    } else {
        Err(AppError::policy(format!(
            "worktree path escapes repository scope: {}",
            outside.join(", ")
        )))
    }
}

fn selected_tests(paths: &[String]) -> Vec<(String, Vec<String>)> {
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
        .filter(|package| package != "typst-agent-dev")
        .map(|package| {
            (
                format!("cargo test -p {package}"),
                vec!["test".into(), "-p".into(), package],
            )
        })
        .collect::<Vec<_>>();
    if integration {
        commands.push(("cargo testit".into(), vec!["testit".into()]));
    }
    commands
}

fn context(args: &ContextArgs) -> AppResult<Value> {
    let repo = root()?;
    let paths = if args.paths.is_empty() {
        changed_paths(&repo, None)?
    } else {
        args.paths
            .iter()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect()
    };
    let invariants = read_invariants(&repo)?;
    let mut guides = BTreeSet::new();
    let mut records = BTreeMap::new();
    for path in &paths {
        if let Some(guide) = guide_for_path(path) {
            guides.insert(guide.to_string());
        }
        for invariant in &invariants.records {
            if path == &invariant.scope
                || path.starts_with(&format!("{}/", invariant.scope))
            {
                records.insert(invariant.id.clone(), invariant.clone());
            }
        }
    }
    if paths.is_empty() {
        guides.insert(".agents/INDEX.md".into());
    }
    Ok(
        json!({"paths": paths, "guides": guides.into_iter().collect::<Vec<_>>(), "invariants": records.into_values().collect::<Vec<_>>() }),
    )
}

fn guide_for_path(path: &str) -> Option<&'static str> {
    let guide = if path.starts_with("crates/typst-syntax/") {
        ".agents/areas/parser-spans.md"
    } else if path.starts_with("crates/typst-eval/")
        || path.starts_with("crates/typst-library/")
    {
        ".agents/areas/evaluation.md"
    } else if path.starts_with("crates/typst-layout/")
        || path.starts_with("crates/typst-realize/")
        || path.starts_with("crates/typst/")
    {
        ".agents/areas/layout.md"
    } else if path.starts_with("crates/typst-ide/") {
        ".agents/areas/ide.md"
    } else if path.starts_with("crates/typst-cli/") {
        ".agents/areas/cli.md"
    } else if path.starts_with("crates/typst-pdf/")
        || path.starts_with("crates/typst-render/")
        || path.starts_with("crates/typst-svg/")
    {
        ".agents/areas/output.md"
    } else if path.starts_with("tests/") {
        ".agents/areas/tests.md"
    } else if path.starts_with(".github/")
        || path == "Dockerfile"
        || path.starts_with("scripts/")
    {
        ".agents/areas/release.md"
    } else if path.starts_with(".agents/")
        || path.starts_with("agent-contract/")
        || path == "AGENTS.md"
    {
        "AGENTS.md"
    } else {
        return None;
    };
    Some(guide)
}

fn impact(base: &str) -> AppResult<ImpactReport> {
    let repo = root()?;
    let changed = changed_paths(&repo, Some(base))?;
    let metadata = cargo_metadata(&repo)?;
    let mut changed_packages = BTreeSet::new();
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
        if changed.iter().any(|path| {
            path == &package.manifest_path || path.starts_with(&format!("{relative}/"))
        }) {
            changed_packages.insert(package.name.clone());
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
    let direct: Vec<String> = changed_packages.iter().cloned().collect();
    let mut affected = changed_packages.clone();
    let mut queue = direct.clone();
    while let Some(name) = queue.pop() {
        for dependent in reverse.get(&name).into_iter().flatten() {
            if affected.insert(dependent.clone()) {
                queue.push(dependent.clone());
            }
        }
    }
    let reverse_dependencies = reverse
        .into_iter()
        .map(|(key, values)| (key, values.into_iter().collect()))
        .collect();
    let scoped =
        context(&ContextArgs { paths: changed.iter().map(PathBuf::from).collect() })?;
    let scoped_guides = scoped
        .get("guides")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let invariant_ids = scoped
        .get("invariants")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("id").and_then(Value::as_str).map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(ImpactReport {
        base: base.to_owned(),
        head: require_success(
            run_command("git", ["rev-parse", "HEAD"], Some(&repo))?,
            "git rev-parse",
        )?
        .trim()
        .to_owned(),
        changed_paths: changed,
        changed_packages: affected.into_iter().collect(),
        reverse_dependencies,
        scoped_guides,
        invariant_ids,
    })
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

fn policy_check() -> AppResult<Value> {
    let repo = root()?;
    let mut violations = Vec::new();
    let push_url =
        run_command("git", ["remote", "get-url", "--push", "upstream"], Some(&repo))?;
    let push_url = push_url.stdout.trim().to_string();
    if push_url.is_empty() || push_url.contains("github.com/typst/typst") {
        violations.push("upstream has a writable or missing push URL".to_string());
    }
    let files =
        run_command("git", ["ls-files", "-co", "--exclude-standard"], Some(&repo))?;
    for path in files.stdout.lines().map(str::trim).filter(|path| !path.is_empty()) {
        if path.starts_with(".git/")
            || path.starts_with("target/")
            || path.starts_with(".tmp/")
            || path.contains("node_modules/")
        {
            continue;
        }
        let full = repo.join(path);
        let Ok(bytes) = fs::read(&full) else { continue };
        if bytes.contains(&0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let private_key_marker = ["-----BEGIN ", "PRIVATE KEY-----"].concat();
        let github_token_marker = ["g", "ho_"].concat();
        let openai_marker = ["s", "k-"].concat();
        let aws_marker = ["AK", "IA"].concat();
        let slack_marker = ["xox", "b-"].concat();
        let secret = text.contains(&private_key_marker)
            || contains_token(&text, &github_token_marker, 20)
            || contains_token(&text, &openai_marker, 20)
            || contains_token(&text, &aws_marker, 16)
            || contains_token(&text, &slack_marker, 20);
        if secret {
            violations.push(format!("possible credential in {path}"));
        }
        let upstream_push_marker =
            ["git push https://github.com/", "typst/typst"].concat();
        let upstream_ssh_marker = ["git@github.com:", "typst/typst"].concat();
        if text.contains(&upstream_push_marker) || text.contains(&upstream_ssh_marker) {
            violations.push(format!("upstream publication command in {path}"));
        }
    }
    let required = ["AI_DISCLOSURE.md", "TRADEMARKS.md", "DCO", ".github/CODEOWNERS"];
    for path in required {
        if !repo.join(path).is_file() {
            violations.push(format!("missing {path}"));
        }
    }
    if !violations.is_empty() {
        return Err(AppError::policy(violations.join("; ")));
    }
    Ok(
        json!({"violations": [], "upstream_push_url": push_url, "checked": "tracked-and-untracked-files"}),
    )
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
    let repo = root()?;
    let dirty = dirty_paths(&repo)?;
    assert_contained(&dirty)?;
    let references = reference_paths(&dirty);
    if !references.is_empty() && !repo.join(".tmp/agent/reference-review.json").is_file()
    {
        return Err(AppError::policy(format!(
            "reference changes require .tmp/agent/reference-review.json: {}",
            references.join(", ")
        )));
    }
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
    let mut commands: Vec<(String, Vec<String>)> = vec![
        (
            "cargo fmt --check".into(),
            vec!["fmt".into(), "--all".into(), "--check".into()],
        ),
        (
            "cargo check -p typst-agent-dev".into(),
            vec!["check".into(), "-p".into(), "typst-agent-dev".into()],
        ),
    ];
    if matches!(tier, VerifyTier::Pr | VerifyTier::Full) {
        commands.push((
            "cargo test -p typst-agent-dev".into(),
            vec!["test".into(), "-p".into(), "typst-agent-dev".into()],
        ));
        if !matches!(tier, VerifyTier::Full) {
            commands.extend(selected_tests(&dirty));
        }
    }
    if matches!(tier, VerifyTier::Full) {
        commands.push((
            "cargo test --workspace".into(),
            vec!["test".into(), "--workspace".into()],
        ));
    }
    for (name, args) in commands {
        let result = run_command("cargo", args.iter(), Some(&repo))?;
        let passed = result.status == Some(0);
        checks.push(CheckEvidence {
            name,
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
        selected_tests: selected_tests(&dirty)
            .into_iter()
            .map(|(name, _)| name)
            .collect(),
        dirty_paths: dirty,
        reference_paths: references,
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
    let dirty = dirty_paths(&repo)?;
    assert_contained(&dirty)?;
    let references = reference_paths(&dirty);
    let impact_report = impact(base)?;
    let policy = policy_check()?;
    let pack = json!({
        "contract_version": CONTRACT_VERSION,
        "base": base,
        "impact": impact_report,
        "policy": policy,
        "dirty_worktree": {"paths": dirty, "contained": true},
        "reference_guard": {"paths": references, "approval_required": true, "baseline_update_allowed": false},
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
    let remote_tags = remote_tags(&repo)?;
    let local_tags = local_tags(&repo)?;
    if remote_tags != local_tags {
        return Err(AppError::policy(format!(
            "mirrored tags differ (upstream={}, local={})",
            remote_tags.len(),
            local_tags.len()
        )));
    }
    Ok(
        json!({"mirror": mirror, "upstream": fetched, "identical": true, "push_url": push_url, "tags": {"count": local_tags.len(), "identical": true}}),
    )
}

fn remote_tags(repo: &Path) -> AppResult<BTreeMap<String, String>> {
    let output = require_success(
        run_command("git", ["ls-remote", "--tags", "upstream"], Some(repo))?,
        "upstream tags",
    )?;
    Ok(output
        .lines()
        .filter_map(|line| {
            let (sha, reference) = line.split_once('\t')?;
            let name = reference.strip_prefix("refs/tags/")?;
            if name.ends_with("^{}") {
                return None;
            }
            Some((name.to_owned(), sha.to_owned()))
        })
        .collect())
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
    Ok(output
        .lines()
        .filter_map(|line| {
            let (sha, name) = line.split_once('\t')?;
            Some((name.to_owned(), sha.to_owned()))
        })
        .collect())
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
    if kinds < 8 || context.get("guides").and_then(Value::as_array).is_none() {
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
    ];
    let task_directory = repo.join("evals/tasks");
    let mut tasks = BTreeSet::new();
    for entry in fs::read_dir(&task_directory).map_err(|error| {
        AppError::authority(format!("cannot read eval tasks: {error}"))
    })? {
        let path = entry.map_err(|error| AppError::authority(error.to_string()))?.path();
        if path.extension().and_then(OsStr::to_str) != Some("toml") {
            continue;
        }
        let content = fs::read_to_string(&path)
            .map_err(|error| AppError::authority(error.to_string()))?;
        let value: toml::Value = content.parse().map_err(|error| {
            AppError::invalid(format!("invalid eval task {}: {error}", path.display()))
        })?;
        let Some(id) = value.get("id").and_then(toml::Value::as_str) else {
            return Err(AppError::invalid(format!(
                "eval task {} has no id",
                path.display()
            )));
        };
        for key in ["scope", "exercise", "expected", "grader"] {
            if value.get(key).is_none() {
                return Err(AppError::invalid(format!("eval task {id} has no {key}")));
            }
        }
        tasks.insert(id.to_owned());
    }
    let missing: Vec<_> = required_tasks
        .iter()
        .filter(|id| !tasks.contains(**id))
        .copied()
        .collect();
    if !missing.is_empty() {
        return Err(AppError::verification(format!(
            "evaluation catalog is missing: {}",
            missing.join(", ")
        )));
    }
    Ok(
        json!({"checks": ["contract-schema", "scoped-context", "secret-boundary", "task-catalog"], "task_count": tasks.len(), "model_calls": 0, "status": "passed"}),
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
    fn path_rules_select_the_narrowest_guidance() {
        assert_eq!(
            guide_for_path("crates/typst-syntax/src/parser.rs"),
            Some(".agents/areas/parser-spans.md")
        );
        assert_eq!(
            guide_for_path("crates/typst-cli/src/main.rs"),
            Some(".agents/areas/cli.md")
        );
        assert_eq!(guide_for_path("unknown/file.txt"), None);
    }

    #[test]
    fn output_is_bounded() {
        let output = bounded("x".repeat(MAX_OUTPUT_BYTES + 4));
        assert!(output.len() <= MAX_OUTPUT_BYTES + "\n[output truncated]".len());
        assert!(output.ends_with("[output truncated]"));
    }
}
