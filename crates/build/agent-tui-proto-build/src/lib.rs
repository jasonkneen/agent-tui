pub mod find_protoc;

use anyhow::Context;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{fs, iter};

/// Find the protoc well-known types include directory.
///
/// When PROTOC is set (e.g., in Bazel), the include directory is typically
/// at `../include` relative to the `bin/protoc` binary. For example:
/// - PROTOC = `/path/to/external/protoc_linux_x86_64/bin/protoc`
/// - Include = `/path/to/external/protoc_linux_x86_64/include`
///
/// This is needed because Bazel places the protoc binary and include files
/// in separate locations within the sandbox, and protoc doesn't automatically
/// find them without an explicit -I flag.
fn find_protoc_include_dir(protoc: Option<&Path>) -> Option<PathBuf> {
    let protoc = protoc?;

    // protoc is typically at .../bin/protoc, so include is at .../include
    let parent = protoc.parent()?; // .../bin
    let grandparent = parent.parent()?; // .../
    let include_dir = grandparent.join("include");

    if include_dir.is_dir() {
        Some(include_dir)
    } else {
        None
    }
}

/// Parse protoc's `--dependency_out` file into the list of dependency paths.
///
/// The file is a makefile rule: `<target>: dep1 \`, then one continued
/// dependency per line, `\` marking continuation.
///
/// `target` is stripped by exact match rather than by splitting on the first
/// `:` — on Windows the target is an absolute path like
/// `C:\Temp\xyz\descriptor.pbbin`, so splitting on `:` would truncate it at the
/// drive letter.
///
/// protoc's own well-known types (`.../include/google/protobuf/*.proto`) are
/// skipped: they live at host-specific absolute paths (Homebrew, apt, the
/// dotslash cache), and emitting them would make build output non-deterministic
/// across machines. Separators are normalized so this matches on Windows too.
fn parse_dependency_file<'a>(target: &str, contents: &'a str) -> anyhow::Result<Vec<&'a str>> {
    let mut lines = contents.lines();
    let first_line = lines.next().context("protoc dependency output is empty")?;
    let rem = first_line
        .strip_prefix(target)
        .and_then(|rem| rem.strip_prefix(':'))
        .with_context(|| {
            format!("protoc dependency output must start with {target:?}: {contents:?}")
        })?;

    let mut deps = Vec::new();
    for line in iter::once(rem).chain(lines) {
        let line = line.trim();
        // Drop the trailing line-continuation marker, then re-trim the space
        // that separated it from the path.
        let line = line.strip_suffix('\\').unwrap_or(line).trim();
        if line.is_empty() {
            continue;
        }
        if line
            .replace('\\', "/")
            .contains("/include/google/protobuf/")
        {
            continue;
        }
        deps.push(line);
    }
    Ok(deps)
}

pub struct XaiProtoBuilder {
    builder: tonic_prost_build::Builder,
    file_descriptor_set_path: Option<PathBuf>,
    gen_pbjson: bool,
    pbjson_ignore_unknown_fields: bool,
    pbjson_preserve_proto_field_names: bool,
}

impl XaiProtoBuilder {
    fn map_builder(
        self,
        f: impl FnOnce(tonic_prost_build::Builder) -> tonic_prost_build::Builder,
    ) -> Self {
        Self {
            builder: f(self.builder),
            ..self
        }
    }

    pub fn bytes<S: AsRef<str>>(self, paths: impl IntoIterator<Item = S>) -> Self {
        self.map_builder(|b| paths.into_iter().fold(b, |b, path| b.bytes(path)))
    }

    pub fn extern_path(self, proto_path: impl AsRef<str>, rust_path: impl AsRef<str>) -> Self {
        self.map_builder(|b| b.extern_path(proto_path, rust_path))
    }

    pub fn file_descriptor_set_path(mut self, path: impl AsRef<Path>) -> Self {
        self.file_descriptor_set_path = Some(path.as_ref().to_path_buf());
        self.map_builder(|b| b.file_descriptor_set_path(path))
    }

    pub fn gen_pbjson(mut self) -> Self {
        self.gen_pbjson = true;
        self
    }

    pub fn pbjson_ignore_unknown_fields(mut self) -> Self {
        self.pbjson_ignore_unknown_fields = true;
        self
    }

    /// Serialize JSON using the original proto field names (snake_case) instead
    /// of the proto3-JSON default (camelCase). Deserialization still accepts
    /// both casings, so this is backward-compatible with already-stored
    /// camelCase documents.
    pub fn pbjson_preserve_proto_field_names(mut self) -> Self {
        self.pbjson_preserve_proto_field_names = true;
        self
    }

    pub fn generate_default_stubs(self, enable: bool) -> Self {
        self.map_builder(|b| b.generate_default_stubs(enable))
    }

    pub fn type_attribute(self, path: impl AsRef<str>, attr: impl AsRef<str>) -> Self {
        self.map_builder(|b| b.type_attribute(path, attr))
    }

    pub fn field_attribute(self, path: impl AsRef<str>, attr: impl AsRef<str>) -> Self {
        self.map_builder(|b| b.field_attribute(path, attr))
    }

