#!/usr/bin/env bash

set -x

export RUST_BACKTRACE=1

TEST_FAILED=0

# =================
# run our own tests
# =================

cd /code/backupfs
RUSTFLAGS=-g cargo test --release
TEST_FAILED=$(( $? | $TEST_FAILED ))

# ============================
# run the libfuse example suit
# ============================

TEST_DATA_DIR=$(mktemp --directory)
TEST_DIR=$(mktemp --directory)

cd /code/libfuse/build/test
printf %s ohea | RUST_LOG=debug /code/backupfs/target/release/startos-backup-fs mount "$TEST_DATA_DIR" "$TEST_DIR"
./test_syscalls "$TEST_DIR"
TEST_FAILED=$(( $? | $TEST_FAILED ))
umount "$TEST_DIR"

# ====================
# run the xfstest suit
# ====================

if [ ]; then

TEST_DATA_DIR=$(mktemp --directory)
SCRATCH_DATA_DIR=$(mktemp --directory)
TEST_DIR=$(mktemp --directory)
SCRATCH_DIR=$(mktemp --directory)

cd /code/fuse-xfstests

# requires OFD & POSIX locks. OFD locks are not supported by fuse
echo "generic/478" >> xfs_excludes.txt

# TODO: requires supporting orphaned files, that have an open file handle, but no links
echo "generic/484" >> xfs_excludes.txt

# Writes directly to scratch block dev
echo "generic/062" >> xfs_excludes.txt

# TODO: looks like it requires character file support
echo "generic/078" >> xfs_excludes.txt

# TODO: takes > 10min
echo "generic/069" >> xfs_excludes.txt

# TODO: needs fallocate which is missing from Linux FUSE driver (https://github.com/libfuse/libfuse/issues/395)
echo "generic/263" >> xfs_excludes.txt

# TODO: Passes, but takes ~30min
echo "generic/127" >> xfs_excludes.txt

# TODO: requires more complete falloc support. Also fills up the entire hard disk...
echo "generic/103" >> xfs_excludes.txt

# TODO: requires support for mknod on character files
echo "generic/184" >> xfs_excludes.txt
echo "generic/401" >> xfs_excludes.txt

# TODO: requires fifo support
echo "generic/423" >> xfs_excludes.txt
echo "generic/434" >> xfs_excludes.txt

# TODO: requires ulimit support for limiting file size
echo "generic/394" >> xfs_excludes.txt

# requires BSD lock support, and checks /proc/locks. fuse locks don't seem to show up in /proc/locks
echo "generic/504" >> xfs_excludes.txt

# TODO: requires support for system.posix_acl_access xattr sync'ing to file permissions
# Some information about it linked from here: https://stackoverflow.com/questions/29569408/documentation-of-posix-acl-access-and-friends
echo "generic/099" >> xfs_excludes.txt
echo "generic/105" >> xfs_excludes.txt
echo "generic/375" >> xfs_excludes.txt

# TODO: requires support for mounting read-only
echo "generic/294" >> xfs_excludes.txt
echo "generic/306" >> xfs_excludes.txt
echo "generic/452" >> xfs_excludes.txt

# TODO: requires atime support
echo "generic/003" >> xfs_excludes.txt
echo "generic/192" >> xfs_excludes.txt

# TODO: Passes, but takes ~10min and writes > 20GB. Needs support for writing files with large holes,
# for this test to be fast
echo "generic/130" >> xfs_excludes.txt

# TODO: uses namespaces and inodes don't seem to get mapped properly
# this test ends up trying to chmod "/" (the root inode)
echo "generic/317" >> xfs_excludes.txt

# TODO: requires more complete ACL support
echo "generic/319" >> xfs_excludes.txt
echo "generic/444" >> xfs_excludes.txt

# TODO: Seems to cause a host OOM (even from inside Docker), when run with 84, 87, 88, 100, and 109
echo "generic/089" >> xfs_excludes.txt

# TODO: very slow. Passes, but takes > 30min
echo "generic/074" >> xfs_excludes.txt

# TODO: very slow. Ran for > 3hrs without completing
echo "generic/339" >> xfs_excludes.txt

# TODO: Passes, but takes ~60min on CI
echo "generic/006" >> xfs_excludes.txt
echo "generic/011" >> xfs_excludes.txt
echo "generic/070" >> xfs_excludes.txt

# TODO: very slow. Passes, but takes 20min
echo "generic/438" >> xfs_excludes.txt

# TODO: seems to crash host
echo "generic/476" >> xfs_excludes.txt

# TODO: writing to /proc/sys/vm/drop_caches is not allowed inside Docker
echo "generic/086" >> xfs_excludes.txt
echo "generic/391" >> xfs_excludes.txt
echo "generic/426" >> xfs_excludes.txt
echo "generic/467" >> xfs_excludes.txt
echo "generic/477" >> xfs_excludes.txt

# TODO: permission failure invoking FIBMAP
echo "generic/519" >> xfs_excludes.txt

# TODO: Tries to create 50k+ files, which OOMs
echo "generic/531" >> xfs_excludes.txt

# Test requires mounting a loopback device
echo "generic/564" >> xfs_excludes.txt

PASSWORD="ohea" TEST_DEV="$TEST_DATA_DIR" TEST_DIR="$TEST_DIR" SCRATCH_DEV="$SCRATCH_DATA_DIR" SCRATCH_MNT="$SCRATCH_DIR" ./check-fuser -E xfs_excludes.txt
TEST_FAILED=$(( $? | $TEST_FAILED ))
fi

# ====
# done
# ====
exit $TEST_FAILED
