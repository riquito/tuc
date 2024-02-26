use assert_cmd::Command;

#[test]
fn it_display_short_help_when_run_without_arguments() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.assert();

    assert.success().stdout(predicates::str::starts_with("tuc"));
}

#[test]
fn it_echo_non_delimited_line() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["-d", "/"]).write_stdin("foobar").assert();

    assert.success().stdout("foobar\n");
}

#[test]
fn it_skips_non_delimited_line_when_requested() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["-s"])
        .write_stdin("one\ntwo\tkeepme\nthree")
        .assert();

    assert.success().stdout("two\tkeepme\n");
}

#[test]
fn it_cut_a_field() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["-f", "2"]).write_stdin("foobar\tbaz").assert();

    assert.success().stdout("baz\n");
}

#[test]
fn it_cut_consecutive_delimiters() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["-d", "-", "-f", "1,3"])
        .write_stdin("foo--bar")
        .assert();

    assert.success().stdout("foobar\n");
}

#[test]
fn it_cut_using_multibyte_delimiters() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["-d", "...", "-f", "2"])
        .write_stdin("foo...bar")
        .assert();

    assert.success().stdout("bar\n");
}

#[test]
fn it_works_on_multiple_lines() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["-f", "2"])
        .write_stdin("hello\nfoobar\tbaz")
        .assert();

    assert.success().stdout("hello\nbaz\n");
}

#[test]
fn it_accepts_values_starting_with_hyphen() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["-f", "-1"]).write_stdin("hello").assert();

    assert.success().stdout("hello\n");
}

#[test]
fn it_compresses_delimiters_when_requested() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["-d", "-", "-p", "-f", "2"])
        .write_stdin("foo---bar")
        .assert();

    assert.success().stdout("bar\n");
}

#[test]
fn it_compresses_delimiters_when_requested_and_handles_boundaries() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["-d", "-", "-p"])
        .write_stdin("--foo---bar--")
        .assert();

    assert.success().stdout("-foo-bar-\n");
}

#[cfg(feature = "regex")]
#[test]
fn it_cuts_on_characters() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["--characters", "2,-2"])
        .write_stdin("😁🤩😝😎")
        .assert();

    assert.success().stdout("🤩😝\n");
}

#[test]
fn it_cuts_on_bytes() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["--bytes", "3:"]).write_stdin("über").assert();

    assert.success().stdout("ber");
}

#[test]
fn it_support_zero_terminated_lines() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["-z", "-d", "_", "-f", "2"])
        .write_stdin("hello_world\0foo_bar")
        .assert();

    assert.success().stdout("world\0bar\0");
}

#[test]
fn it_can_complement_the_fields() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["-m", "-d", " ", "-f", "2"])
        .write_stdin("a b c")
        .assert();

    assert.success().stdout("ac\n");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
    let assert = cmd
        .args(["-m", "-d", " ", "-f", "1:"])
        .write_stdin("a b c")
        .assert();

    assert.failure().stderr("Error: the complement is empty\n");
}

#[test]
fn it_cuts_on_lines() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["--lines", "1,3"]).write_stdin("a\nb\nc").assert();

    assert.success().stdout("a\nc\n");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["--lines", "3,1"]).write_stdin("a\nb\nc").assert();

    assert.success().stdout("c\na\n");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["--lines", "2:3"]).write_stdin("a\nb\nc").assert();

    assert.success().stdout("b\nc\n");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["--complement", "--lines", "2"])
        .write_stdin("a\nb\nc")
        .assert();

    assert.success().stdout("a\nc\n");
}

#[test]
fn it_join_fields() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["-d", " ", "-f", "1,3", "-j"])
        .write_stdin("a b c")
        .assert();

    assert.success().stdout("a c\n");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["-d", " ", "-f", "1,3", "-r", "/"])
        .write_stdin("a b c")
        .assert();

    assert.success().stdout("a/c\n");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["-d", "-", "-f", "2", "-j", "-m"])
        .write_stdin("a-b-c")
        .assert();

    assert.success().stdout("a-c\n");
}

#[test]
fn it_join_lines() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["-l", "1,3"]).write_stdin("a\nb\nc").assert();

    assert.success().stdout("a\nc\n");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["-l", "3,1"]).write_stdin("a\nb\nc").assert();

    assert.success().stdout("c\na\n");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["-l", "1,3", "--no-join"])
        .write_stdin("a\nb\nc")
        .assert();

    assert.success().stdout("ac\n");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["-l", "3,1", "--no-join"])
        .write_stdin("a\nb\nc")
        .assert();

    assert.success().stdout("ca\n");
}

#[test]
fn it_format_fields() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["-f", "Say {1} to our {2}.\nJust {{saying}}"])
        .write_stdin("hello\tworld")
        .assert();

    assert
        .success()
        .stdout("Say hello to our world.\nJust {saying}\n");
}

#[test]
fn it_format_field_1_even_with_no_matching_parameters() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["-f", "Say {1}"]).write_stdin("hello").assert();

    assert.success().stdout("Say hello\n");
}

