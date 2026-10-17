//! Check the configured programs using the shared project runner.
fn main() -> std::process::ExitCode {
    your_language::project::run_example(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/01-getting-started/yl.json"
    ))
}
