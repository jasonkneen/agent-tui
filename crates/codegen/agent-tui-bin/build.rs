use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-env-changed=GROK_VERSION");

    windows_link_args();

    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let version = std::env::var("GROK_VERSION")
        .or_else(|_| std::env::var("CARGO_PKG_VERSION"))
        .unwrap_or_else(|_| "0.0.0".to_string());

    println!(
        "cargo:rustc-env=VERSION_WITH_COMMIT={} ({})",
        version, commit
    );
}

/// Windows (MSVC) linker settings for the `agent-tui` binary.
///
/// These are emitted as `rustc-link-arg-bins` so they apply only when linking
/// this package's binaries. Putting them in `.cargo/config.toml` `rustflags`
/// instead would pass them to *every* crate's rustc invocation in the
/// workspace, which is unnecessary (only the final link consumes them).
///
/// `/STACK:8388608` — Windows reserves 1 MiB for the main thread; Linux and
/// macOS give it 8 MiB. The `agent-tui` startup path exceeds 1 MiB, so without
/// this the binary links successfully and then dies on every invocation — even
/// `--version` — with STATUS_STACK_OVERFLOW (0xC00000FD). The reserve is baked
/// into the PE header at link time, so `RUST_MIN_STACK` cannot substitute: it
/// only affects threads Rust spawns, not the process main thread.
///
/// The related PDB symbol-length limit is handled separately, by
/// `symbol-mangling-version=v0` in `.cargo/config.toml` — deliberately not with
/// the linker's `/DEBUG:LongSymbolTruncate`, which does not exist before MSVC
/// 14.5x and is a fatal `LNK1117` on older toolchains.
fn windows_link_args() {
    let is_windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let is_msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if !(is_windows && is_msvc) {
        return;
    }

    println!("cargo:rustc-link-arg-bins=/STACK:8388608");
}
