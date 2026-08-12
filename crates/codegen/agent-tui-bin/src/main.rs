//! Single core binary. Product skins are **symlinks** (`scripts/link-product-bins.sh`).

fn main() {
    agent_tui_bin::apply_product_from_invocation();
    agent_tui_bin::run();
}
