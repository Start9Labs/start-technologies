use std::mem::ManuallyDrop;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use futures::StreamExt;
use inotify::{Inotify, WatchMask};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

use super::*;

#[path = "configuration_reference_tests.rs"]
mod configuration_references;
#[path = "capture_lifecycle_tests.rs"]
mod lifecycle;
#[path = "profile_transport_tests.rs"]
mod profiles;
#[cfg(target_os = "linux")]
#[path = "published_reference_tests.rs"]
mod published_references;
#[path = "root_view_tests.rs"]
mod root_views;
#[path = "selected_reference_tests.rs"]
mod selected_references;
use crate::disk::mount::filesystem::loop_dev::LoopDev;
use crate::disk::mount::guard::GenericMountGuard;
use crate::os_overlay::OVERLAY_FILE;
use crate::util::io::open_file;
use crate::util::squashfs::{DirectoryContents, Entry, Metadata, NodeContents};

async fn transport(profiles: bool) -> tokio::fs::File {
    let file = anonymous_file().unwrap();
    let mut archive = tar::Builder::new(file.into_std().await);
    let data = b"private configuration bytes";
    let mut header = tar::Header::new_gnu();
    header.set_mode(0o600);
    header.set_size(data.len() as u64);
    header.set_cksum();
    archive
        .append_data(&mut header, CAPTURE_FILE, &data[..])
        .unwrap();
    append_json(
        &mut archive,
        KEYS_FILE,
        &vec![OsString::from_vec(b"/key-\xff".to_vec())],
    )
    .unwrap();
    if profiles {
        append_profiles(
            &mut archive,
            &std::collections::BTreeMap::from([(
                "private.nmconnection".to_owned(),
                crate::net::wifi::NetworkProfile {
                    contents: Zeroizing::new("private-profile-secret".to_owned()),
                    modified: (123, 456_789),
                },
            )]),
        )
        .unwrap();
    }
    tokio::fs::File::from_std(archive.into_inner().unwrap())
}

#[tokio::test]
async fn capture_transport_keeps_private_profiles_and_native_paths_in_owned_memory() {
    let (mut image, keys, profiles) = receive(transport(true).await).await.unwrap();
    let mut contents = Zeroizing::new(Vec::new());
    image.read_to_end(&mut contents).await.unwrap();
    assert_eq!(&*contents, b"private configuration bytes");
    assert_eq!(
        keys,
        vec![PathBuf::from(OsString::from_vec(b"/key-\xff".to_vec()))]
    );
    assert_eq!(
        profiles
            .get("private.nmconnection")
            .unwrap()
            .contents
            .as_str(),
        "private-profile-secret"
    );
    assert_eq!(profiles.len(), 1);
}

#[tokio::test]
async fn capture_transport_requires_profiles_and_rejects_bytes_after_the_first_archive() {
    assert!(receive(transport(false).await).await.is_err());
    let mut archive = transport(true).await;
    archive.seek(std::io::SeekFrom::End(0)).await.unwrap();
    archive
        .write_all(b"unexpected trailing output")
        .await
        .unwrap();
    assert!(receive(archive).await.is_err());
}

struct Fixture {
    volume: ManuallyDrop<MountGuard>,
    base: ManuallyDrop<MountGuard>,
    ram: ManuallyDrop<MountGuard>,
    working: ManuallyDrop<MountGuard>,
}

fn source_image() -> PathBuf {
    PathBuf::from(std::env::var_os("FDE_SOURCE_IMAGE").expect("genuine public fixture image"))
}

