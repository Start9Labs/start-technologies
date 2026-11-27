//! Builds a large `OrdValue`, asks the kernel to page out every mapping, then reads it back.
//!
//! `cargo run --release --example arena_rss [--features arena] -- <dir> <megabytes>`

use std::time::Instant;

use imbl_value::{ord_json, OrdValue};

fn status() -> String {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    status
        .lines()
        .filter(|l| {
            l.starts_with("RssAnon") || l.starts_with("RssFile") || l.starts_with("RssShmem")
        })
        .map(|l| l.split_whitespace().skip(1).collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join(" / ")
}

fn page_out_everything() {
    let maps = std::fs::read_to_string("/proc/self/maps").unwrap();
    for line in maps.lines() {
        let range = line.split_whitespace().next().unwrap();
        let (start, end) = range.split_once('-').unwrap();
        let start = usize::from_str_radix(start, 16).unwrap();
        let end = usize::from_str_radix(end, 16).unwrap();
        unsafe { libc::madvise(start as *mut _, end - start, libc::MADV_PAGEOUT) };
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().expect("scratch directory");
    let megabytes: usize = args.next().expect("megabytes").parse().unwrap();
    #[cfg(feature = "arena")]
    imbl_value::arena::init(std::path::Path::new(&dir)).unwrap();
    #[cfg(not(feature = "arena"))]
    let _ = dir;

    let mut db = ord_json!({ "packages": {} });
    let entries = megabytes * 1024;
    let started = Instant::now();
    for i in 0..entries {
        db["packages"].as_object_mut().unwrap().insert(
            format!("pkg-{i:08}").into(),
            ord_json!({
                "title": format!("Package {i}"),
                "notes": "x".repeat(900),
                "versions": [i, i + 1, i + 2],
            }),
        );
    }
    println!("built {entries} entries in {:?}", started.elapsed());
    println!(
        "resident after build      (anon / file / shmem): {}",
        status()
    );
    page_out_everything();
    std::thread::sleep(std::time::Duration::from_secs(2));
    println!(
        "resident after page-out   (anon / file / shmem): {}",
        status()
    );
    let started = Instant::now();
    let total: usize = db["packages"]
        .as_object()
        .unwrap()
        .values()
        .map(|v| v["notes"].as_str().unwrap().len())
        .sum();
    println!("read back {total} bytes in {:?}", started.elapsed());
    println!(
        "resident after read-back  (anon / file / shmem): {}",
        status()
    );
    let _: &OrdValue = &db;
}
