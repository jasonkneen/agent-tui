//! Single core binary. Product skins are **symlinks** to this file
//! (`scripts/link-product-bins.sh`), not separate linked copies.
//!
//! ```sh
//! cargo build -p agent-tui-bin
//! ./scripts/link-product-bins.sh   # grok, lazartui, codex, claude, hermes, agent-multi → agent-tui
//! ```

fn main() {
    agent_tui_bin::apply_product_from_invocation();
    agent_tui_bin::run();
}
