//! `nsh`: the command line with the mobile UI.

fn main() -> anyhow::Result<()> {
    nsh_cli::main_with(vec![nsh_ui_mobile::registry()])
}
