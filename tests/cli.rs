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

#[test]
fn it_compresses_delimiters_when_requested_and_handles_boundaries() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(&["-d", "-", "-p"])
        .write_stdin("--foo---bar--")
        .assert();

    assert.success().stdout("-foo-bar-\n");
}

#[test]
fn it_cuts_on_characters() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(&["--characters", "2,-2"])
        .write_stdin("😁🤩😝😎")
        .assert();

    assert.success().stdout("🤩😝\n");
}

#[test]
fn it_cuts_on_bytes() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(&["--bytes", "3:"]).write_stdin("über").assert();

    assert.success().stdout("ber");
}

#[test]
fn it_support_zero_terminated_lines() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(&["-z", "-d", "_", "-f", "2"])
        .write_stdin("hello_world\0foo_bar")
        .assert();

    assert.success().stdout("world\0bar\0");
}

#[test]
fn it_can_complement_the_fields() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(&["-m", "-d", " ", "-f", "2"])
        .write_stdin("a b c")
        .assert();

    assert.success().stdout("ac\n");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
    let assert = cmd
        .args(&["-m", "-d", " ", "-f", "1:"])
        .write_stdin("a b c")
        .assert();

    assert.success().stdout("\n");
}

#[test]
fn it_cuts_on_lines() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(&["--lines", "1,3"])
        .write_stdin("a\nb\nc")
        .assert();

    assert.success().stdout("ac");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(&["--lines", "3,1"])
        .write_stdin("a\nb\nc")
        .assert();

    assert.success().stdout("ca");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(&["--lines", "2:3"])
        .write_stdin("a\nb\nc")
        .assert();

    assert.success().stdout("b\nc");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(&["--complement", "--lines", "2"])
        .write_stdin("a\nb\nc")
        .assert();

    assert.success().stdout("ac");
}

#[test]
fn it_join_fields() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(&["-d", " ", "-f", "1,3", "-j"])
        .write_stdin("a b c")
        .assert();

    assert.success().stdout("a c\n");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(&["-d", " ", "-f", "1,3", "-j", "-r", "/"])
        .write_stdin("a b c")
        .assert();

    assert.success().stdout("a/c\n");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(&["-d", "-", "-f", "2", "-j", "-m"])
        .write_stdin("a-b-c")
        .assert();

    assert.success().stdout("a-c\n");
}

#[test]
fn it_join_lines() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(&["-l", "1,3", "-j"])
        .write_stdin("a\nb\nc")
        .assert();

    assert.success().stdout("a\nc");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(&["-l", "3,1", "-j"])
        .write_stdin("a\nb\nc")
        .assert();

    assert.success().stdout("c\na");
}

#[test]
fn it_format_fields() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(&["-f", "Say {1} to our {2}.\nJust {{saying}}"])
        .write_stdin("hello\tworld")
        .assert();

    assert
        .success()
        .stdout("Say hello to our world.\nJust {saying}\n");
}
