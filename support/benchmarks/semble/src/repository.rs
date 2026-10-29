//! Deterministic Git checkout management for version-pinned benchmark sources.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::case::Suite;

pub fn resolve_sources(
    suites: &[Suite],
    root: &Path,
    overrides: &HashMap<String, PathBuf>,
) -> Result<HashMap<String, PathBuf>, String> {
    fs::create_dir_all(root)
        .map_err(|error| format!("create source directory {}: {error}", root.display()))?;
    suites
        .iter()
        .map(|suite| {
            let path = overrides
                .get(&suite.name)
                .cloned()
                .unwrap_or_else(|| root.join(&suite.name));
            if overrides.contains_key(&suite.name) {
                verify_checkout(&path, &suite.commit)?;
            } else {
                ensure_checkout(&path, suite)?;
            }
            Ok((suite.name.clone(), path))
        })
        .collect()
}

pub fn prepare_competitor_sources(
    suites: &[Suite],
    sources: &HashMap<String, PathBuf>,
    root: &Path,
) -> Result<HashMap<String, PathBuf>, String> {
    fs::create_dir_all(root).map_err(|error| {
        format!(
            "create competitor source directory {}: {error}",
            root.display()
        )
    })?;
    suites
        .iter()
        .map(|suite| {
            let source = sources
                .get(&suite.name)
                .ok_or_else(|| format!("missing source path for {}", suite.name))?;
            let target = root.join(&suite.name);
            if target.exists() {
                fs::remove_dir_all(&target).map_err(|error| {
                    format!("reset competitor checkout {}: {error}", target.display())
                })?;
            }
            run(
                Command::new("git")
                    .args(["clone", "--shared", "--no-checkout"])
                    .arg(source)
                    .arg(&target),
                "clone isolated competitor checkout",
            )?;
            run(
                Command::new("git").arg("-C").arg(&target).args([
                    "checkout",
                    "--detach",
                    &suite.commit,
                ]),
                "checkout competitor commit",
            )?;
            Ok((suite.name.clone(), target))
        })
        .collect()
}

fn ensure_checkout(path: &Path, suite: &Suite) -> Result<(), String> {
    if !path.join(".git").exists() {
        if path.exists() {
            fs::remove_dir_all(path).map_err(|error| {
                format!("remove incomplete checkout {}: {error}", path.display())
            })?;
        }
        run(
            Command::new("git")
                .args(["clone", "--filter=blob:none", "--no-checkout"])
                .arg(&suite.repository)
                .arg(path),
            "clone repository",
        )?;
    }
    if verify_checkout(path, &suite.commit).is_ok() {
        return Ok(());
    }
    run(
        Command::new("git").arg("-C").arg(path).args([
            "fetch",
            "--depth=1",
            "origin",
            &suite.commit,
        ]),
        "fetch pinned commit",
    )?;
    run(
        Command::new("git")
            .arg("-C")
            .arg(path)
            .args(["checkout", "--detach", &suite.commit]),
        "checkout pinned commit",
    )?;
    verify_checkout(path, &suite.commit)
}

fn verify_checkout(path: &Path, expected: &str) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|error| format!("inspect checkout {}: {error}", path.display()))?;
    if !output.status.success() {
        return Err(format!("{} is not a readable Git checkout", path.display()));
    }
    let actual = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if actual != expected {
        return Err(format!(
            "checkout {} is at {actual}, expected {expected}",
            path.display()
        ));
    }
    Ok(())
}

fn run(command: &mut Command, action: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("{action}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{action} failed with {status}"))
    }
}
