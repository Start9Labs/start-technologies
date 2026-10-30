#!/bin/bash
set -euo pipefail
source_dir=projects/start-os/backup-fs/src
saved=$(mktemp -d)
fixtures=$(mktemp -d)
tar -cf "$saved/current.tar" "$source_dir"
cleanup() {
  tar -xf "$saved/current.tar"
  rm -rf "$saved" "$fixtures"
}
trap cleanup EXIT
git archive 795d8a265 "$source_dir" | tar -xf -
export BACKUP_FS_FIXTURE_DIR="$fixtures"
node <<'JS'
const fs = require('fs');
const sourceDir = 'projects/start-os/backup-fs/src/';
const superblock = sourceDir + 'superblock.rs';
let source = fs.readFileSync(superblock, 'utf8');
const end = source.lastIndexOf('}');
source = source.slice(0, end) + `
    #[test]
    fn export_frozen_fixture_one() {
        let (dir, _) = legacy_store();
        let out = std::path::PathBuf::from(std::env::var("BACKUP_FS_FIXTURE_DIR").unwrap()).join("v1");
        std::fs::create_dir(&out).unwrap();
        assert!(std::process::Command::new("cp").arg("-a").arg(dir.path().join(".")).arg(out).status().unwrap().success());
    }
` + source.slice(end);
fs.writeFileSync(superblock, source);
const snapshot = sourceDir + 'snapshot_tests.rs';
fs.appendFileSync(snapshot, `
#[test]
fn export_frozen_fixture_two_and_three() {
    let root = PathBuf::from(std::env::var("BACKUP_FS_FIXTURE_DIR").unwrap());
    let data = tempfile::tempdir().unwrap();
    let mut options = config(data.path());
    options.enable_snapshots = false;
    let fs = BackupFS::new(options).unwrap();
    let c = ctrl(&fs);
    let tiny = create_file(&c, b"frozen-inline");
    let packed = create_file(&c, &vec![0x42; 12000]);
    let blocks = create_file(&c, &vec![0x31; 4 * CHUNK_SIZE as usize]);
    assert_eq!((tiny.0, packed.0, blocks.0), (2, 3, 4));
    let mut attrs: InodeAttributes = c.load(packed).unwrap();
    attrs.attrs.uid = 123;
    attrs.attrs.gid = 456;
    attrs.attrs.mode = 0o6751;
    attrs.attrs.xattrs.insert(b"user.fixture".to_vec(), b"original".to_vec());
    c.save(&attrs).unwrap();
    c.syncfs().unwrap();
    c.save_checkpoint();
    drop(c);
    drop(fs);
    let target = root.join("v2");
    std::fs::create_dir(&target).unwrap();
    assert!(std::process::Command::new("cp").arg("-a").arg(data.path().join(".")).arg(target).status().unwrap().success());
    let mut fs = BackupFS::new(config(data.path())).unwrap();
    fs.create_snapshot("before").unwrap();
    let c = ctrl(&fs);
    let mut contents = Contents::open(c.clone(), blocks).unwrap();
    contents.write_at(b"versioned", CHUNK_SIZE + 123).unwrap();
    contents.flush().unwrap();
    fs.create_snapshot("after").unwrap();
    contents.resize(2 * CHUNK_SIZE).unwrap();
    contents.flush().unwrap();
    drop(contents);
    c.syncfs().unwrap();
    c.save_checkpoint();
    drop(c);
    drop(fs);
    let target = root.join("v3");
    std::fs::create_dir(&target).unwrap();
    assert!(std::process::Command::new("cp").arg("-a").arg(data.path().join(".")).arg(target).status().unwrap().success());
}
`);
JS
cargo test -p startos-backup-fs --lib export_frozen_fixture -- --test-threads=1
printf '%s\n' 'Generated from production source at 795d8a265 (formats 1/2/3 before replay-binding changes).' 'v1: password=password; root inode 1, raw packed record 42 and legacy blobs.' 'v2: password=snapshots; inode2=frozen-inline, inode3=12000 bytes0x42, inode4=4 chunks0x31.' 'v3: same inodes/password; snapshots before(4 original chunks), after(versioned at chunk1+123); live truncated to2 chunks.' > "$fixtures/README.txt"
tar -czf backup-fs-legacy-fixtures.tar.gz -C "$fixtures" .
printf 'Frozen fixture archive created; original source restored on exit.\n'
