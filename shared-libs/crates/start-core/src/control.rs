use clap::Parser;
use patch_db::Dump;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use ts_rs::TS;

use crate::context::{CliContext, RpcContext};
use crate::db::model::DatabaseModel;
use crate::db::model::package::PackageDataEntry;
use crate::prelude::*;
use crate::{Error, PackageId, RpcError};

#[derive(Deserialize, Serialize, Parser, TS)]
#[group(skip)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
#[command(rename_all = "kebab-case")]
pub struct ControlParams {
    #[arg(help = "help.arg.package-id")]
    pub id: PackageId,
}

fn record_start(db: &mut DatabaseModel, id: &PackageId) -> Result<(), Error> {
    db.as_public_mut()
        .as_package_data_mut()
        .as_idx_mut(id)
        .or_not_found(id)?
        .as_status_info_mut()
        .as_desired_mut()
        .map_mutate(|s| Ok(s.start()))?;
    Ok(())
}

#[instrument(skip_all)]
pub async fn start(ctx: RpcContext, ControlParams { id }: ControlParams) -> Result<(), Error> {
    ctx.db.mutate(|db| record_start(db, &id)).await.result?;
    Ok(())
}

pub async fn cli_start(ctx: CliContext, params: ControlParams) -> Result<(), RpcError> {
    ctx.call_remote::<RpcContext>("package.start", to_value(&params)?)
        .await?;
    let blocked = async {
        let dump = from_value::<Dump>(
            ctx.call_remote::<RpcContext>(
                "db.dump",
                imbl_value::json!({ "pointer": format!("/public/packageData/{}", params.id) }),
            )
            .await?,
        )?;
        Model::<PackageDataEntry>::from(dump.value)
            .has_blocking_task(&params.id)
            .map_err(RpcError::from)
    }
    .await;
    match blocked {
        Ok(true) => eprintln!("{}", t!("control.start-critical-task", id = params.id)),
        Ok(false) => {}
        Err(error) => eprintln!(
            "{}: {error}",
            t!("control.start-critical-task-check-failed", id = params.id)
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::config::ClientConfig;
    use crate::status::{DesiredStatus, StatusInfo};

    fn package_entry(blocked: bool) -> serde_json::Value {
        serde_json::json!({
            "statusInfo": StatusInfo::default(),
            "currentDependencies": {},
            "tasks": {
                "configure": {
                    "active": blocked,
                    "task": {
                        "packageId": "service",
                        "actionId": "configure",
                        "severity": "critical"
                    }
                }
            }
        })
    }

    #[test]
    fn start_records_running_intent_with_critical_tasks() {
        let id = "service".parse().unwrap();
        let mut db = DatabaseModel::from(
            to_value(&serde_json::json!({
                "public": { "packageData": { "service": package_entry(true) } }
            }))
            .unwrap(),
        );
        record_start(&mut db, &id).unwrap();
        let entry = db.as_public().as_package_data().as_idx(&id).unwrap();
        assert_eq!(
            entry.as_status_info().de().unwrap().desired,
            DesiredStatus::Running
        );
        assert!(entry.has_blocking_task(&id).unwrap());
        assert_eq!(entry.as_status_info().de().unwrap().started, None);
    }

    #[test]
    fn start_params_preserve_wire_shape_and_reject_force() {
        let params = ControlParams::try_parse_from(["start", "service"]).unwrap();
        assert_eq!(
            serde_json::to_value(&params).unwrap(),
            serde_json::json!({ "id": "service" })
        );
        let wire: ControlParams =
            serde_json::from_value(serde_json::json!({ "id": "service" })).unwrap();
        assert_eq!(wire.id, params.id);
        assert_eq!(
            ControlParams::try_parse_from(["start", "service", "--force"])
                .err()
                .unwrap()
                .kind(),
            clap::error::ErrorKind::UnknownArgument
        );
    }

    #[tokio::test]
    async fn cli_start_stderr() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        if let Ok(host) = std::env::var("START_CORE_TEST_START_HOST") {
            let ctx =
                CliContext::init(ClientConfig::try_parse_from(["test", "-H", &host]).unwrap())
                    .unwrap();
            ctx.id_key
                .set(ed25519_dalek::SigningKey::from_bytes(&[7; 32]))
                .unwrap();
            let result = cli_start(
                ctx,
                ControlParams {
                    id: "service".parse().unwrap(),
                },
            )
            .await;
            if std::env::var("START_CORE_TEST_START_CASE").unwrap() == "start-failed" {
                assert!(result.is_err());
            } else {
                result.unwrap();
            }
            return;
        }
        for case in ["blocked", "clear", "inspection-failed", "start-failed"] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let host = format!("http://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let count = if case == "start-failed" { 1 } else { 2 };
                for index in 0..count {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let mut request = Vec::new();
                    loop {
                        let byte = stream.read_u8().await.unwrap();
                        request.push(byte);
                        if request.ends_with(b"\r\n\r\n") {
                            break;
                        }
                    }
                    let headers = String::from_utf8(request).unwrap();
                    let length: usize = headers
                        .lines()
                        .find_map(|line| {
                            let (key, value) = line.split_once(':')?;
                            key.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse().unwrap())
                        })
                        .unwrap();
                    let mut body = vec![0; length];
                    stream.read_exact(&mut body).await.unwrap();
                    let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
                    let expected_method = if index == 0 {
                        "package.start"
                    } else {
                        "db.dump"
                    };
                    assert_eq!(request["method"], expected_method);
                    assert_eq!(
                        request["params"],
                        if index == 0 {
                            serde_json::json!({"id": "service"})
                        } else {
                            serde_json::json!({"pointer": "/public/packageData/service"})
                        }
                    );
                    let response = if case == "start-failed" || (index == 1 && case == "inspection-failed") {
                        serde_json::json!({"jsonrpc": "2.0", "id": request["id"], "error": {"code": -32603, "message": "test failure"}})
                    } else {
                        let result = if index == 0 { serde_json::Value::Null } else {
                            serde_json::json!({"id": 1, "value": package_entry(case == "blocked")})
                        };
                        serde_json::json!({"jsonrpc": "2.0", "id": request["id"], "result": result})
                    }.to_string();
                    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).as_bytes()).await.unwrap();
                }
            });
            let output = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                tokio::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "control::tests::cli_start_stderr", "--nocapture"])
                    .env("START_CORE_TEST_START_HOST", host)
                    .env("START_CORE_TEST_START_CASE", case)
                    .env("LANG", "en_US.UTF-8")
                    .output(),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(
                output.status.success(),
                "{case}: {}",
                String::from_utf8_lossy(&output.stdout)
            );
            server.await.unwrap();
            let stderr = String::from_utf8(output.stderr).unwrap();
            match case {
                "blocked" => assert!(stderr.contains("Warning: startup requested for service service; unresolved critical tasks block runtime startup"), "{stderr}"),
                "inspection-failed" => assert!(stderr.contains("Warning: startup requested for service service, but checking for blocking critical tasks failed"), "{stderr}"),
                _ => assert!(stderr.is_empty(), "{case}: {stderr}"),
            }
        }
    }
}

pub async fn stop(ctx: RpcContext, ControlParams { id }: ControlParams) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(&id)
                .or_not_found(&id)?
                .as_status_info_mut()
                .stop()
        })
        .await
        .result?;

    Ok(())
}

pub async fn restart(ctx: RpcContext, ControlParams { id }: ControlParams) -> Result<(), Error> {
    ctx.db
        .mutate(|db| {
            db.as_public_mut()
                .as_package_data_mut()
                .as_idx_mut(&id)
                .or_not_found(&id)?
                .as_status_info_mut()
                .restart()
        })
        .await
        .result?;

    Ok(())
}
