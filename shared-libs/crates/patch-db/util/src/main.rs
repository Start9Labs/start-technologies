use patch_db::json_ptr::{JsonPointer, PtrSegment, ROOT};
use serde_json::Value;

#[tokio::main]
async fn main() {
    let mut app = clap::Command::new("patch-db-util")
        .subcommand(
            clap::Command::new("dump").arg(
                clap::Arg::new("PATH")
                    .required(true)
                    .help("Path to the database file"),
            ),
        )
        .subcommand(
            clap::Command::new("from-dump").arg(
                clap::Arg::new("PATH")
                    .required(true)
                    .help("Path to the database file"),
            ),
        );

    match app.clone().get_matches().subcommand() {
        Some(("dump", matches)) => {
            let path = matches.get_one::<String>("PATH").unwrap();
            let db = patch_db::PatchDb::open(path).await.unwrap();
            let dump = db.dump(&ROOT).await;
            serde_json::to_writer_pretty(&mut std::io::stdout(), &dump.value).unwrap();
            println!();
        }
        Some(("from-dump", matches)) => {
            let path = matches.get_one::<String>("PATH").unwrap();
            let value: Value = serde_json::from_reader(&mut std::io::stdin()).unwrap();
            let db = patch_db::PatchDb::open(path).await.unwrap();
            db.put(
                &JsonPointer::<&str, (&[PtrSegment], &[PtrSegment])>::default(),
                &value,
            )
            .await
            .unwrap();
        }
        _ => app.print_long_help().unwrap(),
    }
}
