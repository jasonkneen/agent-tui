use anyhow::{Context, bail};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn check_protoc_good(protoc: &Path) -> anyhow::Result<()> {
    let output = Command::new(protoc)
        .arg("--version")
        .output()
        .context("Failed to execute protoc")?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "protoc --version failed, likely dotslash is missing; \
             try `cargo install dotslash`; stdout: {stdout:?}, stderr: {stderr:?}"
        );
    }
    Ok(())
}

fn is_github_actions() -> bool {
    env::var_os("GITHUB_ACTIONS").is_some()
}

/// Resolve a DotSlash launcher file to the concrete executable it points at.
///
/// A DotSlash file is a JSON text file with no extension, relying on a shebang
/// to run. Windows has no shebang support, so executing `bin/protoc` directly
/// fails with `%1 is not a valid Win32 application` (os error 193).
/// `dotslash -- fetch` downloads the platform entry if it is not cached yet and
/// prints the path of the real binary, which is what prost-build needs since it
/// takes a single executable path and cannot invoke `dotslash <manifest>`.
fn resolve_with_dotslash(manifest: &Path) -> anyhow::Result<PathBuf> {
    let output = Command::new("dotslash")
        .arg("--")
        .arg("fetch")
        .arg(manifest)
        .output()
        .context("Failed to execute dotslash")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("`dotslash -- fetch` failed: {stderr}");
    }

    let stdout = String::from_utf8(output.stdout).context("dotslash output not UTF-8")?;
    let resolved = PathBuf::from(stdout.trim());
    if !resolved.try_exists()? {
        bail!(
            "`dotslash -- fetch` returned a nonexistent path: {}",
            resolved.display()
        );
    }
    Ok(resolved)
}

/// Find `protoc` command.
///
/// Search order:
/// 1. `$PROTOC` environment variable (set by Bazel `build_script_env` or user override)
/// 2. `bin/protoc` walking up parent directories (dotslash wrapper for local dev)
/// 3. `protoc` on `$PATH` (system install or other tooling)
///
/// When `bin/protoc` exists but fails to execute (e.g. the dotslash wrapper running
/// in Bazel remote execution where `dotslash` is not installed), the error is not fatal —
/// we fall through to the PATH-based lookup instead.
///
/// Returns `Ok(None)` if not found and not in a strict environment (GitHub Actions).
pub fn find_protoc() -> anyhow::Result<Option<PathBuf>> {
    // 1. Check the PROTOC env var first. This is the standard override used by prost-build
    //    and is set by Bazel cargo_build_script build_script_env to point at a hermetic
    //    protoc binary instead of the dotslash wrapper.
    if let Ok(protoc_env) = env::var("PROTOC") {
        let protoc = PathBuf::from(&protoc_env);
        if protoc.try_exists()? {
            check_protoc_good(&protoc)?;
            return Ok(Some(protoc));
        }
    }

    // 2. Walk up directories looking for bin/protoc (dotslash wrapper).
    let cwd = env::current_dir()?;
    let mut dir = cwd.clone();
    let mut dir_rel = PathBuf::new();
    loop {
        // Return relative path to make build more deterministic.
        let protoc = dir_rel.join("bin/protoc");
        if protoc.try_exists()? {
            match check_protoc_good(&protoc) {
                Ok(()) => return Ok(Some(protoc)),
                Err(e) => {
                    // bin/protoc exists but can't be executed directly. Always
                    // true on Windows (no shebang support); elsewhere this
                    // usually means dotslash is missing (e.g. Bazel remote
                    // execution). Ask dotslash to resolve it to a real binary,
                    // then fall through to PATH-based lookup if that fails too.
                    match resolve_with_dotslash(&protoc) {
                        Ok(resolved) if check_protoc_good(&resolved).is_ok() => {
                            return Ok(Some(resolved));
                        }
                        _ => {
                            eprintln!(
                                "bin/protoc found at `{}` but failed to execute: {e:#}; \
                                 trying protoc from PATH as fallback",
                                protoc.display()
                            );
                            break;
                        }
                    }
                }
            }
        }
        if !dir.pop() {
            break;
        }
        dir_rel.push("..");
    }

    // 3. Try protoc from PATH (system install or other tooling).
    if check_protoc_good(Path::new("protoc")).is_ok() {
        return Ok(Some(PathBuf::from("protoc")));
    }

    // 4. Not found anywhere.
    if is_github_actions() {
        return Err(anyhow::anyhow!(
            "`protoc` not found (checked $PROTOC env, bin/protoc, and PATH)"
        ));
    }
    eprintln!("`protoc` not found; likely it is missing in docker image");
    Ok(None)
}
