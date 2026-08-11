use std::env;
use std::fs::{File, create_dir_all};
use std::path::Path;

use clap::{CommandFactory, ValueEnum};
use clap_complete::{Shell, generate_to};
use clap_mangen::Man;

#[path = "src/args.rs"]
#[expect(dead_code)]
mod args;
#[path = "src/identity.rs"]
mod identity;

fn main() {
    // https://stackoverflow.com/a/51311222/11494565
    println!("cargo:rustc-env=TARGET={}", env::var("TARGET").unwrap());
    println!("cargo:rerun-if-env-changed=GEN_ARTIFACTS");
    println!("cargo:rerun-if-env-changed=TYPST_AGENT_COMMIT_SHA");
    let downstream_sha = env::var("TYPST_AGENT_COMMIT_SHA")
        .ok()
        .or_else(git_head)
        .unwrap_or_else(|| "unknown".into());
    assert!(
        downstream_sha == "unknown"
            || downstream_sha.len() == 40
                && downstream_sha.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "TYPST_AGENT_COMMIT_SHA must be a 40-character hexadecimal Git object ID",
    );
    println!("cargo:rustc-env=TYPST_AGENT_COMMIT_SHA={downstream_sha}");

    if let Some(dir) = env::var_os("GEN_ARTIFACTS") {
        let out = &Path::new(&dir);
        create_dir_all(out).unwrap();
        let cmd = &mut args::CliArguments::command();

        Man::new(cmd.clone())
            .render(&mut File::create(out.join("typst-agent.1")).unwrap())
            .unwrap();

        for subcmd in cmd.get_subcommands() {
            let name = format!("typst-agent-{}", subcmd.get_name());
            Man::new(subcmd.clone().name(&name))
                .render(&mut File::create(out.join(format!("{name}.1"))).unwrap())
                .unwrap();
        }

        for shell in Shell::value_variants() {
            generate_to(*shell, cmd, "typst-agent", out).unwrap();
        }
    }
}

fn git_head() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|sha| sha.trim().to_owned())
}