impl Fixture {
    async fn new() -> Self {
        let root =
            PathBuf::from(std::env::var_os("FDE_CAPTURE_TEST_ROOT").expect("owned fixture root"));
        let image = source_image();
        let backing = crate::util::io::create_file_mod(root.join("data.btrfs"), 0o600)
            .await
            .unwrap();
        backing
            .set_len(tokio::fs::metadata(&image).await.unwrap().len() + 512 * 1024 * 1024)
            .await
            .unwrap();
        backing.sync_all().await.unwrap();
        drop(backing);
        Command::new("mkfs.btrfs")
            .args(["-q", "-f"])
            .arg(root.join("data.btrfs"))
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();
        let volume = MountGuard::mount(
            &LoopDev::new(root.join("data.btrfs"), 0, 0),
            root.join("volume"),
            ReadWrite,
        )
        .await
        .unwrap();
        let base = MountGuard::mount(&LoopDev::new(image, 0, 0), root.join("base"), ReadOnly)
            .await
            .unwrap();
        let ram = MountGuard::mount(&PrivateTmpfs::new(), root.join("ram"), ReadWrite)
            .await
            .unwrap();
        let raw = volume.path().join("raw");
        for relative in [
            "raw/etc/ssh",
            "raw/custom",
            "data/main",
            "data/package-data",
            "data/main/network-profiles",
            "generations",
            "images",
            "config",
        ] {
            tokio::fs::create_dir_all(volume.path().join(relative))
                .await
                .unwrap();
        }
        tokio::fs::write(raw.join("etc/shadow"), b"captured-private-shadow")
            .await
            .unwrap();
        tokio::fs::write(raw.join("etc/passwd"), b"root:x:0:0:root:/root:/bin/bash\n")
            .await
            .unwrap();
        tokio::fs::write(
            raw.join("etc/nsswitch.conf"),
            b"passwd: files\ngroup: files\nhosts: files dns\n",
        )
        .await
        .unwrap();
        tokio::fs::write(
            raw.join("etc/ssh/sshd_config"),
            b"Include /media/startos/data/main/capture-gate\n",
        )
        .await
        .unwrap();
        tokio::fs::write(
            raw.join("custom")
                .join(OsString::from_vec(b"key\xff".to_vec())),
            b"captured-native-byte-key",
        )
        .await
        .unwrap();
        tokio::fs::write(
            volume
                .path()
                .join("data/main/network-profiles/private.nmconnection"),
            b"[connection]\nid=captured\n[wifi-security]\npsk=captured-bootstrap-password\n",
        )
        .await
        .unwrap();
        tokio::fs::set_permissions(
            volume
                .path()
                .join("data/main/network-profiles/private.nmconnection"),
            std::fs::Permissions::from_mode(0o600),
        )
        .await
        .unwrap();
        let fifo = volume.path().join("data/main/capture-gate");
        nix::unistd::mkfifo(
            &fifo,
            nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
        )
        .unwrap();
        let selected = volume.path().join("selected.private");
        let empty = Squashfs::<crate::s9pk::merkle_archive::source::DynFileSource>::new(
            Metadata::new(0o755),
            DirectoryContents::new(),
        );
        let mut file = crate::util::io::create_file_mod(&selected, 0o600)
            .await
            .unwrap();
        empty.serialize(&mut file).await.unwrap();
        file.sync_all().await.unwrap();
        drop(file);
        tokio::fs::symlink(&selected, volume.path().join("config/current.rootfs"))
            .await
            .unwrap();
        let working = MountGuard::mount(
            &crate::os_overlay::working_filesystem(
                &raw,
                &ram.path().join("upper"),
                &ram.path().join("work"),
            ),
            root.join("working"),
            ReadWrite,
        )
        .await
        .unwrap();
        Self {
            volume: ManuallyDrop::new(volume),
            base: ManuallyDrop::new(base),
            ram: ManuallyDrop::new(ram),
            working: ManuallyDrop::new(working),
        }
    }

    async fn epoch(&self) -> ReadEpoch {
        let mut epoch = ReadEpoch::new(
            ConfigurationSource::Working {
                lower: self.volume.path().join("raw"),
                upper: self.ram.path().join("upper"),
            },
            self.base.path().to_owned(),
            self.volume.path().join("data"),
            self.volume.path().join("data/main/network-profiles"),
            self.volume.path().join("generations"),
            self.volume.path().join("images"),
            &[
                self.volume.path().to_owned(),
                self.volume.path().join("data/main"),
            ],
            &[self.working.path().to_owned(), self.ram.path().to_owned()],
        )
        .await
        .unwrap();
        epoch.selected_image = Some(
            crate::os_overlay::image_store::selected_image(&self.volume.path().join("config"))
                .await
                .unwrap(),
        );
        epoch
    }