    // tonic-build generation of `rerun-if-changed` is lazy and incorrect.
    // - everything is invalidated when anything inside include directories is changed
    // - also they compute paths incorrectly: assuming paths are relative to current directory
    //   rather than
    fn emit_rerun_if_changed<'a>(
        protoc: Option<&Path>,
        protoc_include_dir: Option<&Path>,
        protos: impl IntoIterator<Item = &'a Path>,
        includes: impl IntoIterator<Item = &'a Path>,
    ) -> anyhow::Result<()> {
        let includes = Vec::from_iter(includes);

        if let Some(protoc) = protoc {
            println!(
                "cargo:rerun-if-changed={}",
                protoc.to_str().context("protoc path not UTF-8")?
            );
        }

        // Can only process one input file when using --dependency_out=FILE.
        for proto in protos {
            // protoc writes these two outputs through its own file I/O, so both
            // destinations must be real paths: the Unix device files
            // `/dev/stdout` and `/dev/null` do not exist on Windows. A temp dir
            // behaves identically on every host, so this needs no cfg branching.
            let tempdir = tempfile::TempDir::new()?;
            let dependency_out = tempdir.path().join("deps.d");
            let descriptor_set_out = tempdir.path().join("descriptor.pbbin");
            let descriptor_set_out_str = descriptor_set_out
                .to_str()
                .context("descriptor_set_out path not UTF-8")?;

            let mut command = Command::new(protoc.unwrap_or(Path::new("protoc")));
            command
                .arg(format!(
                    "--dependency_out={}",
                    dependency_out
                        .to_str()
                        .context("dependency_out path not UTF-8")?
                ))
                .arg(format!("--descriptor_set_out={descriptor_set_out_str}"))
                // Mirror the codegen invocation below, which already sets this.
                // Without it, protoc releases before 3.15 reject the .proto
                // outright ("This file contains proto3 optional fields, but
                // --experimental_allow_proto3_optional was not set"), so the
                // dependency scan fails on any host whose distro protoc predates
                // that — including ubuntu-22.04's protobuf-compiler 3.12.
                .arg("--experimental_allow_proto3_optional");

            // Add protoc's well-known types include directory first (if found).
            // This is needed for Bazel sandboxed builds where protoc and its
            // include files are in different locations.
            if let Some(include_dir) = protoc_include_dir {
                command.arg(format!(
                    "-I{}",
                    include_dir.to_str().context("include path not UTF-8")?
                ));
            }

            for include in &includes {
                command.arg(format!("-I{}", include.to_str().context("path not UTF-8")?));
            }

            command.arg(proto);

            command.stdin(Stdio::null());
            command.stderr(Stdio::inherit());

            let output = command.output().context("protoc command failed")?;
            if !output.status.success() {
                return Err(anyhow::anyhow!("protoc command failed"));
            }

            let output = fs::read_to_string(&dependency_out)
                .context("failed to read protoc --dependency_out file")?;

            for dep in parse_dependency_file(descriptor_set_out_str, &output)? {
                if !fs::exists(dep)? {
                    return Err(anyhow::anyhow!("dependency file not found: {dep}"));
                }

                println!("cargo:rerun-if-changed={dep}");
            }
        }

        Ok(())
    }

    pub fn compile_protos(
        self,
        protos: &[impl AsRef<Path>],
        includes: &[impl AsRef<Path>],
    ) -> anyhow::Result<()> {
        for proto in protos {
            let proto = proto.as_ref();
            if proto.is_absolute() {
                return Err(anyhow::anyhow!(
                    "Absolute paths are not allowed: {}",
                    proto.display()
                ));
            }
        }

        let XaiProtoBuilder {
            builder,
            gen_pbjson,
            file_descriptor_set_path,
            pbjson_ignore_unknown_fields,
            pbjson_preserve_proto_field_names,
        } = self;
        let mut config = prost_build::Config::new();
        config.enable_type_names();

        let protoc = find_protoc::find_protoc()?;

        // Use fixed version of `protoc` binary.
        if let Some(protoc) = &protoc {
            config.protoc_executable(protoc);
        }

        // Find the protoc's well-known types include directory.
        // This is needed for Bazel sandboxed builds where protoc and its
        // include files are placed in different sandbox locations.
        let protoc_include_dir = find_protoc_include_dir(protoc.as_deref());

        let mut builder = builder.emit_rerun_if_changed(false);
        Self::emit_rerun_if_changed(
            protoc.as_deref(),
            protoc_include_dir.as_deref(),
            protos.iter().map(|p| p.as_ref()),
            includes.iter().map(|i| i.as_ref()),
        )?;

        let tempfile;

        let file_descriptor_set_path: Option<PathBuf> =
            if let Some(file_descriptor_set_path) = file_descriptor_set_path {
                Some(file_descriptor_set_path)
            } else if gen_pbjson {
                tempfile = tempfile::TempDir::new()?;
                let file_descriptor_set_path = tempfile.path().join("agent-tui-proto-build.pbbin");
                builder = builder.file_descriptor_set_path(&file_descriptor_set_path);
                Some(file_descriptor_set_path)
            } else {
                None
            };

        // Build the full includes list, prepending the protoc include directory
        // if found (for well-known types like google/protobuf/timestamp.proto).
        let all_includes: Vec<&Path> = protoc_include_dir
            .as_deref()
            .into_iter()
            .chain(includes.iter().map(|i| i.as_ref()))
            .collect();

        let protos: Vec<&Path> = protos.iter().map(|p| p.as_ref()).collect();

        builder
            .compile_with_config(config, &protos, &all_includes)
            .context("tonic_build failed")?;

        if gen_pbjson {
            let file_descriptor_set_path =
                file_descriptor_set_path.context("fds must be set at this moment")?;
            let descriptor_set = fs::read(&file_descriptor_set_path).with_context(|| {
                format!(
                    "Failed to read file descriptor set {}",
                    file_descriptor_set_path.display()
                )
            })?;
            let mut builder = pbjson_build::Builder::new();
            builder
                .register_descriptors(&descriptor_set)
                .context("Failed to register descriptors in pbjson_build")?;
            if pbjson_ignore_unknown_fields {
                builder.ignore_unknown_fields();
            }
            if pbjson_preserve_proto_field_names {
                builder.preserve_proto_field_names();
            }
            builder
                .build(&["."])
                .context("Failed to build descriptor set")?;
        }

        Ok(())
    }
}

