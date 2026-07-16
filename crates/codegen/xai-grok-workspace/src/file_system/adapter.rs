//! AcpFsAdapter: implements `xai-grok-tools::AsyncFileSystem` using ACP gateway calls.
//!
//! This adapter enables file tool execution over ACP (remote filesystem).
//! It translates xai-grok-tools' `AsyncFileSystem` trait into ACP protocol calls:
//!   `read_file()` → read_text_file
//!   `write_file()` → write_text_file
//!   `delete_file()` → not supported by ACP (returns error)
//!
//! Mirrors the pattern of `AcpTerminalAdapter` for terminal execution.

use std::path::{Path, PathBuf};

use agent_client_protocol as acp;
use xai_acp_lib::AcpAgentGatewaySender as GatewaySender;
use xai_grok_tools::computer::types::{AsyncFileSystem, ComputerError};

/// Wraps xai-grok-shell's ACP gateway to satisfy xai-grok-tools' AsyncFileSystem.
///
/// When a client advertises `clientCapabilities.fs.readTextFile` and `writeTextFile`,
/// file operations from tools (read_file, search_replace, etc.) are routed through
/// the ACP gateway back to the client instead of hitting the local disk directly.
pub struct AcpFsAdapter {
    gateway: GatewaySender,
    session_id: acp::SessionId,
    /// Shell-host directory containing opaque, session-owned tool-result
    /// artifacts. Only direct children are readable locally; all project paths
    /// remain delegated to the ACP client.
    local_read_root: Option<PathBuf>,
}

impl AcpFsAdapter {
    pub fn new(gateway: GatewaySender, session_id: acp::SessionId) -> Self {
        Self {
            gateway,
            session_id,
            local_read_root: None,
        }
    }

    pub fn with_local_read_root(mut self, root: PathBuf) -> Self {
        self.local_read_root = Some(canonicalize_with_missing_tail(root));
        self
    }

    fn is_local_artifact_path(&self, path: &Path) -> bool {
        self.local_read_root
            .as_deref()
            .is_some_and(|root| is_direct_child(root, path))
    }
}

fn is_direct_child(root: &Path, path: &Path) -> bool {
    path.parent() == Some(root)
}

fn canonicalize_with_missing_tail(path: PathBuf) -> PathBuf {
    let mut existing = path.as_path();
    let mut missing = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name() else {
            return path;
        };
        missing.push(name.to_os_string());
        let Some(parent) = existing.parent() else {
            return path;
        };
        existing = parent;
    }
    let Ok(canonical) = std::fs::canonicalize(existing) else {
        return path;
    };
    let mut resolved = dunce::simplified(&canonical).to_path_buf();
    for component in missing.iter().rev() {
        resolved.push(component);
    }
    resolved
}

#[async_trait::async_trait]
impl AsyncFileSystem for AcpFsAdapter {
    async fn read_file(&self, path: &Path) -> Result<Vec<u8>, ComputerError> {
        if self.is_local_artifact_path(path) {
            return tokio::fs::read(path)
                .await
                .map_err(|error| ComputerError::io_with_kind(error.to_string(), error.kind()));
        }
        let read_req = acp::ReadTextFileRequest::new(self.session_id.clone(), path.to_path_buf());

        let response = self
            .gateway
            .send(read_req)
            .await
            .map_err(acp_error_to_computer_error)?;

        Ok(response.content.into_bytes())
    }

    async fn write_file(&self, path: &Path, data: &[u8]) -> Result<(), ComputerError> {
        let content =
            String::from_utf8(data.to_vec()).map_err(|e| ComputerError::io(e.to_string()))?;

        let write_req =
            acp::WriteTextFileRequest::new(self.session_id.clone(), path.to_path_buf(), content);

        self.gateway
            .send(write_req)
            .await
            .map_err(acp_error_to_computer_error)?;

        Ok(())
    }

    async fn delete_file(&self, path: &Path) -> Result<(), ComputerError> {
        // ACP protocol doesn't support file deletion yet
        tracing::warn!(?path, "ACP filesystem does not support file deletion");
        Err(ComputerError::io("File deletion not supported via ACP"))
    }
}

#[cfg(test)]
mod local_artifact_tests {
    use super::*;

    #[test]
    fn only_direct_session_artifacts_are_host_local() {
        let root = Path::new("/private/session/tool-results");
        assert!(is_direct_child(root, &root.join("result.txt")));
        assert!(!is_direct_child(root, &root.join("nested/result.txt")));
        assert!(!is_direct_child(root, &root.join("../secret.txt")));
        assert!(!is_direct_child(root, Path::new("/workspace/source.rs")));
    }

    #[cfg(unix)]
    #[test]
    fn missing_artifact_root_is_resolved_through_symlinked_parent() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let actual = temp.path().join("actual");
        let linked = temp.path().join("linked");
        std::fs::create_dir_all(actual.join("session")).unwrap();
        symlink(&actual, &linked).unwrap();
        let logical_root = linked.join("session/tool-results");
        let resolved_root = canonicalize_with_missing_tail(logical_root);
        let canonical_session = std::fs::canonicalize(actual.join("session")).unwrap();
        assert_eq!(resolved_root, canonical_session.join("tool-results"));
        assert!(is_direct_child(
            &resolved_root,
            &canonical_session.join("tool-results/result.txt")
        ));
    }

    #[cfg(windows)]
    #[test]
    fn canonical_artifact_root_uses_non_verbatim_windows_path() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("session/tool-results");
        std::fs::create_dir_all(root.parent().unwrap()).unwrap();
        let resolved = canonicalize_with_missing_tail(root);
        assert!(!resolved.to_string_lossy().starts_with(r"\\?\"));
    }
}

fn acp_error_to_computer_error(err: acp::Error) -> ComputerError {
    match acp_error_to_io_kind(&err) {
        Some(kind) => ComputerError::io_with_kind(err.to_string(), kind),
        None => ComputerError::io(err.to_string()),
    }
}

fn acp_error_to_io_kind(err: &acp::Error) -> Option<std::io::ErrorKind> {
    let msg_lower = err.message.to_ascii_lowercase();

    if err.code == acp::ErrorCode::ResourceNotFound {
        Some(std::io::ErrorKind::NotFound)
    } else if msg_lower.contains("permission denied") {
        Some(std::io::ErrorKind::PermissionDenied)
    } else {
        None
    }
}
