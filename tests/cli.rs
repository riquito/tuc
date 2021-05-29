use assert_cmd::Command;

#[test]
fn it_echo_non_delimited_line() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.write_stdin("foobar").assert();

    assert.success().stdout("foobar\n");
}

#[test]
fn it_skips_non_delimited_line_when_requested() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(&["-s"])
        .write_stdin("one\ntwo\tkeepme\nthree")
        .assert();

    assert.success().stdout("two\tkeepme\n");
}

#[test]
fn it_cut_a_field() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(&["-f", "2"]).write_stdin("foobar\tbaz").assert();

    assert.success().stdout("baz\n");
}

#[test]
fn it_cut_consecutive_delimiters() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(&["-d", "-", "-f", "1,3"])
        .write_stdin("foo--bar")
        .assert();

    assert.success().stdout("foobar\n");
}

#[test]
fn it_works_on_multiple_lines() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(&["-f", "2"])
        .write_stdin("hello\nfoobar\tbaz")
        .assert();

    assert.success().stdout("hello\nbaz\n");
}

#[test]
fn it_accepts_values_starting_with_hyphen() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(&["-f", "-1"]).write_stdin("hello").assert();

    assert.success().stdout("hello\n");
}

#[test]
fn it_compresses_delimiters_when_requested() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(&["-d", "-", "-p", "-f", "2"])
        .write_stdin("foo---bar")
        .assert();

    assert.success().stdout("bar\n");
}
