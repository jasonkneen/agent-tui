//! Persistent credential storage for MCP server OAuth tokens.
//!
//! Credentials are stored in `$GROK_HOME/mcp_credentials.json`, keyed by a
//! composite key derived from the server name and URL. This keeps MCP OAuth
//! tokens isolated from the user's xAI auth (`auth.json`).
//!
//! Stores rmcp's `StoredCredentials` type directly — the same type that
//! rmcp's `AuthorizationManager` uses internally.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::rmcp;

type Result<T> = std::result::Result<T, McpCredentialError>;

#[derive(Debug, thiserror::Error)]
pub enum McpCredentialError {
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

/// File name for the credential store inside `$GROK_HOME`.
const CREDENTIALS_FILENAME: &str = "mcp_credentials.json";

/// On-disk credential store: `$GROK_HOME/mcp_credentials.json`.
///
/// Stores rmcp `StoredCredentials` per MCP server, keyed by
/// `"{server_name}:{server_url}"`.
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct McpCredentialStore {
    #[serde(flatten)]
    entries: BTreeMap<String, rmcp::transport::auth::StoredCredentials>,
}

impl std::fmt::Debug for McpCredentialStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpCredentialStore")
            .field("entry_count", &self.entries.len())
            .finish()
    }
}

impl McpCredentialStore {
    /// Build the composite key for a credential entry.
    pub fn key(server_name: &str, server_url: &Url) -> String {
        format!("{}:{}", server_name, server_url)
    }

    /// Load the credential store from the default path (`$GROK_HOME/mcp_credentials.json`).
    ///
    /// Returns an empty store if the file does not exist.
    pub fn load_default() -> Result<Self> {
        match Self::default_path() {
            Some(path) => Self::load_from(&path),
            None => Ok(Self::default()),
        }
    }

