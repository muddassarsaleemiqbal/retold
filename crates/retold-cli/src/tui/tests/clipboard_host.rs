//! A clipboard that never reads stdin must still time out and leave no direct child.
use super::*;

const LARGE_INPUT_BYTES: usize = 1_048_576;
const LONG_SLEEP_SECONDS: &str = "8";

#[test]
fn large_clipboard_input_does_not_block_on_a_nonreading_child() {
    let result = copy_with(
        &["/bin/sleep", LONG_SLEEP_SECONDS],
        &"x".repeat(LARGE_INPUT_BYTES),
    );
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
}

#[test]
fn timeout_kills_and_reaps_the_direct_child() {
    let child = Command::new("/bin/sleep")
        .arg(LONG_SLEEP_SECONDS)
        .spawn()
        .unwrap();
    let pid = child.id().to_string();
    let result = wait(child, Duration::ZERO);
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
    let mut check = Command::new("/bin/kill");
    check.args(["-0", &pid]);
    check.stdout(Stdio::null()).stderr(Stdio::null());
    assert!(
        !check.status().unwrap().success(),
        "child {pid} still exists"
    );
}
