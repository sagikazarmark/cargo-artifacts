fn main() {
    if let Err(error) = cargo_artifacts::cli::run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