    /// Load from a specific path.
    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        let store: McpCredentialStore = serde_json::from_str(&content)?;
        Ok(store)
    }

    /// Atomically insert a credential and save — safe for concurrent use.
    pub fn insert_and_save(
        &mut self,
        server_name: &str,
        server_url: &url::Url,
        creds: rmcp::transport::auth::StoredCredentials,
    ) -> Result<()> {
        self.mutate_default(|store| store.insert_rmcp(server_name, server_url, creds))
    }

    /// Atomically remove the credentials for one server name + URL and save.
    /// Other entries written by concurrent processes are preserved.
    pub fn remove_and_save(&mut self, server_name: &str, server_url: &Url) -> Result<bool> {
        let path = Self::default_path().ok_or_else(|| {
            McpCredentialError::Other("no user grok home (set $GROK_HOME or $HOME)".into())
        })?;
        self.remove_and_save_to(&path, server_name, server_url)
    }

    fn remove_and_save_to(
        &mut self,
        path: &Path,
        server_name: &str,
        server_url: &Url,
    ) -> Result<bool> {
        self.mutate_and_save_to(path, |store| {
            store
                .entries
                .remove(&Self::key(server_name, server_url))
                .is_some()
        })
    }

    /// Atomically remove every credential for a server name. This preserves the
    /// legacy config-removal behavior while ensuring it cannot overwrite a
    /// concurrent token refresh. Prefer [`Self::remove_and_save`] when the URL
    /// is known.
    pub fn remove_all_by_server_name_and_save(&mut self, server_name: &str) -> Result<usize> {
        let path = Self::default_path().ok_or_else(|| {
            McpCredentialError::Other("no user grok home (set $GROK_HOME or $HOME)".into())
        })?;
        self.remove_all_by_server_name_and_save_to(&path, server_name)
    }

    fn remove_all_by_server_name_and_save_to(
        &mut self,
        path: &Path,
        server_name: &str,
    ) -> Result<usize> {
        self.mutate_and_save_to(path, |store| store.remove_by_server_name(server_name))
    }

    fn mutate_default<T>(&mut self, mutate: impl FnOnce(&mut Self) -> T) -> Result<T> {
        let path = Self::default_path().ok_or_else(|| {
            McpCredentialError::Other("no user grok home (set $GROK_HOME or $HOME)".into())
        })?;
        self.mutate_and_save_to(&path, mutate)
    }

    /// Serialize a read-modify-write transaction across processes. The lock is
    /// cross-platform (`fs2` uses `flock`/`LockFileEx`) and every mutation
    /// reloads from disk while holding it, so stale adapter instances cannot
    /// overwrite credentials saved by another process.
    fn mutate_and_save_to<T>(
        &mut self,
        path: &Path,
        mutate: impl FnOnce(&mut Self) -> T,
    ) -> Result<T> {
        let lock_path = path.with_extension("lock");
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(lock_path)?;
        fs2::FileExt::lock_exclusive(&lock_file)?;

        let mut fresh = Self::load_from(path)?;
        let result = mutate(&mut fresh);
        fresh.save_to(path)?;
        *self = fresh;
        Ok(result)
    }

    /// Save to a specific path.
    ///
    /// Writes atomically via temp file + rename to prevent credential loss on
    /// crash. The temp file is restricted to the current user before credential
    /// bytes are written (mode 0600 on Unix, a protected DACL on Windows).
    fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        #[cfg(not(windows))]
        let mut tmp = tempfile::Builder::new()
            .prefix(".mcp_credentials-")
            .suffix(".tmp")
            .tempfile_in(parent)?;
        #[cfg(windows)]
        let mut tmp = create_secure_windows_tempfile(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tmp.as_file()
                .set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        use std::io::Write;
        tmp.write_all(content.as_bytes())?;
        tmp.as_file_mut().sync_all()?;
        tmp.persist(path).map_err(|e| e.error)?;
        Ok(())
    }

    /// Look up credentials for a server.
    pub fn get(
        &self,
        server_name: &str,
        server_url: &Url,
    ) -> Option<&rmcp::transport::auth::StoredCredentials> {
        self.entries.get(&Self::key(server_name, server_url))
    }

    /// Insert rmcp `StoredCredentials` for a server.
    fn insert_rmcp(
        &mut self,
        server_name: &str,
        server_url: &Url,
        creds: rmcp::transport::auth::StoredCredentials,
    ) {
        self.entries
            .insert(Self::key(server_name, server_url), creds);
    }

    /// Check if credentials exist for a server (regardless of expiry).
    pub fn has_credentials(&self, server_name: &str, server_url: &Url) -> bool {
        self.entries
            .contains_key(&Self::key(server_name, server_url))
    }

    /// Remove all credentials for a server by name (any URL).
    fn remove_by_server_name(&mut self, server_name: &str) -> usize {
        let prefix = format!("{server_name}:");
        let before = self.entries.len();
        self.entries.retain(|k, _| !k.starts_with(&prefix));
        before - self.entries.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Default path: `$GROK_HOME/mcp_credentials.json`.
    fn default_path() -> Option<PathBuf> {
        Some(xai_grok_config::user_grok_home()?.join(CREDENTIALS_FILENAME))
    }
}

/// Create a credential tempfile atomically with a protected DACL that grants
/// access only to the current user. Passing the security descriptor to
/// `CreateFileW` avoids a create-then-harden window in a permissive GROK_HOME.
#[cfg(windows)]
fn create_secure_windows_tempfile(parent: &Path) -> std::io::Result<tempfile::NamedTempFile> {
    tempfile::Builder::new()
        .prefix(".mcp_credentials-")
        .suffix(".tmp")
        .make_in(parent, create_secure_windows_file)
}

