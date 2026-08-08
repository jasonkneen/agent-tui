//! Detect file reads/writes inside a shell command so a managed `Read`/`Edit`
//! deny/ask can't be bypassed via a shell reader/writer/redirect.

use std::path::{Path, PathBuf};

use tree_sitter::Node;

use crate::permission::bash_command_splitting::{
    try_parse_shell, unwrap_wrappers, wrapper_has_chdir,
};
use crate::permission::policy::CompiledPolicy;
use crate::permission::types::{AccessKind, Decision};

impl CompiledPolicy {
    /// Escalate (never auto-allow) a shell reader/writer/redirect touching a
    /// restricted path; unpinnable operands return `Ask`.
    pub fn evaluate_shell_file_access(&self, cmd: &str, cwd: &Path) -> Option<Decision> {
        if !self.has_file_restrictions {
            return None;
        }
        // Parser always yields a tree; real syntax errors surface via `has_error`.
        let tree = try_parse_shell(cmd)?;
        let root = tree.root_node();
        // Syntax errors make operands untrustworthy → prompt.
        let parse_failed = root.has_error();
        let mut forced_ask = false;
        // Strongest outcome wins: a deny beats an earlier ask.
        let mut decision: Option<Decision> = None;

        let invocations = shell_command_invocations(root, cmd);

        // We don't track cwd across `cd`/`pushd`/`env -C`; a relative operand after
        // one is unpinnable → Ask. Managed denies are `**/` basename globs, so they
        // still match — only exact-path rules are affected.
        let cwd_changes = cwd_poison_positions(root, cmd);

        // Redirects from the AST cover glued/fd-prefixed forms.
        for (start_byte, path, mode, ambiguous) in shell_redirect_targets(root, cmd) {
            if ambiguous {
                forced_ask = true;
            }
            if let Some(path) = path {
                if cwd_unpinned_before(&cwd_changes, start_byte)
                    && !is_absolute_shell_path(&normalize_shell_path(&path))
                {
                    forced_ask = true;
                }
                decision = combine_decisions(decision, self.evaluate_shell_path(&path, cwd, mode));
            }
        }

        // A known reader/writer with an unpinnable operand prompts.
        for (start_byte, raw_words, arg_ambiguous) in &invocations {
            let words = unwrap_wrappers(raw_words);
            let Some(program) = words.first().map(|w| shell_program_name(w)) else {
                continue;
            };
            let program_lower = program.to_ascii_lowercase();
            if program_lower == "cd" {
                continue;
            }
            // Unpinnable after a preceding `cd`/`pushd`, or under an `env -C`.
            let cwd_unpinned =
                cwd_unpinned_before(&cwd_changes, *start_byte) || wrapper_has_chdir(raw_words);
            let candidates = shell_file_candidates(words);
            let path_operands = shell_path_command_operands(&program_lower, words);
            let is_known = program_lower == "dd"
                || shell_file_mode(&program_lower).is_some()
                || path_operands.is_some();
            if is_known && (cwd_unpinned || *arg_ambiguous || parse_failed) {
                forced_ask = true;
            }
            // Operands named by flag, not position — the positional loop below misses these.
            for (path, mode) in special_file_operands(&program_lower, words) {
                if shell_arg_is_ambiguous(&path) {
                    forced_ask = true;
                }
                decision = combine_decisions(decision, self.evaluate_shell_path(&path, cwd, mode));
            }
            // dd's only file operands are if=/of=, handled above.
            if program_lower == "dd" {
                continue;
            }
            // Path-moving commands (cp/mv/rm/…) imply Read/Edit on operands.
            if let Some(operands) = path_operands {
                for (path, mode) in operands {
                    if shell_arg_is_ambiguous(path) {
                        forced_ask = true;
                    }
                    decision =
                        combine_decisions(decision, self.evaluate_shell_path(path, cwd, mode));
                }
                continue;
            }
            // In-place sed both reads and rewrites each operand.
            let modes: &[ShellFileMode] = match shell_file_mode(&program_lower) {
                Some(_) if program_lower == "sed" && shell_sed_in_place(words) => {
                    &[ShellFileMode::Read, ShellFileMode::Write]
                }
                Some(ShellFileMode::Read) => &[ShellFileMode::Read],
                Some(ShellFileMode::Write) => &[ShellFileMode::Write],
                None => continue,
            };
            for &token in &candidates {
                if shell_arg_is_ambiguous(token) {
                    forced_ask = true;
                }
                for &mode in modes {
                    decision =
                        combine_decisions(decision, self.evaluate_shell_path(token, cwd, mode));
                }
            }
            if shell_reader_can_recurse(&program_lower, words, &candidates) {
                forced_ask = true;
            }
        }
        combine_decisions(decision, forced_ask.then_some(Decision::Ask))
    }

    fn evaluate_shell_path(
        &self,
        token: &str,
        cwd: &Path,
        mode: ShellFileMode,
    ) -> Option<Decision> {
        let path = normalize_shell_path(token);
        let is_absolute = is_absolute_shell_path(&path);
        // Cwd-aware rule match mirrors the direct Read/Edit tool gate, so a
        // rooted rule like `Read(src/**)` also keys on the same file spelled
        // absolutely. An unpinned cwd anchors nothing: relative operands then
        // keep text-only matching (absolute operands are cwd-independent).
        let rule_cwd = (is_absolute || !cwd_unpinned).then_some(cwd);
        // Escalate only: drop Allow so a file allow-rule can't auto-approve here.
        let escalate = |access: &AccessKind| match self.evaluate_with_cwd(access, rule_cwd) {
            Some(Decision::Reject(reason)) => Some(GateDecision::Reject(reason)),
            Some(Decision::Ask) => Some(GateDecision::AskRuleMatch),
            _ => None,
        };
        // Also re-check the resolved symlink target so a deny keyed on the real
        // path can't be dodged via an in-workspace symlink (`ln -s /etc x`).
        // Resolve the *uncollapsed* operand so a `..` after a link is applied
        // physically, not erased textually before the link is followed.
        let raw = normalize_shell_path_raw(token);
        let raw_absolute = if is_absolute_shell_path(&raw) {
            raw
        } else {
            normalize_shell_path_raw(&cwd.join(&raw).to_string_lossy())
        };
        let resolved_decision = match resolve_symlink_target(&raw_absolute) {
            Some(resolved) if resolved != absolute => escalate(&shell_access(mode, resolved)),
            Some(_) => None,
            // Unresolvable (depth/cycle/error): fail closed to Ask when any
            // component of the operand is a symlink, rather than silently
            // allowing it (covers mid-path chains, not just the leaf).
            None => path_has_symlink(&raw_absolute).then_some(Decision::Ask),
        };
        combine_decisions(
            combine_decisions(
                escalate(&shell_access(mode, path)),
                escalate(&shell_access(mode, absolute)),
            ),
            resolved_decision,
        )
    }
}

/// Write paths from a SINGLE already-split command's words (no redirects — the
/// caller handles those at the tree level). Wrapper-aware. Reused both per parsed
/// command and to re-check the inner command of a package-manager launcher
/// (`uv run`, `npm exec`, ...) whose writes the outer program name would hide.
pub(crate) fn command_words_write_paths(words: &[String]) -> Vec<String> {
    let inner = unwrap_wrappers(words);
    let mut out = Vec::new();
    let Some(program) = inner.first().map(|w| shell_program_name(w)) else {
        return out;
    };
    let program = program.to_ascii_lowercase();

    // Flag-named write operands (`dd of=`, `sort`/`go`/`rustc -o`, `git --output`).
    for (path, mode) in special_file_operands(&program, inner) {
        if matches!(mode, ShellFileMode::Write) {
            out.push(path);
        }
    }
    // Path-moving destinations (`cp`/`mv`/`ln`/`install` dest; `rm`/`touch`/…;
    // `uniq` output operand).
    if let Some(operands) = shell_path_command_operands(&program, inner) {
        for (path, mode) in operands {
            if matches!(mode, ShellFileMode::Write) {
                out.push(path.to_owned());
            }
        }
        return out;
    }
    // Named-argument writers (`tee`/`truncate`/...) and in-place `sed -i`, which
    // rewrites each file operand.
    let writes_operands = matches!(shell_file_mode(&program), Some(ShellFileMode::Write))
        || (program == "sed" && shell_sed_in_place(inner));
    if writes_operands {
        for token in shell_file_candidates(inner) {
            out.push(token.to_owned());
        }
    }
    out
}

/// Every path a shell command WRITES, from an ALREADY-PARSED tree (so a caller
/// that already parsed `src` shares the one parse): output redirects plus the
/// per-command writers from [`command_words_write_paths`] (`dd of=`, `sort -o`,
/// `git --output`, `cp`/`mv` dest, `tee`/`truncate`, in-place `sed`/`rustfmt`,
/// `uniq` output, ...). No safe-sink filtering — the caller decides.
pub(crate) fn command_write_paths_in_tree(root: Node<'_>, src: &str) -> Vec<String> {
    let mut out = Vec::new();

    // Output redirects (`> f`, `>> f`); fd-dups/heredocs are already skipped.
    for (_start, path, mode, _ambiguous) in shell_redirect_targets(root, src) {
        if matches!(mode, ShellFileMode::Write)
            && let Some(path) = path
        {
            out.push(path);
        }
    }
    // Per-command writers, after peeling env/timeout/... wrappers.
    for (_start, raw_words, _ambiguous) in shell_command_invocations(root, src) {
        out.extend(command_words_write_paths(&raw_words));
    }
    out
}

/// Safe write sinks that do not touch a real file. Exact match.
pub(crate) fn is_safe_write_sink(path: &str) -> bool {
    matches!(path, "/dev/null" | "/dev/stdout" | "/dev/stderr")
}

/// Why acceptEdits must still prompt for this edit target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectedEditReason {
    HookRoot,
    GitHooks,
    Ssh,
    StartupFile,
    Etc,
    GrokConfig,
    GrokSandbox,
    ClaudeSettings,
    CursorHooks,
    /// Fail-closed / unclassified sensitive path; no user copy yet.
    Sensitive,
}

impl ProtectedEditReason {
    pub fn kind(self) -> &'static str {
        match self {
            Self::HookRoot => "hook_root",
            Self::GitHooks => "git_hooks",
            Self::Ssh => "ssh",
            Self::StartupFile => "startup_file",
            Self::Etc => "etc",
            Self::GrokConfig => "grok_config",
            Self::GrokSandbox => "grok_sandbox",
            Self::ClaudeSettings => "claude_settings",
            Self::CursorHooks => "cursor_hooks",
            Self::Sensitive => "sensitive",
        }
    }

    pub fn description(self) -> Option<&'static str> {
        match self {
            Self::HookRoot => Some(
                "Note: This edit contains changes to hooks, which can be executed as code on later sessions without a separate execution approval.",
            ),
            Self::GitHooks => Some(
                "Note: This edit contains changes to Git hooks, which can run automatically on commit, push, or other Git actions without a separate execution approval.",
            ),
            Self::Ssh => Some(
                "Note: This edit contains changes under `.ssh`, which can affect credentials and authentication for future sessions.",
            ),
            Self::StartupFile => Some(
                "Note: This edit contains changes to a shell startup file, which can run automatically in future terminals without a separate execution approval.",
            ),
            Self::Etc => Some(
                "Note: This edit contains changes under `/etc`, which is system configuration and can affect this machine beyond the current project.",
            ),
            Self::GrokConfig => Some(
                "Note: This edit contains changes to Grok config, which can alter permissions, tools, and other behavior in later sessions.",
            ),
            Self::GrokSandbox => Some(
                "Note: This edit contains changes to the Grok sandbox config, which can loosen filesystem and network restrictions on commands.",
            ),
            Self::ClaudeSettings => Some(
                "Note: This edit contains changes to Claude-compatible settings, which can install hooks or change permission mode without a separate execution approval.",
            ),
            Self::CursorHooks => Some(
                "Note: This edit contains changes to Cursor hooks, which can run automatically in later sessions without a separate execution approval.",
            ),
            Self::Sensitive => None,
        }
    }
}

