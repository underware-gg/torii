use std::env;
use std::error::Error;
use std::process::Command;

use vergen::{BuildBuilder, Emitter};
use vergen_gitcl::GitclBuilder;

const UNDERWARE_TAG_PREFIX: &str = "uw-v";

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok())?
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn underware_version() -> String {
    git(&[
        "describe",
        "--tags",
        "--match",
        "uw-v*",
        "--abbrev=0",
        "HEAD",
    ])
    .and_then(|tag| tag.strip_prefix(UNDERWARE_TAG_PREFIX).map(str::to_owned))
    .unwrap_or_else(|| "unreleased".to_owned())
}

fn main() -> Result<(), Box<dyn Error>> {
    // vergen watches HEAD and the current branch. The fork release tag and packed-tag fallback
    // are separate inputs to the version specification.
    if let Some(git_dir) = git(&["rev-parse", "--path-format=absolute", "--git-dir"]) {
        println!("cargo:rerun-if-changed={git_dir}/refs/tags");
        println!("cargo:rerun-if-changed={git_dir}/packed-refs");
    }

    let build = BuildBuilder::default().build_timestamp(true).build()?;
    let gitcl = GitclBuilder::default()
        .describe(true, true, None)
        .branch(true)
        .sha(true)
        .build()?;

    // Emit the instructions
    Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&gitcl)?
        .emit_and_set()?;

    let git_sha = env::var("VERGEN_GIT_SHA").unwrap_or("unknown".to_string());
    let version = format!(
        "{}-uw (base torii v{}, {})",
        underware_version(),
        env!("CARGO_PKG_VERSION"),
        git_sha
    );
    println!("cargo:rustc-env=TORII_VERSION_SPEC={version}");

    Ok(())
}