#[cfg(windows)]
fn create_secure_windows_file(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows::Win32::Foundation::{CloseHandle, HLOCAL, LocalFree};
    use windows::Win32::Security::Authorization::{
        EXPLICIT_ACCESS_W, SET_ACCESS, SetEntriesInAclW, TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
    };
    use windows::Win32::Security::{
        ACE_FLAGS, ACL, GetTokenInformation, InitializeSecurityDescriptor, PSECURITY_DESCRIPTOR,
        SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR, SetSecurityDescriptorControl,
        SetSecurityDescriptorDacl, TOKEN_QUERY, TOKEN_USER, TokenUser,
    };
    use windows::Win32::Storage::FileSystem::{
        CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_TEMPORARY, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows::core::PCWSTR;

    unsafe {
        let mut token_handle = windows::Win32::Foundation::HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_handle)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::PermissionDenied, error))?;

        let mut new_acl: *mut ACL = std::ptr::null_mut();
        let result = (|| {
            let mut return_length = 0;
            let _ = GetTokenInformation(token_handle, TokenUser, None, 0, &mut return_length);
            let word_count = (return_length as usize).div_ceil(std::mem::size_of::<usize>());
            let mut token_user_buffer = vec![0_usize; word_count];
            GetTokenInformation(
                token_handle,
                TokenUser,
                Some(token_user_buffer.as_mut_ptr().cast()),
                return_length,
                &mut return_length,
            )
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::PermissionDenied, error))?;

            // Vec<usize> provides sufficient alignment for TOKEN_USER.
            let token_user = &*token_user_buffer.as_ptr().cast::<TOKEN_USER>();
            let explicit_access = EXPLICIT_ACCESS_W {
                grfAccessPermissions: 0x10000000, // GENERIC_ALL
                grfAccessMode: SET_ACCESS,
                grfInheritance: ACE_FLAGS(0),
                Trustee: TRUSTEE_W {
                    pMultipleTrustee: std::ptr::null_mut(),
                    MultipleTrusteeOperation:
                        windows::Win32::Security::Authorization::NO_MULTIPLE_TRUSTEE,
                    TrusteeForm: TRUSTEE_IS_SID,
                    TrusteeType: TRUSTEE_IS_USER,
                    ptstrName: windows::core::PWSTR(token_user.User.Sid.0 as *mut u16),
                },
            };
            let acl_result = SetEntriesInAclW(Some(&[explicit_access]), None, &mut new_acl);
            if acl_result.0 != 0 {
                return Err(std::io::Error::from_raw_os_error(acl_result.0 as i32));
            }

            let mut descriptor = SECURITY_DESCRIPTOR::default();
            let descriptor_ptr =
                PSECURITY_DESCRIPTOR((&mut descriptor as *mut SECURITY_DESCRIPTOR).cast());
            InitializeSecurityDescriptor(descriptor_ptr, 1).map_err(std::io::Error::other)?;
            SetSecurityDescriptorDacl(descriptor_ptr, true, Some(new_acl), false)
                .map_err(std::io::Error::other)?;
            SetSecurityDescriptorControl(descriptor_ptr, SE_DACL_PROTECTED, SE_DACL_PROTECTED)
                .map_err(std::io::Error::other)?;

            let security_attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor_ptr.0,
                bInheritHandle: false.into(),
            };
            let wide_path: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let handle = CreateFileW(
                PCWSTR::from_raw(wide_path.as_ptr()),
                FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
                FILE_SHARE_DELETE | FILE_SHARE_READ | FILE_SHARE_WRITE,
                Some(&security_attributes),
                CREATE_NEW,
                FILE_ATTRIBUTE_TEMPORARY,
                None,
            )
            .map_err(windows_error_to_io)?;

            Ok(std::fs::File::from_raw_handle(handle.0))
        })();

        let _ = LocalFree(Some(HLOCAL(new_acl.cast())));
        let _ = CloseHandle(token_handle);
        result
    }
}