/// ACP `_meta` payload for protected-edit prompts (pager reads this for description).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectedEditPermission {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ProtectedEditPermission {
    pub fn from_reason(reason: ProtectedEditReason) -> Self {
        Self {
            kind: reason.kind().to_owned(),
            description: reason.description().map(str::to_owned),
        }
    }
}

/// Whether an already-resolved direct edit target needs confirmation, and why.
///
/// The caller uses the edit tools' shared model-path resolver first. This helper
/// preserves its uncollapsed components for physical symlink + `..` resolution,
/// while checking a separate lexical normalization for traversal aliases.
pub(crate) fn edit_target_protection(path: &Path) -> Option<ProtectedEditReason> {
    if !path.is_absolute() {
        return Some(ProtectedEditReason::Sensitive);
    }
    let lexical = agent_tui_paths::normalize_lexically(path);
    if let Some(reason) = protected_edit_reason(&lexical) {
        return Some(reason);
    }
    let Some(resolved) = resolve_following_symlinks(path, 0) else {
        return Some(ProtectedEditReason::Sensitive);
    };
    if let Some(reason) = protected_edit_reason(&resolved) {
        return Some(reason);
    }
    resolved_path_is_within_root(&resolved, Path::new("/etc"))
        .then_some(ProtectedEditReason::Sensitive)
}

fn protected_edit_reason(path: &Path) -> Option<ProtectedEditReason> {
    let components: Vec<String> = path
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => Some(part.to_string_lossy().to_ascii_lowercase()),
            _ => None,
        })
        .collect();
    let string_components: Vec<&str> = components.iter().map(String::as_str).collect();
    let file = string_components.last().copied().unwrap_or("");
    const STARTUP_FILES: &[&str] = &[
        ".bashrc",
        ".bash_profile",
        ".bash_login",
        ".bash_logout",
        ".profile",
        ".zshrc",
        ".zshenv",
        ".zprofile",
        ".zlogin",
        ".zlogout",
        ".kshrc",
        ".cshrc",
        ".tcshrc",
        ".login",
        ".logout",
        ".inputrc",
        ".xprofile",
    ];

    if protected_grok_hook_root(path, &string_components) {
        return Some(ProtectedEditReason::HookRoot);
    }
    if string_components.ends_with(&[".claude", "settings.json"])
        || string_components.ends_with(&[".claude", "settings.local.json"])
    {
        return Some(ProtectedEditReason::ClaudeSettings);
    }
    if string_components.ends_with(&[".cursor", "hooks.json"]) {
        return Some(ProtectedEditReason::CursorHooks);
    }
    if protected_git_hooks_path(&string_components) {
        return Some(ProtectedEditReason::GitHooks);
    }
    if string_components.contains(&".ssh") {
        return Some(ProtectedEditReason::Ssh);
    }
    if STARTUP_FILES.contains(&file) {
        return Some(ProtectedEditReason::StartupFile);
    }
    if let Some(reason) = protected_grok_config_file(path, &string_components) {
        return Some(reason);
    }
    if path == Path::new("/etc") || path.starts_with(Path::new("/etc")) {
        return Some(ProtectedEditReason::Etc);
    }
    None
}

/// Grok config files that alter permissions (`config.toml`, the
/// `managed_config.toml` defaults tier, the user `requirements.toml` layer) or
/// sandbox restrictions (`sandbox.toml`) in the running and later sessions; a
/// silent edit would let the agent loosen its own guardrails. Matched directly
/// inside any `.grok` dir (user-global default and workspace overlays) and
/// directly under a custom `$GROK_HOME`, which the component match cannot see.
fn protected_grok_config_file(path: &Path, components: &[&str]) -> Option<ProtectedEditReason> {
    protected_grok_config_file_with_home(
        path,
        components,
        agent_tui_config::user_grok_home().as_deref(),
    )
}

fn protected_grok_config_file_with_home(
    path: &Path,
    components: &[&str],
    user_grok_home: Option<&Path>,
) -> Option<ProtectedEditReason> {
    let reason = match components.last().copied() {
        Some(
            agent_tui_config::USER_CONFIG_FILENAME
            | agent_tui_config::MANAGED_CONFIG_FILENAME
            | agent_tui_config::REQUIREMENTS_FILENAME,
        ) => ProtectedEditReason::GrokConfig,
        Some("sandbox.toml") => ProtectedEditReason::GrokSandbox,
        _ => return None,
    };
    let in_dot_grok = components.len() >= 2 && components[components.len() - 2] == ".grok";
    let in_grok_home = || grok_home_matches(user_grok_home, |home| path.parent() == Some(home));
    (in_dot_grok || in_grok_home()).then_some(reason)
}

/// True when `pred` holds for the user grok home in either its lexical or
/// physically-resolved form. Both forms are checked because callers hold a
/// lexical and a resolved candidate path, and the home itself may sit behind a
/// symlink. The comparison is byte-exact (no case folding), like every other
/// resolved-path check in this module.
fn grok_home_matches(home: Option<&Path>, pred: impl Fn(&Path) -> bool) -> bool {
    home.is_some_and(|home| {
        let lexical = agent_tui_paths::normalize_lexically(home);
        pred(&lexical)
            || resolve_following_symlinks(&lexical, 0).is_some_and(|resolved| pred(&resolved))
    })
}

fn path_is_under_user_grok_hook_root(path: &Path, grok_home: &Path) -> bool {
    path.starts_with(grok_home.join("hooks")) || path == grok_home.join("hooks-paths")
}

fn protected_grok_hook_root(path: &Path, components: &[&str]) -> bool {
    components.windows(2).any(|pair| pair == [".grok", "hooks"])
        || components.ends_with(&[".grok", "hooks-paths"])
        || grok_home_matches(agent_tui_config::user_grok_home().as_deref(), |home| {
            path_is_under_user_grok_hook_root(path, home)
        })
}

fn protected_git_hooks_path(components: &[&str]) -> bool {
    components.windows(2).any(|pair| pair == [".git", "hooks"])
        || components.iter().enumerate().any(|(git, component)| {
            *component == ".git"
                && components.get(git + 1) == Some(&"modules")
                && components[git + 2..]
                    .iter()
                    .skip(1)
                    .any(|component| *component == "hooks")
        })
}

/// `resolved_path` is already physical; resolve `root` so platform aliases such
/// as macOS `/etc -> /private/etc` compare in the same namespace. Resolution
/// failure is conservative: the caller then requires confirmation.
fn resolved_path_is_within_root(resolved_path: &Path, root: &Path) -> bool {
    resolve_following_symlinks(root, 0)
        .map(|resolved_root| resolved_path.starts_with(resolved_root))
        .unwrap_or(true)
}

#[derive(Clone, Copy)]
pub(crate) enum ShellFileMode {
    Read,
    Write,
}

/// Tools that read/write a file named as an argument. Not exhaustive — redirects
/// are the robust catch-all (caught via the AST for any program).
fn shell_file_mode(program: &str) -> Option<ShellFileMode> {
    match program {
        "cat" | "tac" | "nl" | "head" | "tail" | "grep" | "egrep" | "fgrep" | "rg" | "sed"
        | "awk" | "less" | "more" | "bat" | "strings" | "xxd" | "od" | "hexdump" | "base64"
        | "base32" | "cut" | "sort" | "uniq" | "wc" | "type" | "get-content" | "gc" | "diff"
        | "comm" | "rev" | "jq" | "yq" | "select-string" | "sls" | "ag" | "ack" | "zcat"
        | "zless" | "zmore" | "zgrep" | "zegrep" | "zfgrep" | "bzcat" | "bzgrep" | "xzcat"
        | "xzgrep" | "zstdcat" | "lz4cat" => Some(ShellFileMode::Read),
        "tee" | "set-content" | "out-file" | "add-content" | "tee-object" | "truncate" => {
            Some(ShellFileMode::Write)
        }
        _ => None,
    }
}

fn shell_program_name(word: &str) -> &str {
    word.rsplit(['/', '\\']).next().unwrap_or(word)
}

/// True if a command runs in the current shell (reaching later commands), not in
/// a subshell/pipeline/substitution or backgrounded.
fn runs_in_current_shell(cmd: Node<'_>) -> bool {
    let mut node = cmd;
    loop {
        if node.next_sibling().is_some_and(|s| s.kind() == "&") {
            return false; // backgrounded subshell
        }
        let Some(parent) = node.parent() else {
            return true;
        };
        if matches!(
            parent.kind(),
            "subshell" | "pipeline" | "command_substitution" | "process_substitution"
        ) {
            return false;
        }
        node = parent;
    }
}

/// Source positions of in-shell `cd`/`pushd`/`popd`. We don't resolve the new
/// directory; a relative operand after one is unpinnable → Ask.
fn cwd_poison_positions(root: Node<'_>, src: &str) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "command"
            && node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                .is_some_and(|p| matches!(shell_program_name(p), "cd" | "pushd" | "popd"))
            && runs_in_current_shell(node)
        {
            positions.push(node.start_byte());
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                stack.push(child);
            }
        }
    }
    positions
}

/// Whether an operand at `at` runs after a cwd change, so it can't be pinned.
fn cwd_unpinned_before(positions: &[usize], at: usize) -> bool {
    positions.iter().any(|&p| p < at)
}

/// A command operand or redirect destination extracted from the AST.
enum ArgText {
    /// Literal path/word, no runtime expansion.
    Literal(String),
    /// Runtime expansion; unpinnable, so callers prompt.
    Ambiguous,
}

/// True if any descendant expands at runtime (e.g. `$X` in `.e"$X"`), so the text
/// isn't a literal path.
fn node_has_expansion(node: Node<'_>) -> bool {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        for i in 0..n.child_count() {
            let Some(child) = n.child(i) else { continue };
            if matches!(
                child.kind(),
                "expansion"
                    | "simple_expansion"
                    | "command_substitution"
                    | "arithmetic_expansion"
                    | "process_substitution"
            ) {
                return true;
            }
            stack.push(child);
        }
    }
    false
}

/// Literal text of an operand node, or `Ambiguous` if it expands at runtime;
/// `None` for non-operands (e.g. a leading `VAR=value`).
fn shell_node_arg(node: Node<'_>, src: &str) -> Option<ArgText> {
    let text = || node.utf8_text(src.as_bytes()).ok().map(str::to_owned);
    match node.kind() {
        "variable_assignment" => None,
        "word" | "number" => text().map(ArgText::Literal),
        "raw_string" => {
            let raw = node.utf8_text(src.as_bytes()).ok()?;
            let stripped = raw
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
                .unwrap_or(raw);
            Some(ArgText::Literal(stripped.to_owned()))
        }
        "string" => {
            if node_has_expansion(node) {
                return Some(ArgText::Ambiguous);
            }
            let raw = node.utf8_text(src.as_bytes()).ok()?;
            let stripped = raw
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(raw);
            Some(ArgText::Literal(stripped.to_owned()))
        }
        "concatenation" => {
            if node_has_expansion(node) {
                Some(ArgText::Ambiguous)
            } else {
                text().map(ArgText::Literal)
            }
        }
        _ => Some(ArgText::Ambiguous),
    }
}

