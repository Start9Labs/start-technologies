use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWrite};
use tokio::process::Command;

use super::Invoke;
use crate::ErrorKind;

const SIZE: usize = 4 * 1024 * 1024;

fn fixture(script: &str) -> Command {
    let mut cmd = Command::new("python3");
    cmd.args(["-c", script]);
    cmd
}

#[derive(Default)]
struct Sink {
    bytes: usize,
    flushed: bool,
    flushes: usize,
    flush_failure: Option<io::ErrorKind>,
    flush_pending: bool,
    failure: Option<io::ErrorKind>,
    zero: bool,
}

impl AsyncWrite for Sink {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        if let Some(kind) = self.failure {
            return Poll::Ready(Err(io::Error::new(kind, "sink failure")));
        }
        let len = if self.zero { 0 } else { bytes.len().min(37) };
        assert!(bytes[..len].iter().all(|b| *b == b'x'));
        self.bytes += len;
        Poll::Ready(Ok(len))
    }

    fn poll_flush(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.flushed = true;
        self.flushes += 1;
        if self.flush_pending {
            Poll::Pending
        } else if let Some(kind) = self.flush_failure {
            Poll::Ready(Err(io::Error::new(kind, "flush failure")))
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        panic!("borrowed sink must not be shut down")
    }
}

#[tokio::test]
async fn command_stream_short_writes_and_concurrent_input_stderr() {
    let bytes = vec![b'i'; SIZE];
    let mut input = &bytes[..];
    let mut sink = Sink::default();
    let result = fixture("import sys; sys.stderr.buffer.write(b'e'*4194304); sys.stderr.flush(); sys.stdout.buffer.write(b'x'*4194304); sys.stdout.flush(); assert len(sys.stdin.buffer.read()) == 4194304")
        .input(Some(&mut input))
        .capture(false)
        .output_to(&mut sink)
        .timeout(Some(Duration::from_secs(10)))
        .invoke(ErrorKind::Filesystem)
        .await.unwrap();
    assert!(result.is_empty());
    assert_eq!(sink.bytes, SIZE);
    assert!(sink.flushed);
    assert_eq!(sink.flushes, 1);
}

#[tokio::test]
async fn command_stream_bounded_duplex() {
    let (mut writer, mut reader) = tokio::io::duplex(1024);
    let mut cmd = fixture("import sys; sys.stdout.buffer.write(b'x'*4194304)");
    let producer = async {
        cmd.output_to(&mut writer)
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        drop(writer);
    };
    let consumer = async {
        let mut total = 0;
        let mut buf = [0; 257];
        loop {
            let n = reader.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            assert!(buf[..n].iter().all(|b| *b == b'x'));
            total += n;
        }
        assert_eq!(total, SIZE);
    };
    tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(producer, consumer);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn command_stream_nonzero_retains_stderr() {
    let mut sink = Sink::default();
    let err = fixture("import sys; sys.stdout.buffer.write(b'x'*4194304); sys.stdout.flush(); sys.stderr.write('export failed'); sys.exit(7)")
        .output_to(&mut sink).invoke(ErrorKind::Filesystem).await.unwrap_err();
    assert_eq!(sink.bytes, SIZE);
    assert!(err.to_string().contains("export failed"));
    assert_eq!(err.kind, ErrorKind::Filesystem);
    assert!(!sink.flushed);
}

#[tokio::test]
async fn command_stream_sink_failures() {
    for (zero, failure, expected) in [
        (true, None, io::ErrorKind::WriteZero),
        (
            false,
            Some(io::ErrorKind::BrokenPipe),
            io::ErrorKind::BrokenPipe,
        ),
    ] {
        let mut sink = Sink {
            zero,
            failure,
            ..Sink::default()
        };
        let err = fixture("import sys; sys.stdout.buffer.write(b'x'*4194304)")
            .output_to(&mut sink)
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap_err();
        assert_eq!(
            err.source.downcast_ref::<io::Error>().unwrap().kind(),
            expected
        );
        assert!(!sink.flushed);
    }
}

#[tokio::test]
async fn command_stream_pipeline_and_old_capture() {
    let mut sink = Sink::default();
    let mut second = fixture(
        "import sys; sys.stderr.buffer.write(b'e'*4194304); sys.stderr.flush(); sys.stdout.buffer.write(sys.stdin.buffer.read())",
    );
    let mut third = Command::new("cat");
    fixture("import sys; sys.stdout.buffer.write(b'x'*4194304)")
        .pipe(&mut second)
        .pipe(&mut third)
        .output_to(&mut sink)
        .timeout(Some(Duration::from_secs(10)))
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    assert_eq!(sink.bytes, SIZE);
    assert!(sink.flushed);
    assert_eq!(
        Command::new("printf")
            .arg("captured")
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap(),
        b"captured"
    );
    assert_eq!(
        Command::new("printf")
            .arg("piped")
            .pipe(&mut Command::new("cat"))
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap(),
        b"piped"
    );
    let mut input = &b"input"[..];
    assert_eq!(
        Command::new("cat")
            .input(Some(&mut input))
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap(),
        b"input"
    );
    assert!(
        fixture("pass")
            .capture(false)
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap()
            .is_empty()
    );
}

#[cfg(target_os = "linux")]
async fn assert_dead(pid: u32) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while std::path::Path::new(&format!("/proc/{pid}")).exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn command_stream_cancellation_supervises_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let mut cmds: Vec<_> = (0..3)
        .map(|i| {
            let mut cmd = fixture(
                "import os,sys,time; open(sys.argv[1],'w').write(str(os.getpid())); time.sleep(60)",
            );
            cmd.arg(dir.path().join(i.to_string()));
            cmd
        })
        .collect();
    let mut third = cmds.pop().unwrap();
    let mut second = cmds.pop().unwrap();
    let mut root = cmds.pop().unwrap();
    let mut sink = Sink::default();
    let mut extended = root.pipe(&mut second);
    let mut run = Box::pin(
        extended
            .pipe(&mut third)
            .output_to(&mut sink)
            .invoke(ErrorKind::Filesystem),
    );
    let pids = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            tokio::select! {
                result = &mut run => panic!("unexpected completion: {result:?}"),
                _ = tokio::time::sleep(Duration::from_millis(10)) => {}
            }
            let pids: Option<Vec<u32>> = (0..3)
                .map(|i| {
                    std::fs::read_to_string(dir.path().join(i.to_string()))
                        .ok()?
                        .parse()
                        .ok()
                })
                .collect();
            if let Some(pids) = pids {
                break pids;
            }
        }
    })
    .await
    .unwrap();
    drop(run);
    for pid in pids {
        assert_dead(pid).await;
    }
}