#[cfg(windows)]
fn windows_error_to_io(error: windows::core::Error) -> std::io::Error {
    // Win32 APIs exposed by windows-rs encode GetLastError in an HRESULT.
    // Recover the original code so std::io maps filename collisions to
    // ErrorKind::AlreadyExists and tempfile can retry with a new name.
    let hresult = error.code().0 as u32;
    let raw_code = if hresult & 0xffff_0000 == 0x8007_0000 {
        hresult & 0xffff
    } else {
        hresult
    };
    std::io::Error::from_raw_os_error(raw_code as i32)
}

/// Adapter implementing rmcp's `CredentialStore` trait backed by the on-disk
/// `McpCredentialStore`. Each adapter instance is scoped to a single MCP server
/// (keyed by name + URL); rmcp's `AuthorizationManager` calls load/save/clear
/// transparently during token exchange and refresh.
pub struct McpCredentialStoreAdapter {
    server_name: String,
    server_url: url::Url,
}

impl McpCredentialStoreAdapter {
    pub fn new(server_name: String, server_url: url::Url) -> Self {
        Self {
            server_name,
            server_url,
        }
    }
}

#[async_trait::async_trait]
impl rmcp::transport::auth::CredentialStore for McpCredentialStoreAdapter {
    async fn load(
        &self,
    ) -> std::result::Result<
        Option<rmcp::transport::auth::StoredCredentials>,
        rmcp::transport::auth::AuthError,
    > {
        let name = self.server_name.clone();
        let url = self.server_url.clone();
        tokio::task::spawn_blocking(move || {
            let store = McpCredentialStore::load_default()
                .map_err(|e| rmcp::transport::auth::AuthError::InternalError(e.to_string()))?;
            Ok(store.get(&name, &url).cloned())
        })
        .await
        .map_err(|e| rmcp::transport::auth::AuthError::InternalError(e.to_string()))?
    }

    async fn save(
        &self,
        credentials: rmcp::transport::auth::StoredCredentials,
    ) -> std::result::Result<(), rmcp::transport::auth::AuthError> {
        let name = self.server_name.clone();
        let url = self.server_url.clone();
        tokio::task::spawn_blocking(move || {
            let mut store = McpCredentialStore::load_default().unwrap_or_default();
            store
                .insert_and_save(&name, &url, credentials)
                .map_err(|e| rmcp::transport::auth::AuthError::InternalError(e.to_string()))
        })
        .await
        .map_err(|e| rmcp::transport::auth::AuthError::InternalError(e.to_string()))?
    }

