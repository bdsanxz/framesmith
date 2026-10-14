//! The runner of the yardstick tests (`docs/dev/gates.md`, section 5).
//!
//! A yardstick test compares Framesmith with a program that is installed outside the repository
//! (FFmpeg, or the reference). The Rust test harness has two outcomes, passed and failed; a
//! comparison whose program is absent is a third, skipped. A test file of yardstick tests is
//! therefore built with `harness = false` and its `main` is [`yardstick_tests!`](crate::yardstick_tests),
//! which hands the tests to [`main`].
//!
//! The runner reads the options of the Rust harness that matter here (names to match, `--exact`,
//! `--skip`, `--list`, `--ignored`, `--include-ignored`) and prints each test in the harness's form.

use std::ffi::OsStr;
use std::io::Write;
use std::path::Path;

/// The program a yardstick test compares with, and how to tell that it is there.
#[derive(Clone, Copy)]
pub struct Needs {
    /// Said in the lines of the run: `ffmpeg and ffprobe`, `the reference`.
    pub program: &'static str,
    /// `Ok` when the program is present; `Err` with where it was not found and what is missing.
    /// Looking starts nothing.
    pub check: fn() -> Result<(), String>,
}

/// One yardstick test.
#[derive(Clone, Copy)]
pub struct Test {
    pub name: &'static str,
    pub run: fn(),
}

/// The names of `programs` that are not a file in any folder of `path`.
pub fn missing_on(path: &OsStr, programs: &[&str]) -> Vec<String> {
    programs
        .iter()
        .filter(|name| !std::env::split_paths(path).any(|folder| is_file(&folder.join(name))))
        .map(|name| (*name).to_string())
        .collect()
}

fn is_file(path: &Path) -> bool {
    path.is_file()
}

fn ffmpeg_present() -> Result<(), String> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let missing = missing_on(&path, &["ffmpeg", "ffprobe"]);
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} {} not found in any folder of the search path",
            missing.join(" and "),
            if missing.len() == 1 { "was" } else { "were" }
        ))
    }
}

impl Needs {
    /// FFmpeg: both `ffmpeg` and `ffprobe` are files in a folder of the search path.
    pub fn ffmpeg() -> Needs {
        Needs {
            program: "ffmpeg and ffprobe",
            check: ffmpeg_present,
        }
    }
}

/// What the harness is asked to do.
struct Options {
    filters: Vec<String>,
    skips: Vec<String>,
    exact: bool,
    list: bool,
    run_ignored: bool,
}

fn options(args: &[String]) -> Options {
    let mut options = Options {
        filters: Vec::new(),
        skips: Vec::new(),
        exact: false,
        list: false,
        run_ignored: false,
    };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--exact" => options.exact = true,
            "--list" => options.list = true,
            "--ignored" | "--include-ignored" => options.run_ignored = true,
            "--skip" => options.skips.extend(args.next().cloned()),
            // Options of the harness that take a value, and are of no use here.
            "--test-threads" | "--color" | "--format" | "--logfile" | "--report-time" | "--shuffle-seed" => {
                args.next();
            }
            other if other.starts_with('-') => {}
            name => options.filters.push(name.to_string()),
        }
    }
    options
}

fn selected(options: &Options, name: &str) -> bool {
    let matches = |filter: &String| {
        if options.exact {
            name == filter
        } else {
            name.contains(filter.as_str())
        }
    };
    (options.filters.is_empty() || options.filters.iter().any(matches)) && !options.skips.iter().any(|skip| name.contains(skip.as_str()))
}

/// Runs the yardstick `tests` as `args` ask, writes the lines of the run to `out`, and gives the
/// exit code: 0 unless a test failed, 101 when one did. Tests are run one at a time, in the order
/// of their names, each on a thread named after it, so that a panic names the test.
pub fn run(args: &[String], needs: &Needs, tests: &[Test], out: &mut dyn Write) -> i32 {
    let options = options(args);
    let mut chosen: Vec<&Test> = tests.iter().filter(|test| selected(&options, test.name)).collect();
    chosen.sort_by_key(|test| test.name);
    let filtered_out = tests.len() - chosen.len();
    let write = |out: &mut dyn Write, text: String| {
        let _ = writeln!(out, "{text}");
        let _ = out.flush();
    };

    if options.list {
        for test in &chosen {
            write(out, format!("{}: test", test.name));
        }
        write(out, format!("\n{} tests, 0 benchmarks", chosen.len()));
        return 0;
    }

    write(
        out,
        format!("\nrunning {} test{}", chosen.len(), if chosen.len() == 1 { "" } else { "s" }),
    );

    if !options.run_ignored {
        for test in &chosen {
            write(
                out,
                format!(
                    "test {} ... ignored, yardstick: needs {}, which it starts from outside",
                    test.name, needs.program
                ),
            );
        }
        write(
            out,
            format!(
                "\ntest result: ok. 0 passed; 0 failed; {} ignored; 0 measured; {filtered_out} filtered out\n",
                chosen.len()
            ),
        );
        return 0;
    }

    // Nothing asked for, nothing to look for: a run that matches no test is an ordinary empty run.
    let absent = if chosen.is_empty() { Ok(()) } else { (needs.check)() };
    if let Err(why) = absent {
        for test in &chosen {
            write(out, format!("test {} ... skipped, {why}; nothing was compared", test.name));
        }
        write(
            out,
            format!(
                "\ntest result: skipped. 0 passed; 0 failed; {} skipped; 0 measured; {filtered_out} filtered out\n",
                chosen.len()
            ),
        );
        return 0;
    }

    let (mut passed, mut failed) = (0, Vec::new());
    for test in &chosen {
        let run = test.run;
        let finished = std::thread::Builder::new()
            .name(test.name.to_string())
            .spawn(run)
            .map(|handle| handle.join().is_ok())
            .unwrap_or(false);
        if finished {
            passed += 1;
            write(out, format!("test {} ... ok", test.name));
        } else {
            failed.push(test.name);
            write(out, format!("test {} ... FAILED", test.name));
        }
    }
    if !failed.is_empty() {
        write(out, "\nfailures:\n".to_string());
        for name in &failed {
            write(out, format!("    {name}"));
        }
    }
    write(
        out,
        format!(
            "\ntest result: {}. {passed} passed; {} failed; 0 skipped; 0 measured; {filtered_out} filtered out\n",
            if failed.is_empty() { "ok" } else { "FAILED" },
            failed.len()
        ),
    );
    if failed.is_empty() {
        0
    } else {
        101
    }
}

/// The `main` of a test file of yardstick tests: reads the command line, runs, and exits.
pub fn main(needs: Needs, tests: &[Test]) -> ! {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = run(&args, &needs, tests, &mut std::io::stdout());
    std::process::exit(code);
}

/// The `main` of a test file built with `harness = false` that holds yardstick tests: `needs`
/// is the [`Needs`] of the program they compare with, and the names are those of the functions.
/// A function that is left out of the list is never run and the compiler says it is unused.
#[macro_export]
macro_rules! yardstick_tests {
    ($needs:expr; $($name:ident),+ $(,)?) => {
        fn main() {
            $crate::yardstick::main(
                $needs,
                &[$($crate::yardstick::Test { name: stringify!($name), run: $name }),+],
            )
        }
    };
}
