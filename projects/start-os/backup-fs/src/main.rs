use std::ffi::OsString;
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use backup_fs_protocol::{CONVERT_COMMAND, INSPECT_COMMAND, MOUNT_COMMAND};
use backupfs::error::BkfsErrorKind;
use backupfs::{BackupFS, BackupFSOptions, ConversionPhase};
use clap::{CommandFactory, FromArgMatches, Parser};
use fuser::{Config, MountOption, SessionACL};
use log::{error, info};
use zeroize::Zeroizing;

#[derive(clap::Parser)]
struct FilesystemOptions {
    #[command(flatten)]
    backup_opts: BackupFSOptions,
    #[arg(long, help = "Read the password from a file instead of standard input")]
    password_file: Option<PathBuf>,
}

#[derive(clap::Parser)]
struct InspectOptions {
    data_dir: PathBuf,
}

#[derive(clap::Parser)]
struct MountOptions {
    #[command(flatten)]
    filesystem: FilesystemOptions,
    #[arg(short = 'o', value_delimiter = ',')]
    opt: Vec<String>,
    #[arg(long, help = "Mount a named snapshot read-only")]
    snapshot: Option<String>,
    mountpoint: PathBuf,
}

#[derive(clap::Parser)]
struct BasicMountOptions {
    #[arg(short = 'o', value_delimiter = ',')]
    opt: Vec<String>,
    data_dir: PathBuf,
    mountpoint: PathBuf,
}

#[derive(clap::Parser)]
struct ChangePasswordOptions {
    #[command(flatten)]
    filesystem: FilesystemOptions,
    #[arg(long, help = "Read the new password from a file")]
    new_password_file: PathBuf,
}

#[derive(clap::Parser)]
struct NamedSnapshotOptions {
    #[command(flatten)]
    filesystem: FilesystemOptions,
    name: String,
}

#[derive(clap::Parser)]
struct SnapshotOptions {
    #[command(subcommand)]
    command: SnapshotCommand,
}

#[derive(clap::Subcommand)]
enum SnapshotCommand {
    Create(NamedSnapshotOptions),
    List(FilesystemOptions),
    Delete(NamedSnapshotOptions),
}

enum ParsedOption {
    Mount(MountOption),
    AllowOther,
    AllowRoot,
}

fn parse_option(s: &str) -> ParsedOption {
    match s {
        "allow_other" => ParsedOption::AllowOther,
        "allow_root" => ParsedOption::AllowRoot,
        "auto_unmount" => ParsedOption::Mount(MountOption::AutoUnmount),
        "default_permissions" => ParsedOption::Mount(MountOption::DefaultPermissions),
        "dev" => ParsedOption::Mount(MountOption::Dev),
        "nodev" => ParsedOption::Mount(MountOption::NoDev),
        "suid" => ParsedOption::Mount(MountOption::Suid),
        "nosuid" => ParsedOption::Mount(MountOption::NoSuid),
        "ro" => ParsedOption::Mount(MountOption::RO),
        "rw" => ParsedOption::Mount(MountOption::RW),
        "exec" => ParsedOption::Mount(MountOption::Exec),
        "noexec" => ParsedOption::Mount(MountOption::NoExec),
        "atime" => ParsedOption::Mount(MountOption::Atime),
        "noatime" => ParsedOption::Mount(MountOption::NoAtime),
        "dirsync" => ParsedOption::Mount(MountOption::DirSync),
        "sync" => ParsedOption::Mount(MountOption::Sync),
        "async" => ParsedOption::Mount(MountOption::Async),
        x if x.starts_with("fsname=") => ParsedOption::Mount(MountOption::FSName(x[7..].into())),
        x if x.starts_with("subtype=") => ParsedOption::Mount(MountOption::Subtype(x[8..].into())),
        x => ParsedOption::Mount(MountOption::CUSTOM(x.into())),
    }
}

