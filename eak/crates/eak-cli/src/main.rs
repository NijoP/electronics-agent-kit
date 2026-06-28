//! `eak` binary — a thin shell over the `eak_cli` library composition root.
fn main() -> std::process::ExitCode {
    eak_cli::run_cli()
}
