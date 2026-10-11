fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (text, code) = parity::cli::run(&args);
    print!("{text}");
    std::process::exit(code);
}