/// Every `command` node (incl. nested) as `(start_byte, words, ambiguous)`, in
/// source order. `start_byte` orders invocations against cwd-change positions.
fn shell_command_invocations(root: Node<'_>, src: &str) -> Vec<(usize, Vec<String>, bool)> {
    let mut found: Vec<(usize, Vec<String>, bool)> = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "command" {
            let mut words = Vec::new();
            let mut ambiguous = false;
            for i in 0..node.named_child_count() {
                let Some(child) = node.named_child(i) else {
                    continue;
                };
                let operand = if child.kind() == "command_name" {
                    child
                        .named_child(0)
                        .and_then(|inner| shell_node_arg(inner, src))
                } else {
                    shell_node_arg(child, src)
                };
                match operand {
                    Some(ArgText::Literal(w)) => words.push(w),
                    Some(ArgText::Ambiguous) => ambiguous = true,
                    None => {}
                }
            }
            found.push((node.start_byte(), words, ambiguous));
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                stack.push(child);
            }
        }
    }
    found.sort_by_key(|(start, _, _)| *start);
    found
}

/// Every `file_redirect` target as `(start_byte, path, mode, ambiguous)`; skips
/// heredocs/fd-dups that touch no named file.
pub(crate) fn shell_redirect_targets(
    root: Node<'_>,
    src: &str,
) -> Vec<(usize, Option<String>, ShellFileMode, bool)> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "file_redirect"
            && let Some((path, mode, ambiguous)) = shell_redirect_one(node, src)
        {
            out.push((node.start_byte(), path, mode, ambiguous));
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                stack.push(child);
            }
        }
    }
    out
}

fn shell_redirect_one(node: Node<'_>, src: &str) -> Option<(Option<String>, ShellFileMode, bool)> {
    let mut mode = None;
    for i in 0..node.child_count() {
        let kind = node.child(i)?.kind();
        // `<<`/`<<<` read from inline text, not a file.
        if kind.contains("<<") {
            return None;
        }
        if kind.contains('>') {
            mode = Some(ShellFileMode::Write);
            break;
        }
        if kind.contains('<') {
            mode = Some(ShellFileMode::Read);
            break;
        }
    }
    let mode = mode?;
    let dest = node.child_by_field_name("destination")?;
    match shell_node_arg(dest, src)? {
        ArgText::Literal(s) => {
            // Skip fd duplications (`>&1`) and empty targets.
            if s.is_empty() || s.starts_with('&') || s.bytes().all(|b| b.is_ascii_digit()) {
                None
            } else {
                let ambiguous = shell_arg_is_ambiguous(&s);
                Some((Some(s), mode, ambiguous))
            }
        }
        ArgText::Ambiguous => Some((None, mode, true)),
    }
}

fn shell_sed_in_place(words: &[String]) -> bool {
    words.iter().skip(1).any(|word| {
        word == "--in-place"
            || word.starts_with("--in-place=")
            // `i` is sed's only short flag with that letter → any `-…i…` is in-place.
            || (word.starts_with('-') && !word.starts_with("--") && word.contains('i'))
    })
}

fn shell_output_flag_values(words: &[String]) -> impl Iterator<Item = &str> {
    words.iter().enumerate().filter_map(|(i, token)| {
        token
            .strip_prefix("--output=")
            .or_else(|| token.strip_prefix("-o").filter(|value| !value.is_empty()))
            .or_else(|| {
                (token == "--output" || token == "-o")
                    .then(|| words.get(i + 1).map(String::as_str))
                    .flatten()
            })
    })
}

/// Values of a value-taking flag written as `flag=v`, `flag v`, or — for short
/// flags only — glued `flagv` (e.g. `-ov`). Long (`--`) flags match `--flag=v` /
/// `--flag v` only (no glued form).
fn value_flag_values<'a>(words: &'a [String], flag: &str) -> Vec<&'a str> {
    let eq_prefix = format!("{flag}=");
    words
        .iter()
        .enumerate()
        .filter_map(|(i, token)| {
            if let Some(value) = token.strip_prefix(&eq_prefix) {
                Some(value)
            } else if token == flag {
                words.get(i + 1).map(String::as_str)
            } else if !flag.starts_with("--") {
                token.strip_prefix(flag).filter(|value| !value.is_empty())
            } else {
                None
            }
        })
        .collect()
}

/// Flag-named file operands (not positionals): `dd`'s `if=`/`of=` (read/write),
/// `sort`/`go`'s `-o`/`--output` build output, `git`'s `--output`/`-o`/`-O`, and
/// `rustfmt`'s file operands (rewritten in place). Empty for other programs.
fn special_file_operands(program: &str, words: &[String]) -> Vec<(String, ShellFileMode)> {
    match program {
        "dd" => words
            .iter()
            .skip(1)
            .filter_map(|token| {
                token
                    .strip_prefix("if=")
                    .map(|path| (path.to_owned(), ShellFileMode::Read))
                    .or_else(|| {
                        token
                            .strip_prefix("of=")
                            .map(|path| (path.to_owned(), ShellFileMode::Write))
                    })
            })
            .collect(),
        // `--output`/`-o` write the output file. (`git`'s `-O` is a READ
        // order-file, NOT a write, so it is intentionally excluded.)
        "sort" | "go" | "git" => shell_output_flag_values(words)
            .map(|output| (output.to_owned(), ShellFileMode::Write))
            .collect(),
        // `rustc` writes its compiled output via `-o`/`--out-dir` (mirrors `go`).
        "rustc" => shell_output_flag_values(words)
            .chain(value_flag_values(words, "--out-dir"))
            .map(|output| (output.to_owned(), ShellFileMode::Write))
            .collect(),
        // `rustfmt` rewrites each file operand in place (like an always-on
        // `sed -i`), so its non-flag operands are writes.
        "rustfmt" => shell_file_candidates(words)
            .into_iter()
            .map(|path| (path.to_owned(), ShellFileMode::Write))
            .collect(),
        _ => Vec::new(),
    }
}

fn shell_access(mode: ShellFileMode, path: String) -> AccessKind {
    match mode {
        ShellFileMode::Read => AccessKind::Read(Some(path)),
        ShellFileMode::Write => AccessKind::Edit(path),
    }
}

/// Operands that may name a file. After a bare `--`, tokens are positional even if
/// `-`-prefixed (`rm -- -/../.env`). `=`-names are kept (a real `VAR=value` is
/// already dropped by the AST).
fn shell_file_candidates(words: &[String]) -> Vec<&str> {
    let mut out = Vec::new();
    let mut end_of_options = false;
    for token in words.iter().skip(1) {
        if !end_of_options && token == "--" {
            end_of_options = true;
            continue;
        }
        if end_of_options || (token != "-" && !token.starts_with('-')) {
            out.push(token.as_str());
        }
    }
    out
}

/// File operands implied by path-moving commands. `cp`/`mv`/`ln`/`install` read
/// source(s) and write the destination; `rm`/`rmdir`/`mkdir`/`touch` write every
/// operand; `None` otherwise. (`chmod`/`chown` touch metadata, not content.)
fn shell_path_command_operands<'a>(
    program: &str,
    words: &'a [String],
) -> Option<Vec<(&'a str, ShellFileMode)>> {
    match program {
        "cp" | "mv" | "ln" | "install" => {
            // Last positional is the destination (Write), the rest sources (Read).
            // The rare `-t DIR` reorder isn't parsed — bounded since denies match
            // by basename.
            let operands = shell_file_candidates(words);
            let (dest, sources) = operands.split_last()?;
            Some(
                sources
                    .iter()
                    .map(|s| (*s, ShellFileMode::Read))
                    .chain(std::iter::once((*dest, ShellFileMode::Write)))
                    .collect(),
            )
        }
        "rm" | "rmdir" | "mkdir" | "touch" => Some(
            shell_file_candidates(words)
                .into_iter()
                .map(|c| (c, ShellFileMode::Write))
                .collect(),
        ),
        // `uniq [INPUT [OUTPUT]]`: a 2nd positional is the output file (Write);
        // the 1st is the input (Read). Fewer operands use stdin/stdout.
        "uniq" => match shell_file_candidates(words).as_slice() {
            [input, output, ..] => Some(vec![
                (*input, ShellFileMode::Read),
                (*output, ShellFileMode::Write),
            ]),
            _ => None,
        },
        _ => None,
    }
}

fn shell_arg_is_ambiguous(token: &str) -> bool {
    token.contains('*') || token.contains('?') || token.contains('[')
}

/// A recursive directory search can't pin its operands → prompt. `rg`/`ag`/`ack`
/// recurse given no path or a directory operand (`candidates[0]` is the pattern);
/// grep only with `-r`/`-R`.
fn shell_reader_can_recurse(program: &str, words: &[String], candidates: &[&str]) -> bool {
    let grep_recursive = matches!(program, "grep" | "egrep" | "fgrep")
        && words.iter().any(|word| {
            word == "--recursive"
                || word == "--dereference-recursive"
                || (word.starts_with('-')
                    && !word.starts_with("--")
                    && (word.contains('r') || word.contains('R')))
        });
    let searches_dir = matches!(program, "rg" | "ag" | "ack")
        && (candidates.len() <= 1 || candidates.iter().skip(1).any(|c| is_directory_operand(c)));
    grep_recursive || searches_dir
}

/// A path that syntactically names a directory (so a recursive reader descends it).
fn is_directory_operand(token: &str) -> bool {
    token == "." || token == ".." || token.ends_with('/')
}

fn is_absolute_shell_path(path: &str) -> bool {
    path.starts_with('/')
        || path.starts_with("~/")
        || path.as_bytes().get(1).is_some_and(|b| *b == b':')
}

fn normalize_shell_path(path: &str) -> String {
    lexical_normalize(&normalize_shell_path_raw(path))
}

/// Quote/backslash/`/c/` normalization WITHOUT collapsing `.`/`..`, so symlink
/// resolution can follow `..` *physically* (after the link) rather than have it
/// erased textually before the link is ever seen.
fn normalize_shell_path_raw(path: &str) -> String {
    let p = path.trim_matches(['\"', '\'']).replace('\\', "/");
    match p.strip_prefix("/c/") {
        Some(rest) => format!("C:/{rest}"),
        None => p,
    }
}

fn lexical_normalize(path: &str) -> String {
    let prefix_len = if path.as_bytes().get(1).is_some_and(|b| *b == b':') {
        2
    } else {
        0
    };
    let (prefix, rest) = path.split_at(prefix_len);
    let absolute = rest.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for segment in rest.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if out.last().is_some_and(|s| *s != "..") {
                    out.pop();
                } else if !absolute {
                    out.push("..");
                }
            }
            segment => out.push(segment),
        }
    }
    let body = out.join("/");
    match (prefix.is_empty(), absolute, body.is_empty()) {
        (false, true, false) => format!("{prefix}/{body}"),
        (false, true, true) => format!("{prefix}/"),
        (false, false, _) => format!("{prefix}{body}"),
        (true, true, false) => format!("/{body}"),
        (true, true, true) => "/".to_owned(),
        (true, false, _) => body,
    }
}

