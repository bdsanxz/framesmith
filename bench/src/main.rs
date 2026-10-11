//! `bench`: measurements of Framesmith (development only).

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (text, code) = bench::run(&args);
    print!("{text}");
    std::process::exit(code);
}
