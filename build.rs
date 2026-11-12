use std::env;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=protocols/hyprland-toplevel-export-v1.xml");
    println!("cargo:rerun-if-changed=config.example.toml");
    println!("cargo:rerun-if-env-changed=HYPRVIEW_RELEASE");
    emit_git_version();
}

/// Embed a short commit id when this is not a build of the exact release tag,
/// so `--version` can tell a tagged release from a local/in-between build.
/// Cargo's version alone stays at the last release until the next bump.
fn emit_git_version() {
    // Official release jobs set this because shallow CI checkouts cannot be
    // relied on to expose tag refs for detection.
    if env::var("HYPRVIEW_RELEASE").is_ok_and(|value| value == "1") {
        return;
    }

    // Keep the embedded metadata honest when the checkout or working tree
    // changes without any source file changing.
    for path in ["HEAD", "index", "packed-refs", "refs/tags"] {
        if let Some(path) = git(&["rev-parse", "--git-path", path]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    println!("cargo:rerun-if-changed=src");

    let Some(version) = env::var("CARGO_PKG_VERSION").ok() else {
        return;
    };
    let release_tag = format!("v{version}");
    let on_release_tag = git(&["tag", "--points-at", "HEAD"])
        .is_some_and(|tags| tags.lines().any(|tag| tag.trim() == release_tag.as_str()));
    if on_release_tag {
        return;
    }

    let Some(sha) = git(&["rev-parse", "--short", "HEAD"]) else {
        return;
    };
    let dirty = git(&["status", "--porcelain"]).is_some_and(|status| !status.is_empty());
    let suffix = if dirty { format!("{sha}-dirty") } else { sha };
    println!("cargo:rustc-env=HYPRVIEW_GIT_SHA={suffix}");
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_owned())
}
