use pulse::cli;

fn main() {
    let result = cli::run(cli::parse());
    if let Err(err) = result {
        cli::print_error(&err);
        std::process::exit(1);
    }
}