    async fn gate(&self) -> (tokio::fs::File, inotify::EventStream<[u8; 4096]>) {
        let fifo = self.volume.path().join("data/main/capture-gate");
        let gate = tokio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&fifo)
            .await
            .unwrap();
        let inotify = Inotify::init().unwrap();
        inotify.watches().add(&fifo, WatchMask::OPEN).unwrap();
        (gate, inotify.into_event_stream([0; 4096]).unwrap())
    }

    async fn cleanup(self) {
        ManuallyDrop::into_inner(self.working)
            .unmount(true)
            .await
            .unwrap();
        ManuallyDrop::into_inner(self.ram)
            .unmount(true)
            .await
            .unwrap();
        ManuallyDrop::into_inner(self.base)
            .unmount(true)
            .await
            .unwrap();
        ManuallyDrop::into_inner(self.volume)
            .unmount(true)
            .await
            .unwrap();
    }
}

fn executable() -> PathBuf {
    PathBuf::from(std::env::var_os("FDE_CAPTURE_EXECUTABLE").expect("genuine built startbox"))
}

fn unit() -> String {
    std::env::var("FDE_CAPTURE_UNIT").expect("owned native operation unit")
}

fn lease_path() -> PathBuf {
    PathBuf::from(std::env::var_os("FDE_CAPTURE_TEST_ROOT").unwrap())
        .join("volume")
        .join(
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../projects/start-os/build/lib/scripts/os-image-lock-name"
            ))
            .trim(),
        )
}

async fn image_lease() -> crate::util::FileLock {
    crate::util::FileLock::new(lease_path(), false)
        .await
        .unwrap()
}

