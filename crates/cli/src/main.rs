use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let code = cli::run(&args, &mut std::io::stdout().lock(), &mut std::io::stderr().lock());
    ExitCode::from(code)
}