fn main() {
    env_logger::builder()
        .format_timestamp_nanos()
        .parse_filters(std::env::var("RUST_LOG").as_deref().unwrap_or("info"))
        .init();
    if std::env::args()
        .next()
        .as_deref()
        .map(Path::new)
        .and_then(|p| p.file_name())
        .and_then(|p| p.to_str())
        == Some("mount.backup-fs")
    {
        let BasicMountOptions {
            opt,
            data_dir,
            mountpoint,
        } = BasicMountOptions::parse();
        return mount(MountOptions {
            filesystem: FilesystemOptions::parse_from(
                [OsString::from("mount.backup-fs")]
                    .into_iter()
                    .chain(opt.iter().filter_map(|opt| {
                        if let ParsedOption::Mount(MountOption::CUSTOM(opt)) = parse_option(opt) {
                            Some::<OsString>(format!("--{opt}").into())
                        } else {
                            None
                        }
                    }))
                    .chain([data_dir.into_os_string()]),
            ),
            opt: opt
                .into_iter()
                .filter(|o| !matches!(parse_option(o), ParsedOption::Mount(MountOption::CUSTOM(_))))
                .collect(),
            snapshot: None,
            mountpoint,
        });
    }
    let mut app = clap::command!()
        .subcommand(MountOptions::command().name(MOUNT_COMMAND))
        .subcommand(FilesystemOptions::command().name("fsck"))
        .subcommand(FilesystemOptions::command().name(CONVERT_COMMAND))
        .subcommand(InspectOptions::command().name(INSPECT_COMMAND))
        .subcommand(ChangePasswordOptions::command().name("change-password"))
        .subcommand(SnapshotOptions::command().name("snapshot"));
    let matches = app.clone().get_matches();
    match matches.subcommand() {
        Some((MOUNT_COMMAND, sub_m)) => mount(MountOptions::from_arg_matches(sub_m).unwrap()),
        Some(("fsck", sub_m)) => fsck(FilesystemOptions::from_arg_matches(sub_m).unwrap()),
        Some((CONVERT_COMMAND, sub_m)) => {
            convert(FilesystemOptions::from_arg_matches(sub_m).unwrap())
        }
        Some((INSPECT_COMMAND, sub_m)) => {
            let options = InspectOptions::from_arg_matches(sub_m).unwrap();
            let status = checked(backupfs::inspect(&options.data_dir));
            serde_json::to_writer(io::stdout().lock(), &status).unwrap();
            println!();
        }
        Some(("change-password", sub_m)) => {
            change_password(ChangePasswordOptions::from_arg_matches(sub_m).unwrap())
        }
        Some(("snapshot", sub_m)) => snapshots(SnapshotOptions::from_arg_matches(sub_m).unwrap()),
        _ => app.print_long_help().unwrap(),
    }
}

fn convert(options: FilesystemOptions) {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let mut last = None;
    checked(BackupFS::convert_with_progress(
        read_options(options),
        |event| {
            let now = Instant::now();
            let boundary =
                event.phase == ConversionPhase::Complete || event.total == Some(event.done);
            if boundary
                || last.is_none_or(|(phase, time): (ConversionPhase, Instant)| {
                    phase != event.phase || now.duration_since(time) >= Duration::from_millis(250)
                })
            {
                serde_json::to_writer(&mut output, &event).map_err(io::Error::other)?;
                writeln!(&mut output)?;
                output.flush()?;
                last = Some((event.phase, now));
            }
            Ok(())
        },
    ));
}

fn read_password(mut input: impl Read) -> io::Result<Zeroizing<String>> {
    let mut password = Zeroizing::new(String::new());
    input.by_ref().take(4097).read_to_string(&mut password)?;
    if password.len() > 4096 {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "password exceeds 4096 bytes",
        ));
    }
    Ok(password)
}

fn password_from_file(path: Option<&Path>) -> Zeroizing<String> {
    let result = match path {
        Some(path) => std::fs::File::open(path).and_then(read_password),
        None => read_password(io::stdin().lock()),
    };
    checked(result.map_err(Into::into))
}

fn read_options(
    FilesystemOptions {
        mut backup_opts,
        password_file,
    }: FilesystemOptions,
) -> BackupFSOptions {
    let mut password = password_from_file(password_file.as_deref());
    backup_opts.password = std::mem::take(&mut *password);
    backup_opts
}

fn new_fs(options: FilesystemOptions) -> BackupFS {
    checked(BackupFS::new(read_options(options)))
}

fn snapshot_fs(options: FilesystemOptions, name: &str) -> BackupFS {
    checked(BackupFS::open_snapshot(read_options(options), name))
}

fn checked<T>(res: backupfs::error::BkfsResult<T>) -> T {
    let err = match res {
        Ok(fs) => return fs,
        Err(err) => err,
    };
    error!("could not load backup: {err:?}");
    match err.kind {
        BkfsErrorKind::BadChecksum => std::process::exit(4),
        _ => std::process::exit(3),
    }
}