/// Whether *any* existing component of `absolute` is a symlink — used to fail
/// closed (Ask) when a linky operand can't be fully resolved, including a
/// mid-path link (not just the leaf).
fn path_has_symlink(absolute: &str) -> bool {
    let path = Path::new(absolute);
    if !path.is_absolute() {
        return false;
    }
    let mut prefix = PathBuf::new();
    for comp in path.components() {
        prefix.push(comp);
        if std::fs::symlink_metadata(&prefix)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

/// Resolve a filesystem-absolute operand to its real symlink target. `None` for
/// relative/unanchorable inputs. Input must be absolute so resolution anchors to
/// the command's cwd, not the process cwd. Point-in-time (TOCTOU) only.
fn resolve_symlink_target(absolute: &str) -> Option<String> {
    let path = Path::new(absolute);
    if !path.is_absolute() {
        return None;
    }
    let resolved = resolve_following_symlinks(path, 0)?;
    // `/`-normalize so the result matches rule text on Windows (backslash form).
    Some(normalize_shell_path(&resolved.to_string_lossy()))
}

/// Resolve `path` following every symlink, including a *dangling* final link
/// (which `canonicalize` alone rejects) and not-yet-existing trailing
/// components. Depth-bounded against cycles; any fs error yields `None`.
/// Blocking fs syscalls; runs per operand when file rules exist.
fn resolve_following_symlinks(path: &Path, depth: usize) -> Option<PathBuf> {
    const MAX_SYMLINK_DEPTH: usize = 40;
    if depth > MAX_SYMLINK_DEPTH {
        return None;
    }
    // `dunce` avoids Windows `\\?\` verbatim paths (repo convention).
    if let Ok(canonical) = dunce::canonicalize(path) {
        return Some(canonical);
    }
    // Resolve the parent, then the final component, so a dangling/new leaf still follows.
    let parent = path.parent()?;
    let file_name = path.file_name()?;
    let resolved_parent = resolve_following_symlinks(parent, depth + 1)?;
    let candidate = resolved_parent.join(file_name);
    if let Ok(meta) = std::fs::symlink_metadata(&candidate)
        && meta.file_type().is_symlink()
    {
        // A symlink must be followed; if it can't be read, treat the whole path
        // as unresolved (`None`) rather than returning the link's own path.
        let target = std::fs::read_link(&candidate).ok()?;
        let target = if target.is_absolute() {
            target
        } else {
            resolved_parent.join(target)
        };
        return resolve_following_symlinks(&target, depth + 1);
    }
    Some(candidate)
}

fn decision_rank(decision: &Decision) -> u8 {
    match decision {
        Decision::Reject(_) | Decision::PolicyDeny(_) => 3,
        Decision::Ask => 2,
        Decision::Allow => 1,
        _ => 0,
    }
}

pub(crate) fn combine_decisions(a: Option<Decision>, b: Option<Decision>) -> Option<Decision> {
    match (a, b) {
        (None, other) | (other, None) => other,
        (Some(a), Some(b)) => Some(if decision_rank(&a) >= decision_rank(&b) {
            a
        } else {
            b
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::types::{
        PatternMode, PermissionConfig, PermissionRule, RuleAction, ToolFilter,
    };

    fn file_rule(action: RuleAction, tool: ToolFilter, pattern: &str) -> PermissionRule {
        PermissionRule {
            action,
            tool,
            pattern: Some(pattern.to_owned()),
            pattern_mode: PatternMode::Glob,
        }
    }

    fn bash_rule(action: RuleAction, pattern: &str) -> PermissionRule {
        file_rule(action, ToolFilter::Bash, pattern)
    }

    fn compiled(rules: Vec<PermissionRule>) -> CompiledPolicy {
        CompiledPolicy::new(PermissionConfig::new(rules))
    }

    fn cwd() -> &'static std::path::Path {
        std::path::Path::new("/work")
    }

    #[test]
    fn shell_file_gate_distinguishes_ask_provenance() {
        let ask = compiled(vec![file_rule(
            RuleAction::Ask,
            ToolFilter::Read,
            "**/secrets/**",
        )]);
        // Rule match: an identified operand hits the ask rule.
        assert_eq!(
            ask.evaluate_shell_file_access_gate("cat secrets/token.txt", cwd()),
            Some(GateDecision::AskRuleMatch)
        );
        // Fail-closed: a recursive reader has no pinnable operands.
        assert_eq!(
            ask.evaluate_shell_file_access_gate("rg TODO", cwd()),
            Some(GateDecision::AskFailClosed)
        );
        // Fail-closed: a dynamic operand on a known reader is unpinnable.
        assert_eq!(
            ask.evaluate_shell_file_access_gate("cat \"$F\"", cwd()),
            Some(GateDecision::AskFailClosed)
        );
        // A rule match anywhere outranks a fail-closed floor in the same script.
        assert_eq!(
            ask.evaluate_shell_file_access_gate("rg TODO && cat secrets/token.txt", cwd()),
            Some(GateDecision::AskRuleMatch)
        );
        // Deny rules keep rejecting with provenance preserved.
        let deny = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "**/.env",
        )]);
        assert!(matches!(
            deny.evaluate_shell_file_access_gate("cat .env", cwd()),
            Some(GateDecision::Reject(_))
        ));
    }

    #[test]
    fn shell_gate_matches_cwd_relative_rules_on_absolute_operands() {
        let deny = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "src/**",
        )]);
        // A rooted relative rule keys on the same file spelled absolutely,
        // matching the direct Read tool gate (which evaluates with the cwd).
        assert!(matches!(
            deny.evaluate_shell_file_access_gate("cat /work/src/secret.txt", cwd()),
            Some(GateDecision::Reject(_))
        ));
        // An absolute operand is cwd-independent, so it stays covered even
        // after a `cd` unpins the working directory.
        assert!(matches!(
            deny.evaluate_shell_file_access_gate("cd /tmp && cat /work/src/secret.txt", cwd()),
            Some(GateDecision::Reject(_))
        ));
        // Outside the working directory the rooted rule stays silent.
        assert_eq!(
            deny.evaluate_shell_file_access_gate("cat /elsewhere/src/secret.txt", cwd()),
            None
        );
    }

    #[test]
    fn sensitive_edit_targets_and_lexical_aliases_prompt() {
        for path in [
            "/home/user/.zshrc",
            "/etc",
            "/etc/grok-test",
            "/work/subdir/../.git/hooks/pre-commit",
            "/home/user/.grok/sandbox.toml",
            "/work/project/.grok/sandbox.toml",
        ] {
            assert!(
                edit_target_protection(Path::new(path)).is_some(),
                "protected edit target must prompt: {path}"
            );
        }
        for path in [
            "/work/src/main.rs",
            "/work/project/.grok/config.toml/backup",
            "/work/project/sandbox.toml",
            "/work/project/requirements.toml",
            "/work/project/managed_config.toml",
        ] {
            assert!(
                edit_target_protection(Path::new(path)).is_none(),
                "ordinary edit target should not prompt: {path}"
            );
        }
    }

    #[test]
    fn sensitive_edit_targets_include_submodule_hooks() {
        for path in [
            "/work/.git/modules/foo/hooks/pre-commit",
            "/work/.git/modules/submodules/sglang-private/hooks/pre-commit",
            "/work/.git/modules/outer/modules/inner/hooks/pre-commit",
            "/work/subdir/../.git/modules/foo/hooks/pre-commit",
        ] {
            assert!(
                edit_target_protection(Path::new(path)).is_some(),
                "submodule hook target must prompt: {path}"
            );
        }
        for path in [
            "/work/.git/modules/hooks/pre-commit",
            "/work/.git/module/foo/hooks/pre-commit",
            "/work/.git/modules/foo/hook/pre-commit",
            "/work/.git/modules/foo/hooks-disabled/pre-commit",
            "/work/src/modules/foo/hooks/pre-commit",
        ] {
            assert!(
                edit_target_protection(Path::new(path)).is_none(),
                "non-hook control must not prompt: {path}"
            );
        }
    }

    #[test]
    fn edit_target_protection_classifies_reasons() {
        let cases = [
            (
                "/home/user/.grok/hooks/evil.json",
                ProtectedEditReason::HookRoot,
            ),
            ("/work/.git/hooks/pre-commit", ProtectedEditReason::GitHooks),
            ("/home/user/.ssh/id_rsa", ProtectedEditReason::Ssh),
            ("/home/user/.zshrc", ProtectedEditReason::StartupFile),
            ("/etc/hosts", ProtectedEditReason::Etc),
            (
                "/home/user/.grok/config.toml",
                ProtectedEditReason::GrokConfig,
            ),
            (
                "/home/user/.grok/sandbox.toml",
                ProtectedEditReason::GrokSandbox,
            ),
            (
                "/work/project/.grok/sandbox.toml",
                ProtectedEditReason::GrokSandbox,
            ),
            (
                "/home/user/.grok/managed_config.toml",
                ProtectedEditReason::GrokConfig,
            ),
            (
                "/home/user/.grok/requirements.toml",
                ProtectedEditReason::GrokConfig,
            ),
            (
                "/home/user/.claude/settings.json",
                ProtectedEditReason::ClaudeSettings,
            ),
            (
                "/home/user/.cursor/hooks.json",
                ProtectedEditReason::CursorHooks,
            ),
        ];
        for (path, reason) in cases {
            assert_eq!(
                edit_target_protection(Path::new(path)),
                Some(reason),
                "{path}"
            );
            assert!(reason.description().is_some(), "{path}");
        }
        assert_eq!(
            edit_target_protection(Path::new("/home/user/project/src/main.rs")),
            None
        );
        assert!(ProtectedEditReason::Sensitive.description().is_none());
    }

    #[test]
    fn sensitive_edit_targets_include_hook_roots() {
        for path in [
            "/home/user/.grok/hooks/evil.json",
            "/home/user/.grok/hooks/nested/deep.json",
            "/home/user/.grok/hooks-paths",
            "/home/user/.claude/settings.json",
            "/home/user/.claude/settings.local.json",
            "/home/user/.cursor/hooks.json",
            "/work/project/.grok/hooks/local.json",
            "/work/project/.grok/hooks-paths",
        ] {
            assert!(
                edit_target_protection(Path::new(path)).is_some(),
                "hook root edit target must prompt: {path}"
            );
        }
        for path in [
            "/home/user/.grok/hooks-disabled/note.json",
            "/home/user/.grok/hooks-evil/note.json",
            "/home/user/project/src/hooks.json",
            "/home/user/.claude/other.json",
            "/home/user/.cursor/settings.json",
        ] {
            assert!(
                edit_target_protection(Path::new(path)).is_none(),
                "ordinary edit target should not prompt: {path}"
            );
        }
    }

    #[test]
    fn path_is_under_user_grok_hook_root_matches_relocated_home() {
        let home = Path::new("/custom/grok-home");
        for path in [
            "/custom/grok-home/hooks/x.json",
            "/custom/grok-home/hooks/nested/deep.json",
            "/custom/grok-home/hooks",
            "/custom/grok-home/hooks-paths",
        ] {
            assert!(
                path_is_under_user_grok_hook_root(Path::new(path), home),
                "must match under custom grok home: {path}"
            );
        }
        for path in [
            "/custom/grok-home/hooks-disabled/note.json",
            "/custom/grok-home/hooks-evil/note.json",
            "/custom/grok-home/config.toml",
            "/custom/other/hooks/x.json",
            "/custom/grok-home-extra/hooks/x.json",
        ] {
            assert!(
                !path_is_under_user_grok_hook_root(Path::new(path), home),
                "must not match outside hook roots: {path}"
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn sensitive_edit_targets_follow_symlinks() {
        use std::os::unix::fs::symlink;
        let ws = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let startup = outside.path().join(".zshrc");
        std::fs::write(&startup, b"").unwrap();
        symlink(&startup, ws.path().join("file-link")).unwrap();
        std::fs::create_dir_all(outside.path().join(".git/hooks")).unwrap();
        symlink(
            outside.path().join(".git/hooks"),
            ws.path().join("hooks-link"),
        )
        .unwrap();
        std::fs::create_dir_all(outside.path().join(".git/modules/foo/hooks")).unwrap();
        symlink(
            outside.path().join(".git/modules/foo/hooks"),
            ws.path().join("module-hooks-link"),
        )
        .unwrap();
        let grok_hook = outside.path().join(".grok/hooks/evil.json");
        std::fs::create_dir_all(grok_hook.parent().unwrap()).unwrap();
        std::fs::write(&grok_hook, b"{}").unwrap();
        symlink(&grok_hook, ws.path().join("grok-hook-link")).unwrap();

        for path in [
            ws.path().join("file-link"),
            ws.path().join("hooks-link/new-hook"),
            ws.path().join("module-hooks-link/new-hook"),
            ws.path().join("grok-hook-link"),
        ] {
            assert!(
                edit_target_protection(&path).is_some(),
                "symlinked protected edit target must prompt: {}",
                path.display()
            );
        }
    }

    /// A custom `$GROK_HOME` has no `.grok` path component, so the live
    /// `config.toml` / `sandbox.toml` must be caught by the home-prefix branch.
    #[test]
    fn grok_config_files_under_custom_grok_home_are_protected() {
        let home = tempfile::tempdir().unwrap();
        let home_path = home.path();
        for (file, reason) in [
            ("config.toml", ProtectedEditReason::GrokConfig),
            ("managed_config.toml", ProtectedEditReason::GrokConfig),
            ("requirements.toml", ProtectedEditReason::GrokConfig),
            ("sandbox.toml", ProtectedEditReason::GrokSandbox),
        ] {
            let path = home_path.join(file);
            let components = [file];
            assert_eq!(
                protected_grok_config_file_with_home(&path, &components, Some(home_path)),
                Some(reason),
                "{file} directly under $GROK_HOME must be protected"
            );
        }
        // Same file names elsewhere (or with no resolvable home) stay ordinary.
        let elsewhere = home_path.join("sub").join("sandbox.toml");
        assert_eq!(
            protected_grok_config_file_with_home(
                &elsewhere,
                &["sub", "sandbox.toml"],
                Some(home_path)
            ),
            None
        );
        assert_eq!(
            protected_grok_config_file_with_home(
                &home_path.join("sandbox.toml"),
                &["sandbox.toml"],
                None
            ),
            None
        );
    }

    /// The resolved-symlink arm of the grok-home match must decide: `$GROK_HOME`
    /// points at a symlink while the edit targets the physical home directory,
    /// so the lexical parent-equality arm cannot fire.
    #[test]
    #[cfg(unix)]
    fn grok_config_under_symlinked_grok_home_is_protected() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let real_home = tmp.path().join("real-home");
        std::fs::create_dir(&real_home).unwrap();
        let link = tmp.path().join("home-link");
        symlink(&real_home, &link).unwrap();
        // tempdir paths can themselves contain symlinks (macOS /var -> /private/var);
        // compare against the physical home the production resolver will produce.
        let physical_home = resolve_following_symlinks(&real_home, 0).unwrap();
        assert_eq!(
            protected_grok_config_file_with_home(
                &physical_home.join("sandbox.toml"),
                &["sandbox.toml"],
                Some(&link)
            ),
            Some(ProtectedEditReason::GrokSandbox)
        );
    }

    /// `protected_edit_reason` lowercases path components before matching, so
    /// the canonical filename constants must stay lowercase or the const
    /// patterns silently stop firing.
    #[test]
    fn protected_config_filename_constants_are_lowercase() {
        for name in [
            agent_tui_config::USER_CONFIG_FILENAME,
            agent_tui_config::MANAGED_CONFIG_FILENAME,
            agent_tui_config::REQUIREMENTS_FILENAME,
        ] {
            assert_eq!(name, name.to_ascii_lowercase(), "{name}");
        }
    }

    #[test]
    fn resolved_root_alias_matches_physical_destination() {
        let resolved_root = resolve_following_symlinks(Path::new("/etc"), 0).unwrap();
        assert!(resolved_path_is_within_root(
            &resolved_root.join("grok-test"),
            Path::new("/etc")
        ));
        assert!(!resolved_path_is_within_root(
            Path::new("/tmp/grok-test"),
            Path::new("/etc")
        ));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn private_etc_alias_requires_prompt() {
        assert!(edit_target_protection(Path::new("/private/etc/hosts")).is_some());
    }

    #[test]
    #[cfg(unix)]
    fn resolved_symlink_target_hits_read_deny() {
        use std::os::unix::fs::symlink;
        let ws = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let secret_dir = outside.path().join("prohibited-zone");
        std::fs::create_dir(&secret_dir).unwrap();
        std::fs::write(secret_dir.join("data.txt"), b"secret").unwrap();
        symlink(&secret_dir, ws.path().join("linked")).unwrap();

        let policy = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "**/prohibited-zone/**",
        )]);
        let decision = policy.evaluate_shell_file_access("cat linked/data.txt", ws.path());
        assert!(
            matches!(decision, Some(Decision::Reject(_))),
            "read via a symlink to a denied dir must be rejected, got {decision:?}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn dangling_symlink_write_hits_edit_deny() {
        use std::os::unix::fs::symlink;
        let ws = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let secret_dir = outside.path().join("prohibited-zone");
        std::fs::create_dir(&secret_dir).unwrap();
        // Dangling link: target doesn't exist yet.
        symlink(secret_dir.join("new.txt"), ws.path().join("out")).unwrap();

        let policy = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Edit,
            "**/prohibited-zone/**",
        )]);
        let decision = policy.evaluate_shell_file_access("echo hi > out", ws.path());
        assert!(
            matches!(decision, Some(Decision::Reject(_))),
            "write through a dangling symlink into a denied dir must be rejected, got {decision:?}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn resolved_symlink_to_allowed_target_not_blocked() {
        use std::os::unix::fs::symlink;
        let ws = tempfile::tempdir().unwrap();
        std::fs::create_dir(ws.path().join("real")).unwrap();
        std::fs::write(ws.path().join("real/data.txt"), b"ok").unwrap();
        symlink(ws.path().join("real"), ws.path().join("linked")).unwrap();

        let policy = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "**/prohibited-zone/**",
        )]);
        assert!(
            policy
                .evaluate_shell_file_access("cat linked/data.txt", ws.path())
                .is_none(),
            "a symlink to a non-denied path must not be blocked"
        );
    }

    /// `..` after a symlink must resolve physically: `link/../dir2/x` where
    /// `link -> <zone>/dir` lands in `<zone>/dir2/x`, not `<cwd>/dir2/x`.
    #[test]
    #[cfg(unix)]
    fn resolved_symlink_dotdot_hits_read_deny() {
        use std::os::unix::fs::symlink;
        let ws = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let zone = outside.path().join("prohibited-zone");
        std::fs::create_dir_all(zone.join("dir")).unwrap();
        std::fs::create_dir_all(zone.join("dir2")).unwrap();
        std::fs::write(zone.join("dir2/x"), b"secret").unwrap();
        symlink(zone.join("dir"), ws.path().join("link")).unwrap();

        let policy = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "**/prohibited-zone/**",
        )]);
        let decision = policy.evaluate_shell_file_access("cat link/../dir2/x", ws.path());
        assert!(
            matches!(decision, Some(Decision::Reject(_))),
            "`..` after a symlink must resolve into the denied tree, got {decision:?}"
        );
    }

    /// An unresolvable linky operand (symlink cycle) fails closed to Ask rather
    /// than silently passing the gate.
    #[test]
    #[cfg(unix)]
    fn unresolvable_symlink_operand_asks() {
        use std::os::unix::fs::symlink;
        let ws = tempfile::tempdir().unwrap();
        symlink(ws.path().join("b"), ws.path().join("a")).unwrap();
        symlink(ws.path().join("a"), ws.path().join("b")).unwrap();

        let policy = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "**/prohibited-zone/**",
        )]);
        let decision = policy.evaluate_shell_file_access("cat a", ws.path());
        assert!(
            matches!(decision, Some(Decision::Ask)),
            "unresolvable symlink operand must escalate to Ask, got {decision:?}"
        );
    }

    /// A *mid-path* symlink chain that can't be resolved (non-symlink leaf) must
    /// still fail closed to Ask, not skip the check.
    #[test]
    #[cfg(unix)]
    fn unresolvable_midpath_symlink_operand_asks() {
        use std::os::unix::fs::symlink;
        let ws = tempfile::tempdir().unwrap();
        // Directory-component cycle: linkdir -> linkdir2 -> linkdir.
        symlink(ws.path().join("linkdir2"), ws.path().join("linkdir")).unwrap();
        symlink(ws.path().join("linkdir"), ws.path().join("linkdir2")).unwrap();

        let policy = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "**/prohibited-zone/**",
        )]);
        // Leaf `file.txt` is not itself a symlink; the link is the `linkdir` component.
        let decision = policy.evaluate_shell_file_access("cat linkdir/file.txt", ws.path());
        assert!(
            matches!(decision, Some(Decision::Ask)),
            "unresolvable mid-path symlink chain must escalate to Ask, got {decision:?}"
        );
    }

    #[test]
    fn shell_readers_hit_read_deny() {
        let policy = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "**/.env",
        )]);
        for cmd in [
            "cat .env",
            "grep . .env",
            "head -n 5 .env",
            "sed -n 1p .env",
            "dd if=.env",
            "base64 .env",
            "sort < .env",
            "sort <.env",
            "grep -f .env README.md",
            "sed -f .env README.md",
            "awk -f .env README.md",
            // additional readers: dumpers, jq, PS/grep-alts, compressed
            "diff .env /dev/null",
            "comm .env /dev/null",
            "rev .env",
            "jq . .env",
            "select-string FAKE .env",
            "ag FAKE .env",
            "zcat .env",
            "zgrep FAKE .env",
        ] {
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(cmd, cwd()),
                    Some(Decision::Reject(_))
                ),
                "expected deny for {cmd}"
            );
        }
    }

    #[test]
    fn shell_writers_hit_edit_deny() {
        let policy = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Edit,
            "**/.env",
        )]);
        for cmd in [
            "tee .env",
            "dd of=.env",
            "Set-Content .env secret",
            "Out-File .env",
            "echo secret > .env",
            "echo secret >.env",
            "echo secret >>.env",
            "sed -i.bak s/FAKE/HACKED/ .env",
            "sed -ni s/FAKE/HACKED/ .env",
            "sort README.md -o .env",
            "truncate -s 0 .env",
            "Tee-Object .env",
        ] {
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(cmd, cwd()),
                    Some(Decision::Reject(_))
                ),
                "expected deny for {cmd}"
            );
        }
    }

    #[test]
    fn shell_gate_merges_decisions_so_deny_beats_earlier_ask() {
        // The whole command runs once approved, so a later deny must beat an earlier ask.
        let policy = compiled(vec![
            file_rule(RuleAction::Ask, ToolFilter::Edit, "**/dump.txt"),
            file_rule(RuleAction::Ask, ToolFilter::Read, "**/notes.txt"),
            file_rule(RuleAction::Deny, ToolFilter::Read, "**/.env"),
        ]);
        for cmd in [
            // Redirect target (checked first) asks; the read operand denies.
            "cat .env > dump.txt",
            // First operand asks; the second denies.
            "cat notes.txt .env",
        ] {
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(cmd, cwd()),
                    Some(Decision::Reject(_))
                ),
                "deny on a later path must win over an earlier ask for {cmd}"
            );
        }
    }

    #[test]
    fn shell_in_place_sed_enforces_read_deny() {
        // `sed -i` reads each operand before rewriting it, so a Read deny must block it.
        let policy = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "**/.env",
        )]);
        for cmd in ["sed -i s/FAKE/X/ .env", "sed -ni s/FAKE/X/ .env"] {
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(cmd, cwd()),
                    Some(Decision::Reject(_))
                ),
                "in-place sed must honor a Read deny for {cmd}"
            );
        }
    }

    #[test]
    fn powershell_and_windows_path_readers_hit_read_deny() {
        let policy = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "**/.env",
        )]);
        for cmd in [
            "Get-Content .env",
            "gc .env",
            "type .env",
            "more .env",
            "Get-Content C:\\Users\\alice\\repo\\.env",
            "Get-Content /c/Users/alice/repo/.env",
        ] {
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(cmd, cwd()),
                    Some(Decision::Reject(_))
                ),
                "expected deny for {cmd}"
            );
        }
    }

    /// A relative operand after any in-shell `cd`/`pushd`/`env -C` is unpinnable
    /// → Ask. Only path-scoped rules are affected; basename denies still fire.
    #[test]
    fn shell_cwd_change_escalates_path_scoped_operands() {
        let policy = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "/repo-b/.env", // path-scoped: matching would need the untracked cd target
        )]);
        let session = std::path::Path::new("/repo-a");
        for cmd in [
            "cd /repo-b && cat .env",                 // cd in the current shell
            "pushd /repo-b; cat .env",                // pushd is never folded
            "if true; then cd /repo-b; fi; cat .env", // conditional cd
            "env -C /repo-b cat .env",                // env chdir wrapper
            "env --chdir=/repo-b cat .env",
            "/usr/bin/env -C /repo-b cat .env", // path-qualified env
            "env FOO=1 -C /repo-b cat .env",    // chdir after an assignment
            "cd /repo-b && echo x > .env",      // redirect operand too
        ] {
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(cmd, session),
                    Some(Decision::Ask)
                ),
                "an unpinnable cwd change must escalate: {cmd}"
            );
        }
    }

    /// A `**/` basename deny matches regardless of cwd, so a `cd`/`env -C` can't
    /// smuggle a denied read past the gate.
    #[test]
    fn shell_basename_deny_survives_cwd_change() {
        let policy = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "**/.env",
        )]);
        let session = std::path::Path::new("/repo-a");
        for cmd in [
            "cd /repo-b && cat .env",
            "env -C /repo-b cat .env",
            "pushd /repo-b; cat .env",
        ] {
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(cmd, session),
                    Some(Decision::Reject(_))
                ),
                "a basename deny must still fire under a cwd change: {cmd}"
            );
        }
    }

    /// A `cd` in a pipeline/subshell/backgrounded `&` doesn't change a sibling's
    /// cwd, so their reads resolve against the original cwd, not the `cd` target.
    #[test]
    fn shell_cd_does_not_scope_across_pipe_subshell_or_background() {
        // Deny is scoped to the original cwd (`/work`), where the reader runs.
        let policy = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "/work/secret.env",
        )]);
        for cmd in [
            "cd /elsewhere | cat secret.env",  // pipeline segment: own subshell
            "(cd /elsewhere); cat secret.env", // subshell ended with `;`
            "cd /elsewhere & cat secret.env",  // backgrounded cd
        ] {
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(cmd, cwd()),
                    Some(Decision::Reject(_))
                ),
                "cd must not scope across boundary: {cmd}"
            );
        }
        // A deny scoped to the cd target must not fire — the reader never runs there.
        let elsewhere = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "/elsewhere/secret.env",
        )]);
        assert!(
            elsewhere
                .evaluate_shell_file_access("cd /elsewhere | cat secret.env", cwd())
                .is_none(),
            "reader runs in the original cwd, so the cd-target deny must not match"
        );
    }

    /// After `--`, tokens are positional even when they start with `-`, so a path
    /// like `-/../.env` must still be deny-checked (not skipped as a flag).
    #[test]
    fn shell_double_dash_end_of_options_extracts_paths() {
        let read = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "**/.env",
        )]);
        assert!(matches!(
            read.evaluate_shell_file_access("cat -- -/../.env", cwd()),
            Some(Decision::Reject(_))
        ));
        let edit = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Edit,
            "**/.env",
        )]);
        assert!(matches!(
            edit.evaluate_shell_file_access("rm -- -/../.env", cwd()),
            Some(Decision::Reject(_))
        ));
    }

    /// `cp`/`mv`/`ln`/`install`/`rm`/`touch` move/destroy files: sources are reads
    /// (exfil), destinations are writes.
    #[test]
    fn shell_path_commands_hit_deny() {
        // Reading a denied source (exfil via copy/move) is caught by a Read deny.
        let read = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "**/.env",
        )]);
        for cmd in [
            "cp .env /tmp/x",
            "mv .env /tmp/exfil",
            "install .env /tmp/x",
        ] {
            assert!(
                matches!(
                    read.evaluate_shell_file_access(cmd, cwd()),
                    Some(Decision::Reject(_))
                ),
                "source read must be denied: {cmd}"
            );
        }
        // Writing/deleting a denied path is caught by an Edit deny.
        let edit = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Edit,
            "**/.env",
        )]);
        for cmd in [
            "rm .env",
            "touch .env",
            "mkdir .env",
            "cp src .env",
            "ln -s src .env",
        ] {
            assert!(
                matches!(
                    edit.evaluate_shell_file_access(cmd, cwd()),
                    Some(Decision::Reject(_))
                ),
                "write/delete must be denied: {cmd}"
            );
        }
        // A copy source is a read, not an edit: an Edit-only deny must not fire.
        assert!(
            edit.evaluate_shell_file_access("cp .env /tmp/x", cwd())
                .is_none(),
            "copying a source only reads it, so an Edit-only deny must not match"
        );
    }

    /// A reader whose operand can't be pinned (glob, recursive search, expansion) prompts.
    #[test]
    fn ambiguous_known_reader_prompts_when_path_cannot_be_pinned() {
        let policy = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "**/.env",
        )]);
        for cmd in ["cat *.env", "grep -r secret .", "cat \"$HOME/.env\""] {
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(cmd, cwd()),
                    Some(Decision::Ask)
                ),
                "unpinnable reader must prompt: {cmd}"
            );
        }
    }

    /// A positional `=`-operand is a filename (deny-checked); only leading
    /// `VAR=value` assignments are dropped by the AST.
    #[test]
    fn shell_reader_checks_equals_containing_operand() {
        let policy = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "**/data=*.env",
        )]);
        assert!(
            matches!(
                policy.evaluate_shell_file_access("cat data=v1.env", cwd()),
                Some(Decision::Reject(_))
            ),
            "operand containing = must be deny-checked"
        );
        // A leading assignment is not an operand, so it isn't treated as a file.
        assert!(
            policy
                .evaluate_shell_file_access("FOO=data=v1.env cat README.md", cwd())
                .is_none()
        );
    }

    /// An expansion nested in a quoted/concatenated operand (`.e"$X"`) is ambiguous
    /// → prompt, not treated as a literal.
    #[test]
    fn shell_nested_expansion_operand_prompts() {
        let policy = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "**/.env",
        )]);
        for cmd in ["cat .e\"$X\"", "cat .e\"$(echo nv)\"", "cat pre\"${X}\"suf"] {
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(cmd, cwd()),
                    Some(Decision::Ask)
                ),
                "nested expansion must be ambiguous and prompt: {cmd}"
            );
        }
    }

    /// `rg`/`ag`/`ack` recurse a directory (no path, `.`, or `dir/`), so a Read deny
    /// on a path they could reach must prompt; a single file operand scopes them.
    #[test]
    fn shell_recursive_readers_prompt_for_directory_search() {
        let policy = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "**/.env",
        )]);
        for cmd in [
            "rg secret",      // no path: searches cwd
            "ack secret",     // no path
            "rg secret .",    // directory operand
            "rg secret src/", // directory operand
            "ag secret .",
        ] {
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(cmd, cwd()),
                    Some(Decision::Ask)
                ),
                "recursive directory search must prompt: {cmd}"
            );
        }
        assert!(
            policy
                .evaluate_shell_file_access("rg secret README.md", cwd())
                .is_none(),
            "a single file operand scopes the search"
        );
    }

    /// Arbitrary interpreters run code we don't parse, so reads inside them fall through.
    #[test]
    fn non_reader_is_not_covered_by_shell_gate() {
        let policy = compiled(vec![file_rule(
            RuleAction::Deny,
            ToolFilter::Read,
            "**/.env",
        )]);
        for cmd in [
            "python -c \"open('.env').read()\"",
            "node -e \"require('fs').readFileSync('.env')\"",
        ] {
            assert!(
                policy.evaluate_shell_file_access(cmd, cwd()).is_none(),
                "expected no shell gate decision for {cmd}"
            );
        }
    }

    /// Representative enterprise deny/ask fixture for managed-policy tests
    /// `[permission]` tier. Tool mapping: `Read`→Read, `Write`/`Edit`→Edit, `Bash`→Bash.
    fn enterprise_requirements_policy() -> CompiledPolicy {
        compiled(vec![
            // ── ask = [...] ──
            bash_rule(RuleAction::Ask, "kubectl *"),
            bash_rule(RuleAction::Ask, "terraform apply *"),
            bash_rule(RuleAction::Ask, "aws *"),
            bash_rule(RuleAction::Ask, "gcloud *"),
            bash_rule(RuleAction::Ask, "az *"),
            bash_rule(RuleAction::Ask, "ssh *"),
            bash_rule(RuleAction::Ask, "security *"),
            bash_rule(RuleAction::Ask, "op *"),
            file_rule(RuleAction::Ask, ToolFilter::Read, "**/secrets/**"),
            file_rule(RuleAction::Ask, ToolFilter::Edit, "**/secrets/**"), // Write(..)
            file_rule(RuleAction::Ask, ToolFilter::Edit, "**/secrets/**"), // Edit(..)
            file_rule(RuleAction::Ask, ToolFilter::Read, "**/Library/Mail/**"),
            // ── deny = [...] ──
            bash_rule(RuleAction::Deny, "rm -rf *"),
            bash_rule(RuleAction::Deny, "sudo *"),
            bash_rule(RuleAction::Deny, "su *"),
            bash_rule(RuleAction::Deny, "ssh *.corp.example"),
            file_rule(RuleAction::Deny, ToolFilter::Read, "**/.env"),
            file_rule(RuleAction::Deny, ToolFilter::Read, "**/.env.*"),
            file_rule(RuleAction::Deny, ToolFilter::Read, "**/*.pem"),
            file_rule(RuleAction::Deny, ToolFilter::Read, "**/*.key"),
            file_rule(RuleAction::Deny, ToolFilter::Read, "**/*.p12"),
            file_rule(RuleAction::Deny, ToolFilter::Read, "**/*.pfx"),
            file_rule(RuleAction::Deny, ToolFilter::Read, "**/*.jks"),
            file_rule(RuleAction::Deny, ToolFilter::Read, "**/*.keystore"),
            file_rule(RuleAction::Deny, ToolFilter::Read, "**/.ssh/**"),
            file_rule(RuleAction::Deny, ToolFilter::Read, "**/.aws/**"),
            file_rule(RuleAction::Deny, ToolFilter::Read, "**/.config/gcloud/**"),
            file_rule(RuleAction::Deny, ToolFilter::Read, "**/.kube/**"),
            file_rule(RuleAction::Deny, ToolFilter::Read, "**/.internal-deploy/**"),
            file_rule(RuleAction::Deny, ToolFilter::Read, "**/.git-credentials"),
            file_rule(RuleAction::Deny, ToolFilter::Read, "**/terraform.tfstate"),
            file_rule(
                RuleAction::Deny,
                ToolFilter::Read,
                "**/terraform.tfstate.backup",
            ),
            file_rule(
                RuleAction::Deny,
                ToolFilter::Read,
                "**/Library/Keychains/**",
            ),
            file_rule(RuleAction::Deny, ToolFilter::Edit, "**/.env*"), // Write(..)
            file_rule(RuleAction::Deny, ToolFilter::Edit, "**/.ssh/**"), // Write(..)
            file_rule(RuleAction::Deny, ToolFilter::Edit, "**/*.pem"), // Write(..)
            file_rule(RuleAction::Deny, ToolFilter::Edit, "**/*.key"), // Write(..)
            file_rule(RuleAction::Deny, ToolFilter::Edit, "**/*.p12"), // Write(..)
            file_rule(RuleAction::Deny, ToolFilter::Edit, "**/.internal-deploy/**"), // Write(..)
            file_rule(RuleAction::Deny, ToolFilter::Edit, "**/terraform.tfstate"), // Write(..)
            file_rule(
                RuleAction::Deny,
                ToolFilter::Edit,
                "**/terraform.tfstate.backup",
            ), // Write(..)
            file_rule(
                RuleAction::Deny,
                ToolFilter::Edit,
                "**/Library/Keychains/**",
            ), // Write(..)
            file_rule(RuleAction::Deny, ToolFilter::Edit, "**/.env"),
            file_rule(RuleAction::Deny, ToolFilter::Edit, "**/.env.*"),
        ])
    }

    /// fd-prefixed/glued READ redirects must still hit the Read deny via the AST walk.
    #[test]
    fn adversarial_fd_and_glued_read_redirects_denied() {
        let policy = enterprise_requirements_policy();
        for cmd in [
            "cat 0<.env",
            "cat 0< .env",
            "cat<.env",
            "grep secret 0<.env",
            "sort 0< .env",
            "head -n1 0<.env",
        ] {
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(cmd, cwd()),
                    Some(Decision::Reject(_))
                ),
                "read redirect must be denied: {cmd}"
            );
        }
    }

    /// fd-prefixed / glued WRITE redirects (truncate, append, stderr, both-streams)
    /// must hit the Edit deny.
    #[test]
    fn adversarial_fd_and_glued_write_redirects_denied() {
        let policy = enterprise_requirements_policy();
        for cmd in [
            "echo x 1>.env",
            "echo x 1> .env",
            "echo x 2>.env",
            "echo x 2> .env",
            "echo x &>.env",
            "echo x &> .env",
            "echo x 1>>.env",
            "echo x>>.env",
        ] {
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(cmd, cwd()),
                    Some(Decision::Reject(_))
                ),
                "write redirect must be denied: {cmd}"
            );
        }
    }

    /// An outer reader fed a substitution can't pin its operand (Ask); an inner
    /// literal read (incl. inside `<(…)`) is a hard deny.
    #[test]
    fn adversarial_substitution_readers_do_not_bypass() {
        let policy = enterprise_requirements_policy();
        for cmd in [
            "cat $(echo .env)",
            "xxd `echo .env`",
            "cut -d= -f2 $(echo .env)",
            "tac $(printf .env)",
            "nl $(echo .env)",
        ] {
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(cmd, cwd()),
                    Some(Decision::Ask)
                ),
                "substitution reader must prompt: {cmd}"
            );
        }
        for cmd in [
            "echo $(cat .env)",
            "echo `cat .env`",
            "echo $(base64 key.pem)",
            "diff <(cat .env) x",
        ] {
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(cmd, cwd()),
                    Some(Decision::Reject(_))
                ),
                "inner literal read must be denied: {cmd}"
            );
        }
    }

    /// Enterprise-policy coverage beyond the single-rule tests: extra readers, non-.env
    /// globs, wrappers, path normalization/traversal, chaining, case, ask-via-shell.
    #[test]
    fn adversarial_enterprise_matrix_denies_and_asks() {
        let policy = enterprise_requirements_policy();
        for cmd in [
            // readers only covered here
            "tail -n1 .env",
            "strings .env",
            "wc -c .env",
            "od -An .env",
            "xxd .env",
            "hexdump -C .env",
            "tac .env",
            "nl .env",
            // non-.env deny globs (*.pem, .ssh/**, .aws/**, .kube/**)
            "cat key.pem",
            "cat .ssh/id_rsa",
            "cat .aws/credentials",
            "cat .kube/config",
            // wrapper stripping / program basename
            "/bin/cat .env",
            "env FOO=1 cat .env",
            "timeout 5 cat .env",
            // path normalization + `..` traversal
            "cat ./.env",
            "cat subdir/../.env",
            // chaining / pipeline — checked per segment
            "ls && cat .env",
            "cat README.md; cat .env",
            "cat .env | head -n1",
            // case-insensitive command match
            "GET-CONTENT .env",
        ] {
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(cmd, cwd()),
                    Some(Decision::Reject(_))
                ),
                "must deny: {cmd}"
            );
        }
        // Ask rules reached through the shell gate (Read + Edit on **/secrets/**).
        for cmd in ["cat secrets/value.txt", "echo x > secrets/new.txt"] {
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(cmd, cwd()),
                    Some(Decision::Ask)
                ),
                "must ask: {cmd}"
            );
        }
    }

    /// Decision-level mirror of the managed-config e2e: asserts the `Decision` the
    /// manager computes across all four entry points (read tool, write/edit tools,
    /// bash rules, shell gate) on the real sentinel paths, no inference.
    #[test]
    fn live_enterprise_e2e_matrix_decision_parity() {
        // What the model can do → the manager function that decides it.
        #[derive(Clone, Copy)]
        enum Vector {
            /// File-read tool / list_dir: `evaluate(AccessKind::Read(..))`.
            ReadTool(&'static str),
            /// write / search_replace / apply_patch: `evaluate(AccessKind::Edit(..))`.
            EditTool(&'static str),
            /// Bash command rules: `evaluate(AccessKind::Bash(..))`.
            Bash(&'static str),
            /// Shell file args: `evaluate_shell_file_access(cmd, cwd)`.
            Shell(&'static str),
        }
        #[derive(Clone, Copy)]
        enum Expect {
            /// Managed deny → `Reject(_)` (the live SENTINEL must never leak).
            Deny,
            /// Managed ask → `Ask` (the live model is prompted).
            Ask,
            /// Not denied/asked → `None` (the live file stays readable / command runs).
            Allowed,
        }
        use Expect::{Allowed, Ask, Deny};
        use Vector::{Bash, EditTool, ReadTool, Shell};

        let policy = enterprise_requirements_policy();
        let matrix: &[(&str, Vector, Expect)] = &[
            // ── file-read tool: real sentinel files (setup.sh) ──
            ("read .env", ReadTool(".env"), Deny),
            ("read .env.staging", ReadTool(".env.staging"), Deny), // **/.env.*
            ("read src/server.pem", ReadTool("src/server.pem"), Deny), // **/*.pem
            (
                "read terraform.tfstate",
                ReadTool("terraform.tfstate"),
                Deny,
            ),
            (
                "read secrets/api_key.txt",
                ReadTool("secrets/api_key.txt"),
                Ask,
            ), // **/secrets/**
            ("read README.md (neg)", ReadTool("README.md"), Allowed),
            ("read src/main.py (neg)", ReadTool("src/main.py"), Allowed),
            // ── file-read tool: every remaining deny/ask glob in the policy ──
            ("read *.key", ReadTool("config/id_rsa.key"), Deny),
            ("read *.p12", ReadTool("cert.p12"), Deny),
            ("read *.pfx", ReadTool("cert.pfx"), Deny),
            ("read *.jks", ReadTool("keystore.jks"), Deny),
            ("read *.keystore", ReadTool("app.keystore"), Deny),
            ("read .ssh/**", ReadTool(".ssh/id_rsa"), Deny),
            ("read .aws/**", ReadTool(".aws/credentials"), Deny),
            (
                "read .config/gcloud/**",
                ReadTool(".config/gcloud/access_tokens.db"),
                Deny,
            ),
            ("read .kube/**", ReadTool(".kube/config"), Deny),
            (
                "read .internal-deploy/**",
                ReadTool(".internal-deploy/config"),
                Deny,
            ),
            ("read .git-credentials", ReadTool(".git-credentials"), Deny),
            (
                "read terraform.tfstate.backup",
                ReadTool("terraform.tfstate.backup"),
                Deny,
            ),
            (
                "read Library/Keychains/**",
                ReadTool("Library/Keychains/login.keychain-db"),
                Deny,
            ),
            (
                "read Library/Mail/** (ask)",
                ReadTool("Library/Mail/Inbox.mbox"),
                Ask,
            ),
            // ── file-read tool: lookalike negatives (must NOT match) ──
            ("read key.pem.txt (neg)", ReadTool("key.pem.txt"), Allowed),
            (
                "read my.env.example (neg)",
                ReadTool("my.env.example"),
                Allowed,
            ),
            // ── write/edit tool: Write(..)/Edit(..) denies + secrets ask ──
            ("edit .env", EditTool(".env"), Deny),
            ("edit .env.local", EditTool(".env.local"), Deny), // **/.env* and **/.env.*
            ("edit src/server.pem", EditTool("src/server.pem"), Deny), // Write(**/*.pem)
            ("edit *.key", EditTool("config/id_rsa.key"), Deny),
            ("edit *.p12", EditTool("cert.p12"), Deny),
            (
                "edit terraform.tfstate",
                EditTool("terraform.tfstate"),
                Deny,
            ),
            ("edit .ssh/**", EditTool(".ssh/authorized_keys"), Deny),
            (
                "edit .internal-deploy/**",
                EditTool(".internal-deploy/config"),
                Deny,
            ),
            (
                "edit Library/Keychains/**",
                EditTool("Library/Keychains/login.keychain-db"),
                Deny,
            ),
            (
                "edit secrets/** (ask)",
                EditTool("secrets/api_key.txt"),
                Ask,
            ),
            // Real-policy asymmetry: *.pfx/*.jks/*.keystore are Read-denied but
            // have NO Write rule, so editing them is allowed (faithful to deploy).
            ("edit *.pfx (no write rule)", EditTool("cert.pfx"), Allowed),
            ("edit README.md (neg)", EditTool("README.md"), Allowed),
            ("edit src/main.py (neg)", EditTool("src/main.py"), Allowed),
            // ── bash command rules: deny set ──
            ("bash rm -rf", Bash("rm -rf /tmp/x"), Deny),
            ("bash sudo", Bash("sudo apt-get update"), Deny),
            ("bash su", Bash("su - root"), Deny),
            // ssh to *.corp.example is deny even though `ssh *` is ask (deny wins).
            ("bash ssh corp-example", Bash("ssh prod.corp.example"), Deny),
            // ── bash command rules: ask set ──
            ("bash kubectl", Bash("kubectl get pods -A"), Ask),
            (
                "bash terraform apply",
                Bash("terraform apply -auto-approve"),
                Ask,
            ),
            ("bash aws", Bash("aws s3 ls"), Ask),
            ("bash gcloud", Bash("gcloud auth list"), Ask),
            ("bash az", Bash("az account show"), Ask),
            ("bash ssh (non-corp-example)", Bash("ssh user@host"), Ask),
            (
                "bash security",
                Bash("security find-generic-password -s x"),
                Ask,
            ),
            ("bash op", Bash("op read op://vault/item"), Ask),
            // ── bash command rules: negatives ──
            ("bash ls (neg)", Bash("ls -la"), Allowed),
            ("bash git status (neg)", Bash("git status"), Allowed),
            // ── shell file-access gate: readers / redirects / substitutions ──
            ("sh cat .env", Shell("cat .env"), Deny),
            ("sh cat .env.staging", Shell("cat .env.staging"), Deny),
            ("sh cat src/server.pem", Shell("cat src/server.pem"), Deny),
            (
                "sh cat terraform.tfstate",
                Shell("cat terraform.tfstate"),
                Deny,
            ),
            ("sh grep FAKE .env", Shell("grep FAKE .env"), Deny),
            ("sh base64 .env", Shell("base64 .env"), Deny),
            ("sh cat 0<.env", Shell("cat 0<.env"), Deny), // fd-prefixed read redirect
            ("sh cat<.env", Shell("cat<.env"), Deny),     // glued read redirect
            ("sh cat $(echo .env)", Shell("cat $(echo .env)"), Ask), // unpinnable substitution
            (
                "sh diff <(cat .env)",
                Shell("diff <(cat .env) /dev/null"),
                Deny,
            ), // process sub
            ("sh cat subdir/../.env", Shell("cat subdir/../.env"), Deny), // `..` traversal
            ("sh cat .ssh/id_rsa", Shell("cat .ssh/id_rsa"), Deny),
            // ── shell file-access gate: writers (file must stay unchanged) ──
            ("sh echo > .env", Shell("echo HACKED > .env"), Deny), // write redirect
            (
                "sh sed -ni .env",
                Shell("sed -ni s/FAKE/HACKED/ .env"),
                Deny,
            ), // in-place sed
            ("sh tee .env", Shell("printf HACKED | tee .env"), Deny), // pipe into tee
            (
                "sh echo > tfstate",
                Shell("echo x > terraform.tfstate"),
                Deny,
            ),
            // ── shell file-access gate: secrets ask + negatives ──
            (
                "sh cat secrets/api_key.txt",
                Shell("cat secrets/api_key.txt"),
                Ask,
            ),
            (
                "sh echo > secrets/new.txt",
                Shell("echo x > secrets/new.txt"),
                Ask,
            ),
            ("sh cat README.md (neg)", Shell("cat README.md"), Allowed),
            (
                "sh cat src/main.py (neg)",
                Shell("cat src/main.py"),
                Allowed,
            ),
        ];

        for &(label, vector, expect) in matrix {
            let decision = match vector {
                ReadTool(path) => policy.evaluate(&AccessKind::Read(Some(path.to_string()))),
                EditTool(path) => policy.evaluate(&AccessKind::Edit(path.to_string())),
                Bash(cmd) => policy.evaluate(&AccessKind::Bash(cmd.to_string())),
                Shell(cmd) => policy.evaluate_shell_file_access(cmd, cwd()),
            };
            match expect {
                Deny => assert!(
                    matches!(decision, Some(Decision::Reject(_))),
                    "[{label}] expected Deny (Reject), got {decision:?}"
                ),
                Ask => assert!(
                    matches!(decision, Some(Decision::Ask)),
                    "[{label}] expected Ask, got {decision:?}"
                ),
                Allowed => assert!(
                    decision.is_none(),
                    "[{label}] expected allowed (None), got {decision:?}"
                ),
            }
        }
    }

    /// Negative controls: legit reads/writes and lookalike names aren't blocked.
    #[test]
    fn adversarial_legitimate_commands_not_overblocked() {
        let policy = enterprise_requirements_policy();
        for cmd in [
            "cat README.md",
            "head -n 5 README.md",
            "tail -n 5 README.md",
            "grep hello README.md",
            "sed -n 1p README.md",
            "wc -c README.md",
            "cut -d: -f1 README.md",
            "sort README.md",
            "uniq README.md",
            "pwd",
            "date",
            "whoami",
            "git status --short",
            "ls && cat README.md",
            "cat my.env.example",
            "cat env.txt",
            "cat key.pem.txt",
            "cat env.dir/README.md",
            "echo ok > scratch.txt",
            "echo x 2>/dev/null",
            "cat README.md 2>&1",
            // Path-moving commands on non-restricted files stay inert.
            "cp README.md backup.md",
            "mv old.txt new.txt",
            "rm scratch.txt",
            "touch newfile.txt",
            "mkdir build",
        ] {
            assert!(
                policy.evaluate_shell_file_access(cmd, cwd()).is_none(),
                "must not over-block: {cmd}"
            );
        }
    }

    /// No Read/Edit/Any rules: skip the shell gate entirely, even for known readers.
    #[test]
    fn adversarial_no_file_rules_means_no_shell_gate() {
        let policy = compiled(vec![bash_rule(RuleAction::Deny, "rm*")]);
        assert!(
            policy
                .evaluate_shell_file_access("cat .env", cwd())
                .is_none()
        );
    }

    /// Local mirror of the grep tool's read-exclude derivation: `Deny` rules on
    /// `Read`/`Any`. Proves a no-restriction policy derives zero read-excludes.
    fn read_deny_globs(config: &PermissionConfig) -> Vec<String> {
        config
            .rules
            .iter()
            .filter(|r| {
                r.action == RuleAction::Deny && matches!(r.tool, ToolFilter::Read | ToolFilter::Any)
            })
            .filter_map(|r| r.pattern.clone())
            .collect()
    }

    /// Read/exfil vectors the shell gate classifies under a policy — reused to
    /// prove a no-restriction policy gates none of them.
    const BYPASS_VECTORS: &[&str] = &[
        "cat .env",
        "grep FAKE .env",
        "base64 .env",
        "cat 0<.env",
        "cat<.env",
        "cat $(echo .env)",
        "diff <(cat .env) /dev/null",
        "echo X > .env",
        "sed -ni s/// .env",
    ];

    /// No-restriction policies (empty / Bash-only / Allow-only-file) must be inert:
    /// gate not armed, every bypass vector declined, zero read-excludes.
    #[test]
    fn h1_no_restriction_policies_are_inert() {
        let policies: [(&str, Vec<PermissionRule>); 3] = [
            ("empty", vec![]),
            (
                "bash-only",
                vec![
                    bash_rule(RuleAction::Deny, "rm -rf *"),
                    bash_rule(RuleAction::Ask, "kubectl *"),
                ],
            ),
            (
                "allow-only-file",
                vec![
                    file_rule(RuleAction::Allow, ToolFilter::Read, "**"),
                    file_rule(RuleAction::Allow, ToolFilter::Edit, "**"),
                ],
            ),
        ];
        for (label, rules) in policies {
            let config = PermissionConfig::new(rules.clone());
            let policy = compiled(rules);
            assert!(
                !policy.has_file_restrictions,
                "[{label}] no-restriction policy must not arm the file gate"
            );
            for cmd in BYPASS_VECTORS {
                assert!(
                    policy.evaluate_shell_file_access(cmd, cwd()).is_none(),
                    "[{label}] inert policy must not gate `{cmd}`"
                );
            }
            assert!(
                read_deny_globs(&config).is_empty(),
                "[{label}] inert policy must derive zero recursive-grep read-excludes"
            );
        }
    }

    /// The policy must not over-match legit look-alikes (direct read or shell gate):
    /// it targets dotfile `.env`/`.env.<x>` and real cert globs, not any `env`/`pem`.
    #[test]
    fn h2_enterprise_policy_does_not_over_match_legit_paths() {
        let policy = enterprise_requirements_policy();
        for path in [
            "environment.txt",
            "foo.env",
            "my.env.example",
            "src/env.rs",
            "environments/config.yaml",
            "README.md",
            "src/main.py",
            "prevent.pem.md",
            ".environment/app.conf",
        ] {
            let direct = policy.evaluate(&AccessKind::Read(Some(path.to_string())));
            assert!(
                !matches!(direct, Some(Decision::Reject(_))),
                "[read {path}] legit look-alike must not be denied, got {direct:?}"
            );
            let shell = policy.evaluate_shell_file_access(&format!("cat {path}"), cwd());
            assert!(
                !matches!(shell, Some(Decision::Reject(_))),
                "[cat {path}] legit look-alike must not be denied, got {shell:?}"
            );
        }
        // Nuance: `**/.env.*` DOES catch a dotfile `.env.<suffix>` (both vectors)…
        for path in [".env.example", ".env.staging"] {
            assert!(
                matches!(
                    policy.evaluate(&AccessKind::Read(Some(path.to_string()))),
                    Some(Decision::Reject(_))
                ),
                "[read {path}] must be denied by **/.env.*"
            );
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(&format!("cat {path}"), cwd()),
                    Some(Decision::Reject(_))
                ),
                "[cat {path}] must be denied by **/.env.*"
            );
        }
        // …but `my.env.example` (not a dotfile) is NOT caught.
        assert!(
            policy
                .evaluate(&AccessKind::Read(Some("my.env.example".to_string())))
                .is_none(),
            "my.env.example must not match **/.env.*"
        );
    }

    /// The shell gate never hard-blocks legit reads; fail-closed cases (glob,
    /// recursion, unpinnable substitution) `Ask`, not `Reject`.
    #[test]
    fn h3_enterprise_gate_never_false_blocks_legit() {
        let policy = enterprise_requirements_policy();
        for cmd in ["cat README.md", "grep foo src/main.py", "wc -l src/main.py"] {
            assert!(
                policy.evaluate_shell_file_access(cmd, cwd()).is_none(),
                "legit read must not be gated: {cmd}"
            );
        }
        for cmd in ["cat *.md", "grep -r foo .", "cat $(echo README.md)"] {
            assert!(
                matches!(
                    policy.evaluate_shell_file_access(cmd, cwd()),
                    Some(Decision::Ask)
                ),
                "fail-closed case must ask, never reject: {cmd}"
            );
        }
    }

    /// A deployment that ships managed config with no `[permission]` rules must see
    /// zero gating (no secrets embedded, only the empty rule set).
    #[test]
    fn unrestricted_enterprise_has_no_file_restrictions() {
        let policy = compiled(vec![]);
        assert!(
            !policy.has_file_restrictions,
            "a deployment with no [permission] rules must not arm the file gate"
        );
        for cmd in BYPASS_VECTORS {
            assert!(
                policy.evaluate_shell_file_access(cmd, cwd()).is_none(),
                "no-restriction enterprise policy must not gate `{cmd}`"
            );
        }
    }
}
