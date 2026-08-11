//! Immutable downstream and upstream build identity.

/// The upstream source snapshot carried by this downstream release.
pub const UPSTREAM_SHA: &str = "a51e028041cac426f97d34335bb01d8f1d8e5e8f";

/// The downstream release version from the CLI package metadata.
pub const RELEASE: &str = env!("CARGO_PKG_VERSION");

/// The exact downstream commit embedded by the CLI build script.
pub const DOWNSTREAM_SHA: &str = match option_env!("TYPST_AGENT_COMMIT_SHA") {
    Some(sha) => sha,
    None => "unknown",
};

/// Render the multi-line identity used by `typst-agent --version`.
pub fn version() -> String {
    format!(
        "{RELEASE}\nupstream Typst {} ({UPSTREAM_SHA})\ndownstream build {DOWNSTREAM_SHA}",
        typst_utils::version().raw(),
    )
}
