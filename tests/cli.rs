use std::process::{Command, Output, Stdio};
use tetris::config::Config;

fn run(args: &[&str], root: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tetris"))
        .args(args)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .stdin(Stdio::null())
        .output()
        .unwrap()
}
#[test]
fn help_and_default_config_work_without_a_terminal_or_side_effects() {
    let dir = tempfile::tempdir().unwrap();
    let help = run(&["--help"], dir.path());
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--config"));
    let defaults = run(&["--print-default-config"], dir.path());
    assert!(defaults.status.success());
    Config::parse(std::str::from_utf8(&defaults.stdout).unwrap()).unwrap();
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}
#[test]
fn invalid_config_is_reported_before_terminal_initialization() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.toml");
    std::fs::write(&path, "[game]\nstart_level = 0").unwrap();
    let output = run(&["--config", path.to_str().unwrap()], dir.path());
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("game.start_level"));
    assert!(!output.stdout.contains(&0x1b));
}
#[test]
fn missing_explicit_config_and_noninteractive_launch_are_clear_errors() {
    let dir = tempfile::tempdir().unwrap();
    let output = run(
        &[
            "--config",
            dir.path().join("missing.toml").to_str().unwrap(),
        ],
        dir.path(),
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot read configuration"));
    let output = run(&[], dir.path());
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("interactive terminal"));
    assert!(!dir.path().join("data").exists());
}