#[test]
fn it_cuts_using_a_greedy_delimiter() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["-g", "-d", "-", "-f", "1,2"])
        .write_stdin("a---b")
        .assert();

    assert.success().stdout("ab\n");
}

#[cfg(feature = "regex")]
#[test]
fn it_cuts_using_a_regex() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["-e", "[.,]", "-f", "1,5"])
        .write_stdin("a..,,b")
        .assert();

    assert.success().stdout("ab\n");
}

#[cfg(feature = "regex")]
#[test]
fn it_cuts_using_a_greedy_delimiter_and_a_regex() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["-g", "-e", "[.,]", "-f", "1,2"])
        .write_stdin("a..,,b")
        .assert();

    assert.success().stdout("ab\n");
}

#[test]
fn it_accept_any_kind_of_range_as_long_as_its_safe() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["-l", "2:-2"]).write_stdin("a\nb\nc\nd").assert();

    assert.success().stdout("b\nc\n");

    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["-l", "2:-4"]).write_stdin("a\nb\nc\nd").assert();

    assert
        .failure()
        .stderr("Error: Field left value cannot be greater than right value\n");
}

#[test]
fn it_fails_if_there_are_unknown_arguments() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["--whatever"]).write_stdin("foobar").assert();

    assert.failure().stderr(
        "tuc: unexpected arguments [\"--whatever\"]\nTry 'tuc --help' for more information.\n",
    );
}

#[test]
fn it_fails_if_both_join_and_nojoin_are_used_at_once() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["-j", "--no-join"]).write_stdin("foobar").assert();

    assert.failure().stderr(
        "tuc: runtime error. It's not possible to use --join and --no-join simultaneously\n",
    );
}

#[test]
fn it_fails_if_both_replace_and_nojoin_are_used_at_once() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["-r", "x", "--no-join"])
        .write_stdin("foobar")
        .assert();

    assert.failure().stderr(
        "tuc: runtime error. You can't pass --no-join when using --replace, which implies --join\n",
    );
}

#[cfg(feature = "regex")]
#[test]
fn it_fails_when_the_regex_is_malformed() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["-e", "["]).write_stdin("foobar").assert();

    assert.failure().stderr(predicates::str::starts_with(
        "tuc: runtime error. The regular expression is malformed.",
    ));

    let assert = cmd.args(["-e", "[", "-g"]).write_stdin("foobar").assert();

    assert.failure().stderr(predicates::str::starts_with(
        "tuc: runtime error. The regular expression is malformed.",
    ));
}

#[cfg(not(feature = "regex"))]
#[test]
fn does_not_panic_if_attemtping_to_use_regex_arg_with_noregex_build() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["-e", "."]).write_stdin("foobar").assert();

    assert.failure().stderr(
        "tuc: unexpected arguments [\"-e\", \".\"]\nTry 'tuc --help' for more information.\n",
    );
}

#[test]
fn it_emit_output_as_json() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["--json", "-d", "/", "-f", "1,2,1:3"])
        .write_stdin("a/b/c/d")
        .assert();

    assert.success().stdout(
        r#"["a","b","a","b","c"]
"#,
    );
}

#[cfg(feature = "regex")]
#[test]
fn it_emit_output_as_json_even_when_cutting_on_chars() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd
        .args(["--json", "-c", "1,2,1:3"])
        .write_stdin("abcd")
        .assert();

    assert.success().stdout(
        r#"["a","b","a","b","c"]
"#,
    );
}

#[test]
fn it_does_not_allow_to_replace_delimiter_with_json() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["--json", "-r", "x"]).assert();

    assert
        .failure()
        .stderr("tuc: runtime error. The use of --replace with --json is not supported\n");
}

#[test]
fn it_is_not_allowed_to_use_character_with_nojoin() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["-c", "1", "--no-join"]).assert();

    assert.failure().stderr(
        "tuc: runtime error. Since --characters implies --join, you can\'t pass --no-join\n",
    );
}

#[cfg(not(feature = "regex"))]
#[test]
fn it_cannot_use_characters_without_regex() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["-c", "1"]).assert();

    assert.failure().stderr(
        "tuc: runtime error. The use of --characters requires `tuc` to be compiled with `regex` support\n",
    );
}

#[test]
fn it_does_not_support_json_on_lines() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["-l", "1", "--json"]).assert();

    assert.failure().stderr(
        "tuc: runtime error. --json support is available only for --fields and --characters\n",
    );
}

#[test]
fn it_does_not_support_json_on_bytes() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["-b", "1", "--json"]).assert();

    assert.failure().stderr(
        "tuc: runtime error. --json support is available only for --fields and --characters\n",
    );
}

#[test]
fn it_cannot_format_fields_alongside_json() {
    let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();

    let assert = cmd.args(["-f", "a{1}b", "--json"]).assert();

    assert
        .failure()
        .stderr("tuc: runtime error. Cannot format fields when using --json\n");
}
