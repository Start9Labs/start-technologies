//! Throughput + incremental-cost benchmark for the redesigned backup-fs.
//!
//! Mounts a fresh encrypted filesystem on a temp dir and drives a handful
//! of representative workloads:
//!   * large sequential write / read
//!   * many small files
//!   * scattered partial overwrites
//!   * an incremental edit, measuring how many block files actually change
//!
//! Run with: `cargo run --release --example benchmark`
//!
//! Numbers reflect the backing store the temp dir lives on (here, the test
//! host's local disk / tmpfs) plus the per-block ChaCha20 + Reed-Solomon
//! cost. The *incremental* result — blocks rewritten per edit — is backing-
//! store independent and is the headline property for rsync/rclone backups.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Instant, SystemTime};

use backupfs::{BackupFS, BackupFSOptions};
use fuser::{MountOption, Session};

const MIB: usize = 1024 * 1024;

fn with_mount(data: &Path, body: impl FnOnce(&Path)) {
    let mnt = tempdir::TempDir::new("bench_mnt").unwrap();
    let opt = vec![
        MountOption::FSName("backup-fs".to_string()),
        MountOption::AutoUnmount,
    ];
    let data_dir = data.to_owned();
    let mnt_dir = mnt.path().to_owned();
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        let fs = BackupFS::new(BackupFSOptions {
            data_dir,
            setuid_support: false,
            password: "benchmark".to_string(),
            file_size_padding: None,
            readonly: false,
            idmapped_root: vec![],
        })
        .unwrap();
        let mut session = Session::new(fs, &mnt_dir, &opt).unwrap();
        tx.send(session.unmount_callable()).unwrap();
        session.run().unwrap();
    });
    let mut umount = rx.recv().unwrap();
    body(mnt.path());
    let _ = umount.unmount();
    let _ = handle.join();
}

fn mbps(bytes: usize, dt: std::time::Duration) -> f64 {
    (bytes as f64 / MIB as f64) / dt.as_secs_f64()
}

fn count_dir_files(dir: &Path) -> (usize, u64) {
    let mut n = 0;
    let mut bytes = 0;
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let m = e.metadata().unwrap();
            if m.is_dir() {
                stack.push(e.path());
            } else {
                n += 1;
                bytes += m.len();
            }
        }
    }
    (n, bytes)
}

fn main() {
    let data = tempdir::TempDir::new("bench_data").unwrap();
    println!("backup-fs benchmark (block store + ChaCha20 + Reed-Solomon ECC)\n");

    with_mount(data.path(), |mnt| {
        // ── 1. Large sequential write ──
        let big = mnt.join("big.bin");
        let total = 256 * MIB;
        let chunk = vec![0xA5u8; 4 * MIB];
        let t = Instant::now();
        {
            let mut f = std::fs::File::create(&big).unwrap();
            let mut written = 0;
            while written < total {
                f.write_all(&chunk).unwrap();
                written += chunk.len();
            }
            f.sync_all().unwrap();
        }
        let dt = t.elapsed();
        println!(
            "sequential write : {:7.1} MiB/s  ({} MiB)",
            mbps(total, dt),
            total / MIB
        );

        // ── 2. Large sequential read ──
        let t = Instant::now();
        {
            let mut f = std::fs::File::open(&big).unwrap();
            let mut buf = vec![0u8; 4 * MIB];
            let mut read = 0;
            while let Ok(n) = f.read(&mut buf) {
                if n == 0 {
                    break;
                }
                read += n;
            }
            assert_eq!(read, total);
        }
        let dt = t.elapsed();
        println!(
            "sequential read  : {:7.1} MiB/s  ({} MiB)",
            mbps(total, dt),
            total / MIB
        );

        // ── 3. Many small files ──
        let dir = mnt.join("small");
        std::fs::create_dir(&dir).unwrap();
        let n_small = 5000;
        let small = vec![0x5Au8; 4096];
        let t = Instant::now();
        for i in 0..n_small {
            std::fs::write(dir.join(format!("f{i:05}")), &small).unwrap();
        }
        let dt = t.elapsed();
        println!(
            "small files      : {:7.0} files/s ({} × 4 KiB)",
            n_small as f64 / dt.as_secs_f64(),
            n_small
        );

        // ── 4. Scattered partial overwrites ──
        let n_writes = 2000;
        let patch = vec![0x33u8; 4096];
        let t = Instant::now();
        {
            let mut f = std::fs::OpenOptions::new().write(true).open(&big).unwrap();
            for i in 0..n_writes {
                // Spread writes across the whole file, striding by a prime
                // so they hit many different blocks.
                let off = ((i * 1_048_583) % (total - 4096)) as u64;
                f.seek(SeekFrom::Start(off)).unwrap();
                f.write_all(&patch).unwrap();
            }
            f.sync_all().unwrap();
        }
        let dt = t.elapsed();
        println!(
            "random 4 KiB write: {:7.0} ops/s   ({} writes)",
            n_writes as f64 / dt.as_secs_f64(),
            n_writes
        );
    });

    // ── 5. Incremental edit: how many block files change for a 1-byte edit? ──
    let (files_before, bytes_before) = count_dir_files(&data.path().join("contents"));
    let mark = SystemTime::now();
    // brief gap so mtimes are strictly after `mark`
    std::thread::sleep(std::time::Duration::from_millis(20));
    with_mount(data.path(), |mnt| {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(mnt.join("big.bin"))
            .unwrap();
        f.seek(SeekFrom::Start(128 * MIB as u64 + 7)).unwrap();
        f.write_all(b"!").unwrap();
        f.sync_all().unwrap();
    });
    let changed = {
        let mut changed = 0;
        let mut stack = vec![data.path().join("contents")];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let m = e.metadata().unwrap();
                if m.is_dir() {
                    stack.push(e.path());
                } else if m.modified().unwrap() > mark {
                    changed += 1;
                }
            }
        }
        changed
    };

    println!(
        "\nincremental edit : 1 byte changed → {} of {} block files rewritten",
        changed, files_before
    );
    println!(
        "                   ({:.0} MiB of content on disk; rsync/rclone re-sends only changed blocks)",
        bytes_before as f64 / MIB as f64
    );
}
