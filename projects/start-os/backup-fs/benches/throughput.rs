//! End-to-end throughput benchmarks for the redesigned backup-fs, run
//! under criterion.
//!
//! Unlike a per-iteration mount, the filesystem is mounted **once** per
//! benchmark group (in the bench function, outside `b.iter`), so the timed
//! closure measures only the workload — not PBKDF2 key derivation or the
//! FUSE mount/unmount handshake. Every workload runs through the real
//! mounted filesystem (the fully-integrated block store + ChaCha20 +
//! Reed-Solomon path).
//!
//! Workloads: large sequential write/read, many small files, scattered
//! partial overwrites, and a single-block incremental edit — plus a
//! one-shot report of how many block files an incremental edit rewrites
//! (the backing-store-independent metric that matters for rsync/rclone).
//!
//! Run with: `cargo bench --bench throughput`

use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, SystemTime};

use backupfs::{BackupFS, BackupFSOptions};
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use fuser::{MountOption, Session};

const MIB: usize = 1024 * 1024;
const SEQ_SIZE: usize = 64 * MIB;
const SMALL_FILES: usize = 1000;
const RANDOM_OPS: usize = 1000;

/// A filesystem mounted once and torn down on drop. Construction pays the
/// PBKDF2 + mount cost a single time, keeping it out of the timed loop.
struct Harness {
    mnt: tempdir::TempDir,
    data: tempdir::TempDir,
    umount: Option<fuser::SessionUnmounter>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Harness {
    fn mount() -> Self {
        let data = tempdir::TempDir::new("bench_data").unwrap();
        let mnt = tempdir::TempDir::new("bench_mnt").unwrap();
        let opt = vec![
            MountOption::FSName("backup-fs".to_string()),
            MountOption::AutoUnmount,
        ];
        let data_dir = data.path().to_owned();
        let mnt_dir = mnt.path().to_owned();
        let (tx, rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
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
        let umount = rx.recv().unwrap();
        Harness {
            mnt,
            data,
            umount: Some(umount),
            thread: Some(thread),
        }
    }

    fn mnt(&self) -> &Path {
        self.mnt.path()
    }

    fn contents_dir(&self) -> std::path::PathBuf {
        self.data.path().join("contents")
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        if let Some(mut u) = self.umount.take() {
            let _ = u.unmount();
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn write_durable(path: &Path, data: &[u8]) {
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(data).unwrap();
    f.sync_all().unwrap();
}

fn count_files(dir: &Path) -> usize {
    let mut n = 0;
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            if e.metadata().unwrap().is_dir() {
                stack.push(e.path());
            } else {
                n += 1;
            }
        }
    }
    n
}

fn count_modified_after(dir: &Path, mark: SystemTime) -> usize {
    let mut n = 0;
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let m = e.metadata().unwrap();
            if m.is_dir() {
                stack.push(e.path());
            } else if m.modified().unwrap() > mark {
                n += 1;
            }
        }
    }
    n
}

fn bench_sequential_write(c: &mut Criterion) {
    let h = Harness::mount();
    let path = h.mnt().join("seqw.bin");
    let buf = vec![0xA5u8; SEQ_SIZE];
    let mut group = c.benchmark_group("sequential_write");
    group.throughput(Throughput::Bytes(SEQ_SIZE as u64));
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.bench_function("64MiB", |b| {
        b.iter(|| write_durable(&path, &buf));
    });
    group.finish();
}

fn bench_sequential_read(c: &mut Criterion) {
    let h = Harness::mount();
    let path = h.mnt().join("seqr.bin");
    write_durable(&path, &vec![0xA5u8; SEQ_SIZE]);
    let mut group = c.benchmark_group("sequential_read");
    group.throughput(Throughput::Bytes(SEQ_SIZE as u64));
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.bench_function("64MiB", |b| {
        b.iter(|| {
            let data = std::fs::read(&path).unwrap();
            black_box(data.len());
        });
    });
    group.finish();
}

fn bench_small_files(c: &mut Criterion) {
    let h = Harness::mount();
    let dir = h.mnt().join("small");
    std::fs::create_dir_all(&dir).unwrap();
    let payload = vec![0x5Au8; 4096];
    let mut group = c.benchmark_group("small_files");
    group.throughput(Throughput::Elements(SMALL_FILES as u64));
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.bench_function("1000x4KiB", |b| {
        b.iter(|| {
            for i in 0..SMALL_FILES {
                std::fs::write(dir.join(format!("f{i:04}")), &payload).unwrap();
            }
        });
    });
    group.finish();
}

fn bench_random_write(c: &mut Criterion) {
    let h = Harness::mount();
    let path = h.mnt().join("rand.bin");
    write_durable(&path, &vec![0u8; SEQ_SIZE]);
    let patch = vec![0x33u8; 4096];
    let mut group = c.benchmark_group("random_4KiB_write");
    group.throughput(Throughput::Elements(RANDOM_OPS as u64));
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.bench_function("1000ops", |b| {
        b.iter(|| {
            let mut f = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
            for i in 0..RANDOM_OPS {
                let off = ((i * 1_048_583) % (SEQ_SIZE - 4096)) as u64;
                f.seek(SeekFrom::Start(off)).unwrap();
                f.write_all(&patch).unwrap();
            }
            f.sync_all().unwrap();
        });
    });
    group.finish();
}

fn bench_medium_files(c: &mut Criterion) {
    // Many medium (64 KiB) files: with content packing on (default) each is a
    // single extent appended to a shared content segment; with
    // BACKUPFS_PACK_MAX=4096 each instead gets its own block file (the A/B).
    let h = Harness::mount();
    let dir = h.mnt().join("medium");
    std::fs::create_dir_all(&dir).unwrap();
    let payload = vec![0x6Du8; 64 * 1024];
    let n = 500usize;
    let mut group = c.benchmark_group("medium_files");
    group.throughput(Throughput::Elements(n as u64));
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.bench_function("500x64KiB", |b| {
        b.iter(|| {
            for i in 0..n {
                write_durable(&dir.join(format!("m{i:04}")), &payload);
            }
        });
    });
    group.finish();
}

fn bench_large_dir_create(c: &mut Criterion) {
    // Creating many files in ONE directory. With directory spilling on
    // (default), each create rewrites a single bucket — O(1). With
    // BACKUPFS_DIR_SPILL set huge the listing stays inline and every create
    // re-serializes the whole growing listing — O(n) per create, O(n²)
    // total — which is the small-file backup bottleneck this fixes.
    let h = Harness::mount();
    let n = 8000usize;
    let counter = std::cell::Cell::new(0u64);
    let mut group = c.benchmark_group("large_dir_create");
    group.throughput(Throughput::Elements(n as u64));
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.bench_function("8000_in_one_dir", |b| {
        b.iter(|| {
            let k = counter.get();
            counter.set(k + 1);
            let dir = h.mnt().join(format!("d{k}"));
            std::fs::create_dir(&dir).unwrap();
            for i in 0..n {
                std::fs::write(dir.join(format!("f{i:05}")), b"x").unwrap();
            }
        });
    });
    group.finish();
}

fn bench_incremental_edit(c: &mut Criterion) {
    let h = Harness::mount();
    let path = h.mnt().join("incr.bin");
    write_durable(&path, &vec![0xA5u8; SEQ_SIZE]);

    // One-shot, backing-store-independent metric: how many block files does
    // a 1-byte edit rewrite? Printed once (criterion times latency below).
    let total_blocks = count_files(&h.contents_dir());
    let mark = SystemTime::now();
    std::thread::sleep(Duration::from_millis(20));
    {
        let mut f = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        f.seek(SeekFrom::Start((SEQ_SIZE / 2) as u64 + 7)).unwrap();
        f.write_all(b"!").unwrap();
        f.sync_all().unwrap();
    }
    let rewritten = count_modified_after(&h.contents_dir(), mark);
    eprintln!(
        "\nincremental edit: 1 byte changed → {rewritten} of {total_blocks} block files rewritten \
         (rsync/rclone re-send only the changed block)\n"
    );

    let mut group = c.benchmark_group("incremental_edit");
    group.sample_size(50);
    group.warm_up_time(Duration::from_secs(1));
    group.bench_function("1byte+fsync", |b| {
        b.iter(|| {
            let mut f = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
            f.seek(SeekFrom::Start((SEQ_SIZE / 2) as u64 + 7)).unwrap();
            f.write_all(b"!").unwrap();
            f.sync_all().unwrap();
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_sequential_write,
    bench_sequential_read,
    bench_small_files,
    bench_medium_files,
    bench_random_write,
    bench_large_dir_create,
    bench_incremental_edit,
);
criterion_main!(benches);
