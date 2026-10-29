//! Shared CLI fixture: an isolated HOME and a captured invocation without a terminal.
#![expect(clippy::unwrap_used, reason = "fixture helpers")]
use assert_cmd::Command;

pub fn command(home: &std::path::Path) -> Command {
    let mut command = Command::cargo_bin("retold").unwrap();
    command
        .env("HOME", home)
        .env_remove("XDG_DATA_HOME")
        .env("PATH", "");
    command
}

pub fn invoke(home: &std::path::Path, args: &[&str], exit: i32) -> String {
    let mut command = command(home);
    let assertion = command.args(args).assert().code(exit);
    let output = assertion.get_output();
    assert_eq!(output.stderr, Vec::<u8>::new());
    String::from_utf8(output.stdout.clone()).unwrap()
}
