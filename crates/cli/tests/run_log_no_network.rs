//! A run opens no network connection (`docs/spec/run-log.md` section 8).
//!
//! The run itself is watched under a system sandbox profile that forbids a connection by the
//! evidence script of the ticket; a test cannot do that, since the core suite already runs under a
//! sandbox profile and the system does not nest them. What a test can hold is that the program does
//! not even import a function that makes a connection.

use std::process::Command;

/// The functions of the system's network interface (`socket(2)`, `connect(2)` and what resolves a
/// name or listens). A program that imports none of them cannot open a connection.
const NETWORK_FUNCTIONS: [&str; 22] = [
    "_socket",
    "_socketpair",
    "_connect",
    "_connectx",
    "_bind",
    "_listen",
    "_accept",
    "_accept4",
    "_send",
    "_sendto",
    "_sendmsg",
    "_recv",
    "_recvfrom",
    "_recvmsg",
    "_getaddrinfo",
    "_getnameinfo",
    "_gethostbyname",
    "_gethostbyname2",
    "_gethostbyaddr",
    "_getifaddrs",
    "_SCNetworkReachabilityCreateWithName",
    "_CFSocketCreate",
];

fn imported_symbols() -> Vec<String> {
    let output = Command::new("/usr/bin/nm")
        .arg("-u")
        .arg(env!("CARGO_BIN_EXE_framesmith"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.trim().to_string())
        .collect()
}

#[test]
fn the_framesmith_program_imports_no_function_that_opens_a_network_connection() {
    let symbols = imported_symbols();
    // The control: the list is the program's real imports, among them the calls the log makes to write a file.
    for expected in ["_open", "_write", "_getenv"] {
        assert!(
            symbols.iter().any(|symbol| symbol == expected),
            "{expected} is not among {symbols:?}"
        );
    }
    let found: Vec<_> = symbols
        .iter()
        .filter(|symbol| NETWORK_FUNCTIONS.contains(&symbol.as_str()))
        .collect();
    assert!(found.is_empty(), "the program imports {found:?}");
}

#[test]
fn the_list_of_network_functions_catches_a_program_that_uses_the_network() {
    // A system program that connects: `nm` shows its imports the same way as for the framesmith program.
    let output = Command::new("/usr/bin/nm").arg("-u").arg("/usr/bin/nc").output().unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let symbols: Vec<&str> = text.lines().map(str::trim).collect();
    assert!(
        symbols.iter().any(|symbol| NETWORK_FUNCTIONS.contains(symbol)),
        "no network function is among the imports of /usr/bin/nc: {symbols:?}"
    );
}