pub fn configure() -> XaiProtoBuilder {
    let builder = tonic_prost_build::configure()
        .compile_well_known_types(true)
        .extern_path(".google.protobuf", "::pbjson_types")
        .extern_path(".google.protobuf.Empty", "()")
        .protoc_arg("--experimental_allow_proto3_optional");
    XaiProtoBuilder {
        builder,
        gen_pbjson: false,
        pbjson_ignore_unknown_fields: false,
        pbjson_preserve_proto_field_names: false,
        file_descriptor_set_path: None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_dependency_file;

    /// Windows: the target and the deps are absolute paths containing a drive
    /// letter. This is the case that a `split(':')` implementation gets wrong —
    /// it would truncate the target to `C` and fail the prefix check.
    #[test]
    fn parses_windows_depfile_with_drive_letters() {
        let target = r"C:\Temp\build\descriptor.pbbin";
        // Trailing `\` is the makefile line-continuation marker.
        let contents = [
            r"C:\Temp\build\descriptor.pbbin: \",
            r" C:\src\proto\grok-tools.proto \",
            r" C:\src\proto\common.proto",
        ]
        .join("\n");

        let deps = parse_dependency_file(target, &contents).unwrap();

        assert_eq!(
            deps,
            vec![
                r"C:\src\proto\grok-tools.proto",
                r"C:\src\proto\common.proto",
            ],
        );
    }

    /// Unix: the pre-existing shape, kept working.
    #[test]
    fn parses_unix_depfile() {
        let target = "/tmp/build/descriptor.pbbin";
        let contents = [
            r"/tmp/build/descriptor.pbbin: \",
            r" proto/grok-tools.proto \",
            r" proto/common.proto",
        ]
        .join("\n");

        let deps = parse_dependency_file(target, &contents).unwrap();

        assert_eq!(deps, vec!["proto/grok-tools.proto", "proto/common.proto"]);
    }

    /// A dependency listed on the same line as the target is still captured —
    /// protoc does this when there is exactly one dependency.
    #[test]
    fn parses_dependency_on_target_line() {
        let deps =
            parse_dependency_file("/tmp/d.pbbin", "/tmp/d.pbbin: proto/only.proto\n").unwrap();
        assert_eq!(deps, vec!["proto/only.proto"]);
    }

    /// protoc's bundled well-known types are filtered on both separator styles,
    /// since their absolute paths differ per host and would make the emitted
    /// `rerun-if-changed` set non-deterministic.
    #[test]
    fn skips_well_known_types_on_both_separator_styles() {
        let contents = [
            r"/tmp/d.pbbin: \",
            r" proto/grok-tools.proto \",
            r" /opt/protobuf/include/google/protobuf/timestamp.proto \",
            r" C:\dotslash\cache\include\google\protobuf\duration.proto",
        ]
        .join("\n");

        let deps = parse_dependency_file("/tmp/d.pbbin", &contents).unwrap();

        assert_eq!(deps, vec!["proto/grok-tools.proto"]);
    }

    /// A depfile whose target is not the descriptor we asked for means we are
    /// misreading the output; that must be an error rather than a silent empty
    /// dependency set (which would disable rebuild-on-change).
    #[test]
    fn rejects_mismatched_target() {
        let err = parse_dependency_file("/tmp/expected.pbbin", "/tmp/other.pbbin: a.proto\n")
            .unwrap_err();
        assert!(
            err.to_string().contains("must start with"),
            "unexpected error: {err}",
        );
    }

    #[test]
    fn rejects_empty_output() {
        assert!(parse_dependency_file("/tmp/d.pbbin", "").is_err());
    }
}