#[tokio::test]
async fn command_capture_concurrent_input_stderr() {
    let bytes = vec![b'i'; SIZE];
    let mut input = &bytes[..];
    let result = fixture("import sys; sys.stderr.buffer.write(b'e'*4194304); sys.stderr.flush(); sys.stdout.buffer.write(b'x'*4194304); sys.stdout.flush(); assert len(sys.stdin.buffer.read()) == 4194304")
        .input(Some(&mut input))
        .timeout(Some(Duration::from_secs(10)))
        .invoke(ErrorKind::Filesystem)
        .await.unwrap();
    assert_eq!(result, vec![b'x'; SIZE]);
}

#[tokio::test]
async fn command_stream_pipeline_checks_upstream_exit() {
    let mut sink = Sink::default();
    let err = fixture("import sys; sys.stderr.write('upstream failed'); sys.exit(9)")
        .pipe(&mut Command::new("cat"))
        .output_to(&mut sink)
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("upstream failed"));
    assert!(!sink.flushed);
}

#[tokio::test]
async fn command_stream_flush_failure_and_timeout() {
    let mut sink = Sink {
        flush_failure: Some(io::ErrorKind::BrokenPipe),
        ..Sink::default()
    };
    let err = fixture("import sys; sys.stdout.write('x')")
        .output_to(&mut sink)
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap_err();
    assert_eq!(sink.bytes, 1);
    assert_eq!(sink.flushes, 1);
    assert_eq!(
        err.source.downcast_ref::<io::Error>().unwrap().kind(),
        io::ErrorKind::BrokenPipe
    );

    let mut sink = Sink {
        flush_pending: true,
        ..Sink::default()
    };
    let err = fixture("import sys; sys.stdout.write('x')")
        .output_to(&mut sink)
        .timeout(Some(Duration::from_millis(150)))
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Timeout);
    assert_eq!(sink.bytes, 1);
    assert!(sink.flushed);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn command_stream_sink_failure_supervises_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let mut root = fixture(
        "import os,sys,time; open(sys.argv[1],'w').write(str(os.getpid())); sys.stdout.buffer.write(b'x'*4194304); sys.stdout.flush(); time.sleep(60)",
    );
    let relay = "import os,sys,shutil,time; open(sys.argv[1],'w').write(str(os.getpid())); shutil.copyfileobj(sys.stdin.buffer,sys.stdout.buffer); sys.stdout.flush(); time.sleep(60)";
    let mut second = fixture(relay);
    let mut third = fixture(relay);
    root.arg(dir.path().join("0"));
    second.arg(dir.path().join("1"));
    third.arg(dir.path().join("2"));
    let mut sink = Sink {
        failure: Some(io::ErrorKind::BrokenPipe),
        ..Sink::default()
    };
    let err = root
        .pipe(&mut second)
        .pipe(&mut third)
        .output_to(&mut sink)
        .timeout(Some(Duration::from_secs(5)))
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap_err();
    assert_eq!(
        err.source.downcast_ref::<io::Error>().unwrap().kind(),
        io::ErrorKind::BrokenPipe
    );
    for i in 0..3 {
        let pid = std::fs::read_to_string(dir.path().join(i.to_string()))
            .unwrap()
            .parse()
            .unwrap();
        assert_dead(pid).await;
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn command_stream_timeout_includes_blocked_input() {
    let dir = tempfile::tempdir().unwrap();
    let (_writer, mut input) = tokio::io::duplex(1);
    let mut sink = Sink::default();
    let err = fixture(
        "import os,sys; open(sys.argv[1],'w').write(str(os.getpid())); sys.stdin.buffer.read()",
    )
    .arg(dir.path().join("pid"))
    .input(Some(&mut input))
    .output_to(&mut sink)
    .timeout(Some(Duration::from_millis(150)))
    .invoke(ErrorKind::Filesystem)
    .await
    .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Timeout);
    assert!(!sink.flushed);
    let pid = std::fs::read_to_string(dir.path().join("pid"))
        .unwrap()
        .parse()
        .unwrap();
    assert_dead(pid).await;
}

#[tokio::test]
async fn command_stream_timeout_includes_blocked_output() {
    let (mut writer, _reader) = tokio::io::duplex(1);
    let err = fixture("import sys; sys.stdout.buffer.write(b'x'*4194304)")
        .output_to(&mut writer)
        .timeout(Some(Duration::from_millis(100)))
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Timeout);
}