    async fn clear(&self) -> std::result::Result<(), rmcp::transport::auth::AuthError> {
        let name = self.server_name.clone();
        let url = self.server_url.clone();
        tokio::task::spawn_blocking(move || {
            let mut store = McpCredentialStore::load_default().unwrap_or_default();
            store
                .remove_and_save(&name, &url)
                .map(|_| ())
                .map_err(|e| rmcp::transport::auth::AuthError::InternalError(e.to_string()))
        })
        .await
        .map_err(|e| rmcp::transport::auth::AuthError::InternalError(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_stored_creds(client_id: &str) -> rmcp::transport::auth::StoredCredentials {
        rmcp::transport::auth::StoredCredentials::new(client_id.to_string(), None, Vec::new(), None)
    }

    #[test]
    fn insert_and_get() {
        let mut store = McpCredentialStore::default();
        let url = Url::parse("https://test.example.com/mcp").unwrap();
        store.insert_rmcp("test", &url, test_stored_creds("test-client"));
        assert!(store.get("test", &url).is_some());
        assert_eq!(store.get("test", &url).unwrap().client_id, "test-client");
    }

    #[test]
    fn has_credentials() {
        let mut store = McpCredentialStore::default();
        let url = Url::parse("https://test.example.com/mcp").unwrap();
        assert!(!store.has_credentials("test", &url));
        store.insert_rmcp("test", &url, test_stored_creds("c"));
        assert!(store.has_credentials("test", &url));
    }

    #[test]
    fn roundtrip_serialization() {
        let mut store = McpCredentialStore::default();
        let url = Url::parse("https://test.example.com/mcp").unwrap();
        store.insert_rmcp("test", &url, test_stored_creds("test-client"));

        let json = serde_json::to_string(&store).unwrap();
        let loaded: McpCredentialStore = serde_json::from_str(&json).unwrap();
        assert!(loaded.get("test", &url).is_some());
    }

    /// Raw JSON fixture in the exact shape rmcp 0.17 persisted to
    /// `$GROK_HOME/mcp_credentials.json`. Existing credential files must keep
    /// loading across rmcp upgrades (2.1's `OAuthTokenResponse` gained vendor
    /// extra token fields), so this must be a string literal — never JSON
    /// serialized by the current code.
    #[test]
    fn legacy_on_disk_fixture_still_deserializes() {
        use oauth2::TokenResponse as _;

        let fixture = r#"{
            "linear:https://mcp.example.com/mcp": {
                "client_id": "legacy-client-id",
                "token_response": {
                    "access_token": "at-123",
                    "token_type": "bearer",
                    "expires_in": 3600,
                    "refresh_token": "rt-456",
                    "scope": "read write"
                },
                "granted_scopes": ["read", "write"],
                "token_received_at": 1730000000
            },
            "noauth:https://example.com/mcp": {
                "client_id": "c2",
                "token_response": null
            }
        }"#;

        let store: McpCredentialStore = serde_json::from_str(fixture).unwrap();
        let url = Url::parse("https://mcp.example.com/mcp").unwrap();
        let creds = store.get("linear", &url).expect("legacy entry loads");
        assert_eq!(creds.client_id, "legacy-client-id");
        let token = creds.token_response.as_ref().expect("token loads");
        assert_eq!(token.access_token().secret(), "at-123");
        assert_eq!(token.refresh_token().unwrap().secret(), "rt-456");
        assert_eq!(creds.granted_scopes, vec!["read", "write"]);
        assert_eq!(creds.token_received_at, Some(1730000000));

        // Entry without the `#[serde(default)]` fields on disk still loads.
        let url2 = Url::parse("https://example.com/mcp").unwrap();
        let creds2 = store.get("noauth", &url2).expect("minimal entry loads");
        assert!(creds2.token_response.is_none());
        assert!(creds2.granted_scopes.is_empty());
        assert!(creds2.token_received_at.is_none());

        // Round-trip through the current serializer and reload.
        let json = serde_json::to_string(&store).unwrap();
        let reloaded: McpCredentialStore = serde_json::from_str(&json).unwrap();
        let re = reloaded
            .get("linear", &url)
            .expect("round-trip keeps entry");
        assert_eq!(re.client_id, "legacy-client-id");
        let re_token = re.token_response.as_ref().expect("round-trip keeps token");
        assert_eq!(re_token.access_token().secret(), "at-123");
        assert_eq!(re_token.refresh_token().unwrap().secret(), "rt-456");
        assert_eq!(re.granted_scopes, vec!["read", "write"]);
        assert_eq!(re.token_received_at, Some(1730000000));
    }

    #[test]
    fn save_and_load_from_file() {
        let dir = std::env::temp_dir().join("grok-mcp-credentials-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_creds.json");

        let mut store = McpCredentialStore::default();
        let url = Url::parse("https://test.example.com/mcp").unwrap();
        store.insert_rmcp("test", &url, test_stored_creds("test-client"));
        store.save_to(&path).unwrap();

        let loaded = McpCredentialStore::load_from(&path).unwrap();
        assert!(loaded.get("test", &url).is_some());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn locked_mutations_merge_stale_store_instances() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        let first_url = Url::parse("https://first.example.com/mcp").unwrap();
        let second_url = Url::parse("https://second.example.com/mcp").unwrap();
        let mut first = McpCredentialStore::default();
        let mut stale_second = McpCredentialStore::default();

        first
            .mutate_and_save_to(&path, |store| {
                store.insert_rmcp("first", &first_url, test_stored_creds("first-client"));
            })
            .unwrap();
        stale_second
            .mutate_and_save_to(&path, |store| {
                store.insert_rmcp("second", &second_url, test_stored_creds("second-client"));
            })
            .unwrap();

        let merged = McpCredentialStore::load_from(&path).unwrap();
        assert!(merged.has_credentials("first", &first_url));
        assert!(merged.has_credentials("second", &second_url));
    }

    #[test]
    fn concurrent_process_mutations_preserve_both_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        let ready_dir = dir.path().join("ready");
        std::fs::create_dir(&ready_dir).unwrap();
        let current_exe = std::env::current_exe().unwrap();
        let mut children = Vec::new();
        for index in 0..2 {
            children.push(
                std::process::Command::new(&current_exe)
                    .args([
                        "credentials::tests::credential_mutation_child",
                        "--ignored",
                        "--exact",
                    ])
                    .env("GROK_MCP_CREDENTIAL_CHILD_PATH", &path)
                    .env("GROK_MCP_CREDENTIAL_CHILD_INDEX", index.to_string())
                    .env("GROK_MCP_CREDENTIAL_CHILD_READY_DIR", &ready_dir)
                    .spawn()
                    .unwrap(),
            );
        }
        let mut children = ChildCleanup(children);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        wait_for_file(&ready_dir.join("ready-0"), deadline, "child 0 readiness");
        wait_for_file(&ready_dir.join("ready-1"), deadline, "child 1 readiness");
        std::fs::write(ready_dir.join("go-0"), b"go").unwrap();
        wait_for_file(
            &ready_dir.join("entered-0"),
            deadline,
            "child 0 transaction entry",
        );
        std::fs::write(ready_dir.join("go-1"), b"go").unwrap();
        children.wait_success(deadline);

        let merged = McpCredentialStore::load_from(&path).unwrap();
        for index in 0..2 {
            let name = format!("server-{index}");
            let url = Url::parse(&format!("https://server-{index}.example.com/mcp")).unwrap();
            assert!(merged.has_credentials(&name, &url));
        }
    }

    #[test]
    #[ignore = "launched as a subprocess by concurrent_process_mutations_preserve_both_entries"]
    fn credential_mutation_child() {
        let path = PathBuf::from(std::env::var_os("GROK_MCP_CREDENTIAL_CHILD_PATH").unwrap());
        let index = std::env::var("GROK_MCP_CREDENTIAL_CHILD_INDEX").unwrap();
        let ready_dir =
            PathBuf::from(std::env::var_os("GROK_MCP_CREDENTIAL_CHILD_READY_DIR").unwrap());
        let go_path = ready_dir.join(format!("go-{index}"));
        std::fs::write(ready_dir.join(format!("ready-{index}")), b"ready").unwrap();
        while !go_path.exists() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let name = format!("server-{index}");
        let url = Url::parse(&format!("https://server-{index}.example.com/mcp")).unwrap();
        let mut store = McpCredentialStore::default();
        store
            .mutate_and_save_to(&path, |fresh| {
                std::fs::write(ready_dir.join(format!("entered-{index}")), b"entered").unwrap();
                if index == "0" {
                    // With the process lock, child 1 cannot enter until this
                    // transaction commits. Without it, both children load the
                    // empty store and this handshake deterministically exposes
                    // the lost update.
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
                    while !ready_dir.join("entered-1").exists()
                        && std::time::Instant::now() < deadline
                    {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                } else {
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                    wait_for_file(&ready_dir.join("committed-0"), deadline, "child 0 commit");
                }
                fresh.insert_rmcp(&name, &url, test_stored_creds(&name));
            })
            .unwrap();
        std::fs::write(ready_dir.join(format!("committed-{index}")), b"committed").unwrap();
    }

    fn wait_for_file(path: &Path, deadline: std::time::Instant, description: &str) {
        while !path.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for {description}"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    struct ChildCleanup(Vec<std::process::Child>);

    impl ChildCleanup {
        fn wait_success(&mut self, deadline: std::time::Instant) {
            let mut completed = vec![false; self.0.len()];
            while completed.iter().any(|done| !done) {
                assert!(
                    std::time::Instant::now() < deadline,
                    "credential mutation subprocesses timed out"
                );
                for (index, child) in self.0.iter_mut().enumerate() {
                    if completed[index] {
                        continue;
                    }
                    if let Some(status) = child.try_wait().unwrap() {
                        assert!(status.success(), "credential child {index} failed");
                        completed[index] = true;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }

    impl Drop for ChildCleanup {
        fn drop(&mut self) {
            for child in &mut self.0 {
                if child.try_wait().ok().flatten().is_none() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
    }

    #[test]
    fn locked_remove_all_preserves_other_names() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        let first_url = Url::parse("https://first.example.com/mcp").unwrap();
        let second_url = Url::parse("https://second.example.com/mcp").unwrap();
        let other_url = Url::parse("https://other.example.com/mcp").unwrap();
        let mut store = McpCredentialStore::default();
        store
            .mutate_and_save_to(&path, |fresh| {
                fresh.insert_rmcp("shared", &first_url, test_stored_creds("first"));
                fresh.insert_rmcp("shared", &second_url, test_stored_creds("second"));
                fresh.insert_rmcp("other", &other_url, test_stored_creds("other"));
            })
            .unwrap();

        let removed = store
            .remove_all_by_server_name_and_save_to(&path, "shared")
            .unwrap();

        assert_eq!(removed, 2);
        let persisted = McpCredentialStore::load_from(&path).unwrap();
        assert!(!persisted.has_credentials("shared", &first_url));
        assert!(!persisted.has_credentials("shared", &second_url));
        assert!(persisted.has_credentials("other", &other_url));
    }

    #[test]
    fn locked_mutation_does_not_overwrite_malformed_store() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        std::fs::write(&path, "{ malformed").unwrap();
        let mut store = McpCredentialStore::default();
        let url = Url::parse("https://new.example.com/mcp").unwrap();

        let result = store.mutate_and_save_to(&path, |fresh| {
            fresh.insert_rmcp("new", &url, test_stored_creds("new-client"));
        });

        assert!(matches!(result, Err(McpCredentialError::Json(_))));
        assert_eq!(std::fs::read_to_string(path).unwrap(), "{ malformed");
    }

    #[test]
    fn locked_remove_preserves_entries_added_after_stale_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        let removed_url = Url::parse("https://remove.example.com/mcp").unwrap();
        let preserved_url = Url::parse("https://preserve.example.com/mcp").unwrap();
        let mut seed = McpCredentialStore::default();
        seed.mutate_and_save_to(&path, |store| {
            store.insert_rmcp("shared", &removed_url, test_stored_creds("removed-client"));
        })
        .unwrap();
        let mut stale_remover = McpCredentialStore::load_from(&path).unwrap();

        let mut concurrent_writer = McpCredentialStore::default();
        concurrent_writer
            .mutate_and_save_to(&path, |store| {
                store.insert_rmcp(
                    "shared",
                    &preserved_url,
                    test_stored_creds("preserved-client"),
                );
            })
            .unwrap();
        let removed = stale_remover
            .mutate_and_save_to(&path, |store| {
                store
                    .entries
                    .remove(&McpCredentialStore::key("shared", &removed_url))
                    .is_some()
            })
            .unwrap();

        assert!(removed);
        let final_store = McpCredentialStore::load_from(&path).unwrap();
        assert!(!final_store.has_credentials("shared", &removed_url));
        assert!(final_store.has_credentials("shared", &preserved_url));
    }
}
