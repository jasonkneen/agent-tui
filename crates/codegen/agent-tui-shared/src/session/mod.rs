use std::path::PathBuf;

pub mod info;

pub use info::Info;

// Re-export shared feedback wire types used by downstream crates
// (e.g. agent-tui-pager-render).
pub use prod_mc_cli_chat_proxy_types::feedback_types::FeedbackTerminalInfo;

pub fn session_dir(info: &Info) -> PathBuf {
    agent_tui_tools::util::grok_home::sessions_cwd_dir(&info.cwd).join(info.id.to_string())
}
