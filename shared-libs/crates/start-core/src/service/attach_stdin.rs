use std::io;
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::net::UnixStream;

use tokio::sync::mpsc;

/// Dropping the cancel peer wakes an idle input read.
pub(super) fn forward(
    stdin: impl AsFd,
    cancel: UnixStream,
    send: &mpsc::Sender<io::Result<u8>>,
) -> io::Result<()> {
    let mut poll = [
        libc::pollfd {
            fd: stdin.as_fd().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: cancel.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    let mut bytes = [0u8; 4 * crate::CAP_1_KiB];
    loop {
        // SAFETY: Both descriptors and the writable poll array remain live.
        if unsafe { libc::poll(poll.as_mut_ptr(), poll.len() as _, -1) } < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if poll[1].revents != 0 {
            return Ok(());
        }
        match nix::unistd::read(stdin.as_fd(), &mut bytes) {
            Ok(0) => return Ok(()),
            Ok(length) => {
                for byte in &bytes[..length] {
                    if send.blocking_send(Ok(*byte)).is_err() {
                        return Ok(());
                    }
                }
            }
            Err(nix::errno::Errno::EINTR) => continue,
            Err(error) => return Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::net::Shutdown;
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn cancellation_wakes_open_idle_stdin_without_waiting_for_eof() {
        let (stdin, writer) = UnixStream::pair().unwrap();
        let (cancel, peer) = UnixStream::pair().unwrap();
        let (send, _recv) = mpsc::channel(1);
        let mut worker = tokio::task::spawn_blocking(move || forward(stdin, cancel, &send));
        drop(peer);
        let result = tokio::time::timeout(Duration::from_secs(2), &mut worker).await;
        drop(writer);
        if result.is_err() {
            let _ = worker.await;
        }
        result
            .expect("stdin ignored cancellation")
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn receiver_disposal_releases_a_full_input_queue() {
        let (stdin, mut writer) = UnixStream::pair().unwrap();
        let (cancel, peer) = UnixStream::pair().unwrap();
        let (send, mut recv) = mpsc::channel(1);
        let mut worker = tokio::task::spawn_blocking(move || forward(stdin, cancel, &send));
        writer.write_all(b"many input bytes").unwrap();
        assert_eq!(recv.recv().await.unwrap().unwrap(), b'm');
        drop(recv);
        drop(peer);
        let result = tokio::time::timeout(Duration::from_secs(2), &mut worker).await;
        drop(writer);
        if result.is_err() {
            let _ = worker.await;
        }
        result
            .expect("stdin remained blocked on its queue")
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn input_bytes_and_eof_are_preserved_across_chunks() {
        let (stdin, mut writer) = UnixStream::pair().unwrap();
        let (cancel, _peer) = UnixStream::pair().unwrap();
        let (send, mut recv) = mpsc::channel(64);
        let worker = tokio::task::spawn_blocking(move || forward(stdin, cancel, &send));
        let expected = (0..12000).map(|n| (n % 256) as u8).collect::<Vec<_>>();
        writer.write_all(&expected).unwrap();
        writer.shutdown(Shutdown::Write).unwrap();
        let mut actual = Vec::new();
        while let Some(byte) = recv.recv().await {
            actual.push(byte.unwrap());
        }
        worker.await.unwrap().unwrap();
        assert_eq!(actual, expected);
    }
}
