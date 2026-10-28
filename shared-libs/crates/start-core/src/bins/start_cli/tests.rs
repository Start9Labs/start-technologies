use std::ffi::OsString;
use std::io::{IoSlice, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::Path;
use std::process::Command;

use crate::prelude::*;

const CHILD_ARGS: &str = "START_CLI_OUTPUT_ARGS";
const BROKEN_STREAM: &str = "START_CLI_BROKEN_STREAM";

fn child(directory: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "bins::start_cli::tests::run_cli_child",
            "--nocapture",
        ])
        .current_dir(directory)
        .env("HOME", directory)
        .env(CHILD_ARGS, serde_json::to_string(args).unwrap());
    command
}

#[test]
fn run_cli_child() {
    let Ok(args) = std::env::var(CHILD_ARGS) else {
        return;
    };
    let args: Vec<String> = serde_json::from_str(&args).unwrap();
    tokio_rustls::rustls::crypto::ring::default_provider()
        .install_default()
        .ok();
    if let Ok(stream) = std::env::var(BROKEN_STREAM) {
        let (reader, writer) = nix::unistd::pipe().unwrap();
        drop(reader);
        match stream.as_str() {
            "stdout" => nix::unistd::dup2_stdout(&writer).unwrap(),
            "stderr" => nix::unistd::dup2_stderr(&writer).unwrap(),
            _ => panic!("unknown stream"),
        }
    }
    let value = 42;
    match args[0].as_str() {
        "stdout-macros" => {
            print!("print:{value}");
            println!();
            println!("line:{value:04}");
        }
        "stderr-macros" => {
            eprint!("print:{value}");
            eprintln!();
            eprintln!("line:{value:04}");
        }
        "table-output" => {
            let table = prettytable::Table::init(vec![prettytable::Row::new(vec![
                prettytable::Cell::new("value"),
            ])]);
            print_table(&table).unwrap();
        }
        "socket-errors" => {
            super::main([OsString::from("start-cli"), OsString::from("git-info")]);
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let mut stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            let (_peer, _) = listener.accept().unwrap();
            stream.shutdown(Shutdown::Write).unwrap();
            assert_eq!(
                stream.write(b"value").unwrap_err().kind(),
                std::io::ErrorKind::BrokenPipe
            );
            assert_eq!(
                stream
                    .write_vectored(&[IoSlice::new(b"a"), IoSlice::new(b"b")])
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::BrokenPipe,
            );
            println!("socket errors handled");
        }
        _ => super::main(
            std::iter::once(OsString::from("start-cli"))
                .chain(args.into_iter().map(OsString::from)),
        ),
    }
    std::process::exit(0);
}

#[test]
fn stdout_exits_quietly_for_all_display_paths() {
    let directory = tempfile::tempdir().unwrap();
    for args in [
        vec!["stdout-macros"],
        vec!["table-output"],
        vec!["net", "dns", "query", "localhost"],
        vec!["net", "dns", "query", "localhost", "--format", "json"],
        vec![
            "net",
            "dns",
            "query",
            "localhost",
            "--format",
            "json-pretty",
        ],
        vec!["net", "dns", "query", "localhost", "--format", "yaml"],
        vec!["net", "dns", "query", "localhost", "--format", "cbor"],
        vec!["net", "dns", "query", "localhost", "--format", "toml"],
        vec![
            "net",
            "dns",
            "query",
            "localhost",
            "--format",
            "toml-pretty",
        ],
        vec!["git-info"],
        vec!["completions", "bash"],
        vec!["--help"],
    ] {
        let normal = child(directory.path(), &args).output().unwrap();
        assert!(normal.status.success(), "{args:?}: {normal:?}");
        assert!(!normal.stdout.is_empty(), "{args:?}");
        assert!(normal.stderr.is_empty(), "{args:?}: {normal:?}");
        if args == ["stdout-macros"] {
            assert!(normal.stdout.ends_with(b"print:42\nline:0042\n"));
        }

        let broken = child(directory.path(), &args)
            .env(BROKEN_STREAM, "stdout")
            .output()
            .unwrap();
        assert_eq!(broken.status.code(), Some(141), "{args:?}: {broken:?}");
        assert!(broken.stderr.is_empty(), "{args:?}: {broken:?}");
    }
}

#[test]
fn stderr_exits_quietly_and_normal_errors_keep_their_exit_code() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("invalid.json"), b"{").unwrap();
    let args = ["-c", "invalid.json", "git-info"];
    let normal = child(directory.path(), &args).output().unwrap();
    assert_eq!(
        normal.status.code(),
        Some(crate::ErrorKind::Deserialization as i32)
    );
    assert!(!normal.stderr.is_empty(), "{normal:?}");

    for args in [args.as_slice(), &["stderr-macros"]] {
        let broken = child(directory.path(), args)
            .env(BROKEN_STREAM, "stderr")
            .output()
            .unwrap();
        assert_eq!(broken.status.code(), Some(141), "{broken:?}");
    }
    let normal = child(directory.path(), &["stderr-macros"])
        .output()
        .unwrap();
    assert!(normal.status.success(), "{normal:?}");
    assert_eq!(normal.stderr, b"print:42\nline:0042\n");
}

#[test]
fn socket_write_errors_do_not_terminate_the_cli() {
    let directory = tempfile::tempdir().unwrap();
    let output = child(directory.path(), &["socket-errors"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(
        output.stdout.ends_with(b"socket errors handled\n"),
        "{output:?}"
    );
    assert!(output.stderr.is_empty(), "{output:?}");
}
