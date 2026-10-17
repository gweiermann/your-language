#[path = "backend-hooks/hosts/mod.rs"]
mod hosts;

fn main() {
    if let Err(message) = run() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let language = hosts::load()?;
    let (path, source) = hosts::source_argument()?;
    let program = language
        .prepare_backend(&path, &source)
        .map_err(|e| hosts::render_errors(&e, &path, &source))?;
    let output = program
        .run(&mut hosts::JavaScript::default())
        .map_err(|e| hosts::render_errors(&[e], &path, &source))?;
    print!("{output}");
    Ok(())
}
