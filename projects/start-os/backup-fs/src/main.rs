use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use backupfs::error::BkfsErrorKind;
use backupfs::{BackupFS, BackupFSOptions};
use clap::{CommandFactory, FromArgMatches, Parser};
use fuser::{Config, MountOption, SessionACL};
use log::{error, info};

#[derive(clap::Parser)]
struct MountOptions {
    #[command(flatten)]
    backup_opts: BackupFSOptions,
    #[arg(short = 'o', value_delimiter = ',')]
    opt: Vec<String>,
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
    backup_opts: BackupFSOptions,
    #[arg(long)]
    new_password: String,
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
            backup_opts: BackupFSOptions::parse_from(
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
            mountpoint,
        });
    }
    let mut app = clap::command!()
        .subcommand(MountOptions::command().name("mount"))
        .subcommand(BackupFSOptions::command().name("fsck"))
        .subcommand(ChangePasswordOptions::command().name("change-password"));
    let matches = app.clone().get_matches();
    match matches.subcommand() {
        Some(("mount", sub_m)) => mount(MountOptions::from_arg_matches(sub_m).unwrap()),
        Some(("fsck", sub_m)) => fsck(BackupFSOptions::from_arg_matches(sub_m).unwrap()),
        Some(("change-password", sub_m)) => {
            change_password(ChangePasswordOptions::from_arg_matches(sub_m).unwrap())
        }
        _ => app.print_long_help().unwrap(),
    }
}

fn new_fs(opts: BackupFSOptions) -> BackupFS {
    let res = BackupFS::new(opts);
    let err = match res {
        Ok(fs) => return fs,
        Err(err) => err,
    };
    error!("could not load backup: {err:?}");
    match err.kind {
        // return a special code if the password was incorrect (presumably)
        BkfsErrorKind::BadChecksum => std::process::exit(4),
        // return a special code if we could not load the backend for some other reason
        _ => std::process::exit(3),
    }
}

fn mount(
    MountOptions {
        mut backup_opts,
        opt,
        mountpoint,
    }: MountOptions,
) {
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

    let result = fuser::Session::new(new_fs(backup_opts), &mountpoint, &config);
    match result {
        Err(e) => {
            error!("{:?}", e);
            // Return a special error code for permission denied, which usually indicates that
            // "user_allow_other" is missing from /etc/fuse.conf
            if e.kind() == ErrorKind::PermissionDenied {
                std::process::exit(2);
            }
            std::process::exit(1);
        }
        Ok(s) => {
            // daemon() forks; only the calling thread survives, so spawn the
            // session loop in the child. Normal shutdown is an external
            // unmount (StartOS `umount`), which makes the loop exit via
            // ENODEV and join() return. On SIGINT/SIGTERM just exit and let
            // AutoUnmount tear the mount down — an explicit umount deadlocks
            // against AutoUnmount under spawn().
            nix::unistd::daemon(true, true).unwrap();
            let bg = s.spawn().unwrap();
            ctrlc::set_handler(|| std::process::exit(0)).unwrap();
            bg.join().unwrap()
        }
    }
}

fn fsck(options: BackupFSOptions) {
    new_fs(options).fsck().unwrap()
}

fn change_password(
    ChangePasswordOptions {
        backup_opts,
        new_password,
    }: ChangePasswordOptions,
) {
    new_fs(backup_opts).change_password(&new_password).unwrap()
}