async fn gate_opened<T>(
    events: &mut (impl futures::Stream<Item = Result<inotify::EventOwned, std::io::Error>> + Unpin),
    operation: &mut std::pin::Pin<Box<impl std::future::Future<Output = T>>>,
) {
    tokio::time::timeout(Duration::from_secs(45), async {
        tokio::select! {
            biased;
            _ = async {
                while let Some(event) = events.next().await {
                    if event.unwrap().mask.contains(inotify::EventMask::OPEN) { return; }
                }
                panic!("native FIFO event stream ended");
            } => (),
            _ = &mut *operation => panic!("capture ended before its actual prospective read"),
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires exclusive managed guest, real PID1/Btrfs/loop/overlay privileges, genuine FDE_SOURCE_IMAGE and built FDE_CAPTURE_EXECUTABLE; never on build host"]
async fn native_capture_freezes_sources_keeps_db_reads_live_and_projects_after_thaw() {
    let fixture = Fixture::new().await;
    let current_id = Guid::new();
    let relative = PathBuf::from(current_id.to_string()).join("etc/current-key");
    let backing = fixture
        .volume
        .path()
        .join("data/main")
        .join(super::super::private_config::DIRECTORY)
        .join(&relative);
    tokio::fs::create_dir_all(backing.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(backing, b"live current credential fixture")
        .await
        .unwrap();
    tokio::fs::symlink(
        Path::new(super::super::private_config::RUNTIME).join(relative),
        PathBuf::from(std::env::var_os("FDE_CAPTURE_TEST_ROOT").unwrap()).join("running-reference"),
    )
    .await
    .unwrap();
    let published_id = Guid::new();
    let stored_id = Guid::new();
    let selected_id = Guid::new();
    let pinned_id = Guid::new();
    let generation = fixture
        .volume
        .path()
        .join("generations")
        .join(format!("generation-{}", Guid::new()));
    tokio::fs::create_dir(&generation).await.unwrap();
    for (id, image) in [
        (&published_id, generation.join(OVERLAY_FILE)),
        (&pinned_id, generation.join(crate::os_overlay::ROOTFS_FILE)),
        (
            &stored_id,
            fixture.volume.path().join("images/stored.rootfs"),
        ),
        (&selected_id, fixture.volume.path().join("selected.private")),
    ] {
        let relative = PathBuf::from(id.to_string()).join("etc/image-key");
        let backing = fixture
            .volume
            .path()
            .join("data/main")
            .join(super::super::private_config::DIRECTORY)
            .join(&relative);
        tokio::fs::create_dir_all(backing.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(backing, b"image credential fixture")
            .await
            .unwrap();
        let mut snapshot = Squashfs::<crate::s9pk::merkle_archive::source::DynFileSource>::new(
            Metadata::new(0o755),
            DirectoryContents::new(),
        );
        snapshot
            .contents_mut()
            .insert_path(
                "image-reference",
                Entry::new(
                    Metadata::new(0o777),
                    NodeContents::Symlink(
                        Path::new(super::super::private_config::RUNTIME).join(relative),
                    ),
                ),
            )
            .unwrap();
        let mut file = crate::util::io::create_file_mod(image, 0o600)
            .await
            .unwrap();
        snapshot.serialize(&mut file).await.unwrap();
        file.sync_all().await.unwrap();
    }
    let data_alias = MountGuard::mount(
        &IsolatedBind::new(fixture.volume.path().join("data/main")),
        fixture.volume.path().join("alias"),
        ReadOnly,
    )
    .await
    .unwrap()
    .with_strict_unmount();
    let expected = source_image();
    let backing = loop_backing_image(fixture.base.path(), expected.parent().unwrap())
        .await
        .unwrap();
    assert!(
        crate::util::io::same_file(&backing, &expected)
            .await
            .unwrap()
    );
    let epoch = fixture.epoch().await;
    assert_eq!(epoch.frozen_filesystems.len(), 1);
    assert_eq!(
        epoch.readonly_filesystems,
        [fixture.working.path(), fixture.ram.path()]
    );
    let (mut gate, mut events) = fixture.gate().await;
    let db = patch_db::PatchDb::open(fixture.volume.path().join("data/main/capture.db"))
        .await
        .unwrap();
    db.put(&patch_db::json_ptr::ROOT, &1).await.unwrap();
    let pause = db.pause_mutations().await;
    let mut write = Box::pin(db.put(&patch_db::json_ptr::ROOT, &2));
    assert!(futures::poll!(&mut write).is_pending());
    let executable = executable();
    let unit = unit();
    let images = image_lease().await;
    let mut capture = Box::pin(capture_in_unit(&epoch, &executable, &unit, &images));
    gate_opened(&mut events, &mut capture).await;
    assert_eq!(
        db.get_value(&patch_db::json_ptr::ROOT).await,
        patch_db::value::json!(1)
    );
    assert!(
        tokio::fs::write(
            fixture.ram.path().join("upper/direct-raw-writer"),
            b"blocked"
        )
        .await
        .is_err()
    );
    let mut existing = tokio::fs::read_dir(
        data_alias
            .path()
            .join(super::super::private_config::DIRECTORY),
    )
    .await
    .unwrap();
    let mut names = std::collections::BTreeSet::new();
    while let Some(entry) = existing.next_entry().await.unwrap() {
        names.insert(entry.file_name());
    }
    assert_eq!(
        names,
        std::collections::BTreeSet::from([
            std::ffi::OsString::from(current_id.to_string()),
            std::ffi::OsString::from(published_id.to_string()),
            std::ffi::OsString::from(stored_id.to_string()),
            std::ffi::OsString::from(selected_id.to_string()),
            std::ffi::OsString::from(pinned_id.to_string())
        ])
    );
    drop(existing);
    assert!(
        tokio::fs::write(fixture.working.path().join("blocked"), b"blocked")
            .await
            .is_err()
    );
    let mut writer = Command::new("/bin/sh")
        .arg("-c")
        .arg("printf 'started\\n'; printf late-change > \"$1\"; printf post-epoch-profile > \"$2\"")
        .arg("native-source-writer")
        .arg(fixture.volume.path().join("raw/etc/shadow"))
        .arg(
            fixture
                .volume
                .path()
                .join("data/main/network-profiles/private.nmconnection"),
        )
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdout = tokio::io::BufReader::new(writer.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).await.unwrap();
    assert_eq!(line, "started\n");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), writer.wait())
            .await
            .is_err()
    );
    gate.write_all(b"HostKey \"/custom/key\xff\"\n")
        .await
        .unwrap();
    drop(gate);
    let (image, keys, profiles) = capture.await.unwrap();
    assert_eq!(
        profiles
            .get("private.nmconnection")
            .unwrap()
            .contents
            .as_str(),
        "[connection]\nid=captured\n[wifi-security]\npsk=captured-bootstrap-password\n"
    );
    assert!(writer.wait().await.unwrap().success());
    assert_eq!(
        tokio::fs::read(
            fixture
                .volume
                .path()
                .join("data/main/network-profiles/private.nmconnection")
        )
        .await
        .unwrap(),
        b"post-epoch-profile"
    );
    drop(pause);
    write.await.unwrap();
    assert_eq!(
        db.get_value(&patch_db::json_ptr::ROOT).await,
        patch_db::value::json!(2)
    );
    assert!(keys.contains(&PathBuf::from(OsString::from_vec(
        b"/custom/key\xff".to_vec()
    ))));
    tokio::fs::write(fixture.working.path().join("restored"), b"writable")
        .await
        .unwrap();
    let snapshot = Squashfs::deserialize(MultiCursorFile::from(image))
        .await
        .unwrap()
        .into_dyn();
    let projected =
        ProjectedConfig::stage(snapshot, &fixture.volume.path().join("data/main"), &keys)
            .await
            .unwrap();
    let source = PathBuf::from(std::env::var_os("FDE_SOURCE_IMAGE").unwrap());
    let signing = crate::os_overlay::generate_signing_key().unwrap();
    let prepared = projected
        .prepare_generation(
            &source,
            &fixture.volume.path().join("generations"),
            &signing,
        )
        .await
        .unwrap();
    let overlay = Squashfs::deserialize(
        MultiCursorFile::open(
            &open_file(prepared.staging_path().join(OVERLAY_FILE))
                .await
                .unwrap(),
        )
        .await
        .unwrap(),
    )
    .await
    .unwrap();
    let shadow = overlay
        .contents()
        .get_path(Path::new("etc/shadow"))
        .unwrap()
        .as_node()
        .unwrap();
    let NodeContents::Symlink(target) = &shadow.contents else {
        panic!("private shadow was not projected")
    };
    let backing = fixture
        .volume
        .path()
        .join("data/main/fde-private-config")
        .join(
            target
                .strip_prefix(super::super::private_config::RUNTIME)
                .unwrap(),
        );
    assert_eq!(
        tokio::fs::read(&backing).await.unwrap(),
        b"captured-private-shadow"
    );
    assert_eq!(
        tokio::fs::read(fixture.volume.path().join("raw/etc/shadow"))
            .await
            .unwrap(),
        b"late-change"
    );
    db.close().await;
    drop(overlay);
    projected.discard_generation(prepared).await.unwrap();
    assert!(!backing.exists());
    data_alias.unmount(false).await.unwrap();
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "requires exclusive managed guest, real PID1/Btrfs/loop/overlay privileges, genuine FDE_SOURCE_IMAGE and built FDE_CAPTURE_EXECUTABLE; never on build host"]
async fn native_profile_capture_failure_thaws_before_returning_without_projection() {
    let fixture = Fixture::new().await;
    let mut epoch = fixture.epoch().await;
    epoch.network_profiles = fixture.volume.path().join("raw/etc/passwd");
    let (mut gate, mut events) = fixture.gate().await;
    let executable = executable();
    let unit = unit();
    let images = image_lease().await;
    let mut capture = Box::pin(capture_in_unit(&epoch, &executable, &unit, &images));
    gate_opened(&mut events, &mut capture).await;
    gate.write_all(b"HostKey \"/custom/key\xff\"\n")
        .await
        .unwrap();
    drop(gate);
    assert!(
        tokio::time::timeout(Duration::from_secs(30), capture)
            .await
            .unwrap()
            .is_err()
    );
    tokio::fs::write(fixture.working.path().join("restored"), b"writable")
        .await
        .unwrap();
    tokio::fs::write(fixture.volume.path().join("raw/restored"), b"writable")
        .await
        .unwrap();
    assert!(
        !fixture
            .volume
            .path()
            .join("data/main/fde-private-config")
            .exists()
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "requires exclusive managed guest, real PID1/Btrfs/loop/overlay privileges, genuine FDE_SOURCE_IMAGE and built FDE_CAPTURE_EXECUTABLE; never on build host"]
async fn native_capture_helper_death_restores_owned_sources_and_preserves_readonly_intent() {
    let fixture = Fixture::new().await;
    remount(fixture.working.path(), true).await.unwrap();
    remount(fixture.ram.path(), true).await.unwrap();
    let epoch = fixture.epoch().await;
    assert!(epoch.readonly_filesystems.is_empty());
    let (gate, mut events) = fixture.gate().await;
    let executable = executable();
    let unit = unit();
    let images = image_lease().await;
    let mut capture = Box::pin(capture_in_unit(&epoch, &executable, &unit, &images));
    gate_opened(&mut events, &mut capture).await;
    let pid = Command::new("systemctl")
        .args(["show", "--value", "--property=MainPID", &unit])
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    let pid: i32 = std::str::from_utf8(&pid).unwrap().trim().parse().unwrap();
    assert!(pid > 1);
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid),
        nix::sys::signal::Signal::SIGKILL,
    )
    .unwrap();
    assert!(capture.await.is_err());
    drop(gate);
    tokio::fs::write(fixture.volume.path().join("restored"), b"writable")
        .await
        .unwrap();
    assert!(
        tokio::fs::write(fixture.working.path().join("still-readonly"), b"blocked")
            .await
            .is_err()
    );
    assert!(
        tokio::fs::write(fixture.ram.path().join("upper/still-readonly"), b"blocked")
            .await
            .is_err()
    );
    assert!(
        !fixture
            .volume
            .path()
            .join("data/main/fde-private-config")
            .exists()
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "requires exclusive managed guest, real PID1/cgroup-v2/Btrfs/loop/overlay privileges and genuine source/executables; never on build host"]
async fn native_capture_stop_thaws_before_waiting_for_a_blocked_owned_writer() {
    let fixture = Fixture::new().await;
    let epoch = fixture.epoch().await;
    let (gate, mut events) = fixture.gate().await;
    let executable = executable();
    let unit = unit();
    let images = image_lease().await;
    let mut capture = Box::pin(capture_in_unit(&epoch, &executable, &unit, &images));
    gate_opened(&mut events, &mut capture).await;
    let mut writer = Command::new("/bin/sh")
        .arg("-c")
        .arg("printf 'started\\n'; printf late > \"$1\"")
        .arg("owned-blocked-writer")
        .arg(fixture.volume.path().join("blocked-writer"))
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdout = tokio::io::BufReader::new(writer.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).await.unwrap();
    assert_eq!(line, "started\n");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), writer.wait())
            .await
            .is_err()
    );
    let control_group = Command::new("systemctl")
        .args(["show", "--value", "--property=ControlGroup", &unit])
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    let control_group = std::str::from_utf8(&control_group).unwrap().trim();
    assert!(control_group.contains(&unit));
    let members = Path::new("/sys/fs/cgroup")
        .join(control_group.strip_prefix('/').unwrap())
        .join("cgroup.procs");
    tokio::fs::write(&members, writer.id().unwrap().to_string())
        .await
        .unwrap();
    Command::new("systemctl")
        .arg("stop")
        .arg(&unit)
        .timeout(Some(Duration::from_secs(15)))
        .invoke(ErrorKind::Filesystem)
        .await
        .unwrap();
    assert!(capture.await.is_err());
    tokio::time::timeout(Duration::from_secs(5), writer.wait())
        .await
        .unwrap()
        .unwrap();
    drop(gate);
    tokio::fs::write(fixture.volume.path().join("after-stop"), b"writable")
        .await
        .unwrap();
    tokio::fs::write(fixture.working.path().join("after-stop"), b"writable")
        .await
        .unwrap();
    assert!(
        !fixture
            .volume
            .path()
            .join("data/main/fde-private-config")
            .exists()
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "native requester subprocess fixture; invoked only by the owning managed-guest test"]
async fn native_capture_requester_subprocess_fixture() {
    let path =
        PathBuf::from(std::env::var_os("FDE_CAPTURE_REQUESTER_REQUEST").expect("owned request"));
    let epoch = request(&path).await.unwrap();
    let images = image_lease().await;
    capture_in_unit(&epoch, &executable(), &unit(), &images)
        .await
        .unwrap();
    panic!("requester must be killed during the native query");
}

#[tokio::test]
#[ignore = "requires exclusive managed guest, real PID1/Btrfs/loop/overlay privileges and genuine source/executables; never on build host"]
async fn native_capture_requester_death_leaves_cleanup_owned_by_pid1() {
    let fixture = Fixture::new().await;
    let epoch = fixture.epoch().await;
    let request =
        PathBuf::from(std::env::var_os("FDE_CAPTURE_TEST_ROOT").unwrap()).join("requester.json");
    write_file_private_atomic(&request, serde_json::to_vec(&epoch).unwrap())
        .await
        .unwrap();
    let root = PathBuf::from(std::env::var_os("FDE_CAPTURE_TEST_ROOT").unwrap());
    assert!(
        root.as_os_str()
            .as_bytes()
            .iter()
            .all(|byte| { byte.is_ascii_alphanumeric() || b"/._-".contains(byte) })
    );
    let release = root.join("release-gate");
    nix::unistd::mkfifo(
        &release,
        nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
    )
    .unwrap();
    let mut release_gate = tokio::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&release)
        .await
        .unwrap();
    let release_events = Inotify::init().unwrap();
    release_events
        .watches()
        .add(&release, WatchMask::OPEN)
        .unwrap();
    let mut release_events = release_events.into_event_stream([0; 4096]).unwrap();
    let wrapper = root.join("capture-wrapper");
    tokio::fs::write(&wrapper, format!(
        "#!/bin/sh\nif [ \"$1\" = start-fde-release ]; then\n IFS= read -r _ < {}\nfi\nexec {} \"$@\"\n",
        release.display(), executable().display(),
    )).await.unwrap();
    tokio::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700))
        .await
        .unwrap();
    let (gate, mut events) = fixture.gate().await;
    let mut requester = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "fde::capture::tests::native_capture_requester_subprocess_fixture",
            "--nocapture",
        ])
        .env("FDE_CAPTURE_REQUESTER_REQUEST", &request)
        .env("FDE_CAPTURE_EXECUTABLE", &wrapper)
        .process_group(0)
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut completion = Box::pin(requester.wait());
    gate_opened(&mut events, &mut completion).await;
    drop(completion);
    nix::sys::signal::killpg(
        nix::unistd::Pid::from_raw(requester.id().unwrap().try_into().unwrap()),
        nix::sys::signal::Signal::SIGKILL,
    )
    .unwrap();
    assert!(!requester.wait().await.unwrap().success());
    let rejected_while_frozen = matches!(
        tokio::time::timeout(
            Duration::from_millis(500),
            crate::util::FileLock::new(lease_path(), false),
        )
        .await,
        Ok(Err(_))
    );
    let adapter = root.join("flock");
    let flock_entered = root.join("kernel-flock-entered");
    tokio::fs::write(
        &adapter,
        format!(
            "#!/bin/sh\nprintf reached > {}\nexec /usr/bin/flock --nonblock \"$@\"\n",
            flock_entered.display()
        ),
    )
    .await
    .unwrap();
    tokio::fs::set_permissions(&adapter, std::fs::Permissions::from_mode(0o700))
        .await
        .unwrap();
    let scripts =
        PathBuf::from(std::env::var_os("FDE_IMAGE_SCRIPTS").expect("current native image scripts"));
    let mut shell = Command::new("/bin/bash");
    shell
        .args([
            "-euc",
            "source \"$1/os-image-store\"; startos_reserve_image_store \"$2\"",
            "frozen-shell-lease",
        ])
        .arg(scripts)
        .arg(fixture.volume.path())
        .env(
            "PATH",
            format!("{}:/usr/sbin:/usr/bin:/sbin:/bin", root.display()),
        )
        .env_remove(
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../projects/start-os/build/lib/scripts/os-image-lock-fd-name"
            ))
            .trim(),
        );
    let shell_rejected_while_frozen = matches!(
        tokio::time::timeout(
            Duration::from_millis(500),
            shell.invoke(ErrorKind::Filesystem)
        )
        .await,
        Ok(Err(_))
    );
    let shell_reached_kernel = flock_entered.exists();
    assert!(
        tokio::fs::write(fixture.working.path().join("before-stop"), b"blocked")
            .await
            .is_err()
    );
    let mut stop = Box::pin(async {
        Command::new("systemctl")
            .arg("stop")
            .arg(unit())
            .timeout(Some(Duration::from_secs(15)))
            .invoke(ErrorKind::Filesystem)
            .await
    });
    gate_opened(&mut release_events, &mut stop).await;
    assert!(
        crate::util::FileLock::new(lease_path(), false)
            .await
            .is_err()
    );
    release_gate.write_all(b"restore\n").await.unwrap();
    stop.await.unwrap();
    assert!(
        crate::util::FileLock::new(lease_path(), false)
            .await
            .is_ok()
    );
    assert!(
        rejected_while_frozen,
        "busy acquisition waited for filesystem thaw"
    );
    assert!(
        shell_rejected_while_frozen && shell_reached_kernel,
        "native shell acquisition waited for filesystem thaw"
    );
    drop(gate);
    tokio::fs::write(
        fixture.volume.path().join("after-requester-death"),
        b"writable",
    )
    .await
    .unwrap();
    tokio::fs::write(
        fixture.working.path().join("after-requester-death"),
        b"writable",
    )
    .await
    .unwrap();
    assert!(
        !fixture
            .volume
            .path()
            .join("data/main/fde-private-config")
            .exists()
    );
    fixture.cleanup().await;
}