fn mount(
    MountOptions {
        mut filesystem,
        opt,
        snapshot,
        mountpoint,
    }: MountOptions,
) {
    let backup_opts = &mut filesystem.backup_opts;
    if snapshot.is_some() {
        backup_opts.readonly = true;
    }
    let mut config = Config::default();
    for o in &opt {
        match parse_option(o) {
            ParsedOption::Mount(m) => config.mount_options.push(m),
            ParsedOption::AllowOther => config.acl = SessionACL::All,
            ParsedOption::AllowRoot => config.acl = SessionACL::RootAndOwner,
        }
    }

    config
        .mount_options
        .push(MountOption::FSName("backup-fs".to_string()));
    config.mount_options.push(MountOption::DefaultPermissions);
    backup_opts.idmapped = true;

    if backup_opts.setuid_support {
        info!("setuid bit support enabled");
        config.mount_options.push(MountOption::Suid);
    } else if config.mount_options.contains(&MountOption::Suid) {
        info!("setuid bit support enabled");
        backup_opts.setuid_support = true;
    } else {
        config.mount_options.push(MountOption::AutoUnmount);
    }

    if backup_opts.readonly {
        config.mount_options.push(MountOption::RO);
    } else if config.mount_options.contains(&MountOption::RO) {
        backup_opts.readonly = true;
    }

    // AutoUnmount requires a non-Owner acl in 0.17 (fusermount needs
    // allow_other/allow_root); the old fuser injected allow_other here.
    if config.mount_options.contains(&MountOption::AutoUnmount) && config.acl == SessionACL::Owner {
        config.acl = SessionACL::All;
    }

    let filesystem = match snapshot {
        Some(name) => snapshot_fs(filesystem, &name),
        None => new_fs(filesystem),
    };
    let result = fuser::Session::new(filesystem, &mountpoint, &config);
    match result {
        Err(e) => {
            error!("{:?}", e);
            // Permission errors commonly indicate missing user_allow_other configuration.
            if e.kind() == ErrorKind::PermissionDenied {
                std::process::exit(2);
            }
            std::process::exit(1);
        }
        Ok(s) => {
            // Fork before spawning worker threads.
            nix::unistd::daemon(true, true).unwrap();
            let bg = s.spawn().unwrap();
            // Explicit unmount deadlocks against AutoUnmount in the signal handler.
            ctrlc::set_handler(|| std::process::exit(0)).unwrap();
            bg.join().unwrap()
        }
    }
}

fn fsck(options: FilesystemOptions) {
    checked(new_fs(options).fsck())
}

fn change_password(
    ChangePasswordOptions {
        filesystem,
        new_password_file,
    }: ChangePasswordOptions,
) {
    let password = password_from_file(Some(&new_password_file));
    checked(new_fs(filesystem).change_password(&password))
}

fn snapshots(options: SnapshotOptions) {
    match options.command {
        SnapshotCommand::Create(NamedSnapshotOptions { filesystem, name }) => {
            checked(new_fs(filesystem).create_snapshot(&name));
        }
        SnapshotCommand::List(mut filesystem) => {
            filesystem.backup_opts.readonly = true;
            for snapshot in checked(new_fs(filesystem).list_snapshots()) {
                println!("{}", snapshot.name);
            }
        }
        SnapshotCommand::Delete(NamedSnapshotOptions { filesystem, name }) => {
            checked(new_fs(filesystem).delete_snapshot(&name));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_are_not_command_line_arguments() {
        assert!(MountOptions::try_parse_from([
            "backup-fs",
            "/data",
            "/mount",
            "--password",
            "secret"
        ])
        .is_err());
        assert!(ChangePasswordOptions::try_parse_from([
            "backup-fs",
            "/data",
            "--new-password",
            "secret"
        ])
        .is_err());
        assert!(MountOptions::try_parse_from(["backup-fs", "/data", "/mount"]).is_ok());
    }

    #[test]
    fn snapshot_commands_and_recovery_mounts_parse_without_secret_arguments() {
        let options =
            SnapshotOptions::try_parse_from(["snapshot", "create", "/data", "before-update"])
                .unwrap();
        assert!(
            matches!(options.command, SnapshotCommand::Create(NamedSnapshotOptions { name, .. }) if name == "before-update")
        );
        assert!(SnapshotOptions::try_parse_from(["snapshot", "list", "/data"]).is_ok());
        assert!(
            SnapshotOptions::try_parse_from(["snapshot", "delete", "/data", "before-update"])
                .is_ok()
        );
        let options = MountOptions::try_parse_from([
            "mount",
            "/data",
            "/recovery",
            "--snapshot",
            "before-update",
        ])
        .unwrap();
        assert_eq!(options.snapshot.as_deref(), Some("before-update"));
    }

    #[test]
    fn password_input_preserves_bytes_and_is_bounded() {
        for password in [
            "",
            "secret",
            "password\nwith newline",
            "a comma, and Unicode 🔑",
        ] {
            assert_eq!(&*read_password(password.as_bytes()).unwrap(), password);
        }
        assert_eq!(read_password(&vec![b'a'; 4096][..]).unwrap().len(), 4096);
        assert_eq!(
            read_password(&vec![b'a'; 4097][..]).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
    }
}
