#!/bin/bash
set -euo pipefail
source_dir=projects/start-os/backup-fs/src
saved=$(mktemp -d)
fixtures=$(mktemp -d)
tar -cf "$saved/current.tar" "$source_dir"
cleanup() {
  tar -xf "$saved/current.tar"
  find "$source_dir" -name '*.rs' -exec touch {} +
  rm -rf "$saved" "$fixtures"
}
trap cleanup EXIT
git archive c7ee6171d7be310a5f030527bfe96c0d8733225a "$source_dir" | tar -xf -
export BACKUP_FS_FIXTURE_DIR="$fixtures"
node <<'JS'
const fs = require('node:fs')
fs.appendFileSync('projects/start-os/backup-fs/src/lib.rs', `
#[cfg(test)]
mod fixture_export {
    use super::*;
    use crate::contents::Contents;

    #[test]
    fn export_format_one_fixture() {
        let out = PathBuf::from(std::env::var("BACKUP_FS_FIXTURE_DIR").unwrap());
        let fs = BackupFS::new(BackupFSOptions { data_dir: out.clone(), password: "password".into(), readonly: false, setuid_support: false, idmapped: false, file_size_padding: None }).unwrap();
        let c = fs.handler.lock().unwrap().ctrl().clone();
        for (id, bytes) in [(2, b"frozen-inline".to_vec()), (3, vec![0x42; 12000]), (4, vec![0x31; 4 * blockstore::CHUNK_SIZE as usize])] {
            let inode = c.next_inode().unwrap();
            assert_eq!(inode.0, id);
            c.save(&InodeAttributes::new(inode, None, FileData::Inline(Vec::new()))).unwrap();
            let mut contents = Contents::open(c.clone(), inode).unwrap();
            contents.write_at(&bytes, 0).unwrap();
            contents.flush().unwrap();
            if id == 3 {
                let mut attrs: InodeAttributes = c.load(inode).unwrap();
                attrs.attrs.uid = 123;
                attrs.attrs.gid = 456;
                attrs.attrs.mode = 0o6751;
                attrs.attrs.xattrs.insert(b"user.fixture".to_vec(), b"original".to_vec());
                c.save(&attrs).unwrap();
            }
        }
        c.syncfs().unwrap();
        drop(c);
        drop(fs);
        let sb = superblock::Superblock::open_or_create(&out.join("superblock"), "password", false).unwrap();
        assert_eq!(superblock::FORMAT_VERSION, 1);
        for name in ["contents", "dirents"] {
            let path = out.join(name).join("abcd/0123456789abcdef0123456789ab");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, vault::seal(name.as_bytes(), &sb.key, sb.constants.ecc())).unwrap();
        }
        let mut log = seglog::SegmentLog::open(out.join("segments"), sb.key).unwrap();
        log.append(&seglog::seal_content(&sb.key, sb.constants.ecc(), 42, b"packed bytes").unwrap()).unwrap();
        log.sync().unwrap();
        log.save_checkpoint().unwrap();
    }
}
`)
JS
cargo test -p startos-backup-fs --lib fixture_export::export_format_one_fixture -- --exact --test-threads=1
tar -czf backup-fs-format1-fixture.tar.gz -C "$fixtures" .
printf 'Pre-PR format1 fixture created; original source restored on exit.\n'
