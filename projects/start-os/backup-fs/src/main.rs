use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use backupfs::error::BkfsErrorKind;
use backupfs::{BackupFS, BackupFSOptions};
use clap::{CommandFactory, FromArgMatches, Parser};
use fuser::MountOption;
use log::{error, info};

#[derive(clap::Parser)]
struct MountOptions {
    #[command(flatten)]
    backup_opts: BackupFSOptions,
    #[arg(short = 'o', value_delimiter = ',')]
    opt: Vec<MountOption>,
    mountpoint: PathBuf,
}

#[derive(clap::Parser)]
struct BasicMountOptions {
    #[arg(short = 'o', value_delimiter = ',')]
    opt: Vec<MountOption>,
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
                        if let MountOption::CUSTOM(opt) = opt {
                            Some::<OsString>(format!("--{opt}").into())
                        } else {
                            None
                        }
                    }))
                    .chain([data_dir.into_os_string()]),
            ),
            opt: opt
                .into_iter()
                .filter(|o| !matches!(o, MountOption::CUSTOM(_)))
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
        mut opt,
        mountpoint,
    }: MountOptions,
) {
    opt.push(MountOption::FSName("backup-fs".to_string()));

    if backup_opts.setuid_support {
        info!("setuid bit support enabled");
        opt.push(MountOption::Suid);
    } else if opt.contains(&MountOption::Suid) {
        info!("setuid bit support enabled");
        backup_opts.setuid_support = true;
    } else {
        opt.push(MountOption::AutoUnmount);
    }

    if backup_opts.readonly {
        opt.push(MountOption::RO);
    } else if opt.contains(&MountOption::RO) {
        backup_opts.readonly = true;
    }

    let result = fuser::Session::new(new_fs(backup_opts), &mountpoint, &opt);
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
        Ok(mut s) => {
            let mut umount = s.unmount_callable();
            ctrlc::set_handler(move || {
                let _ = umount.unmount();
            })
            .unwrap();
            nix::unistd::daemon(true, true).unwrap();
            s.run().unwrap()
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
