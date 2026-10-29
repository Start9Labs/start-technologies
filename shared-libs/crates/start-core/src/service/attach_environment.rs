use std::path::Path;

use tokio::process::Command;

use crate::ImageId;
use crate::prelude::*;
use crate::util::env::{Environment, is_json_env_chunk, json_env_chunks};

pub(super) fn configure(
    command: &mut Command,
    image_id: &ImageId,
    environment: Option<&Environment>,
) -> Result<(), Error> {
    for (name, _) in std::env::vars_os().filter(|(name, _)| is_json_env_chunk(name)) {
        command.env_remove(name);
    }
    if let Some(environment) = environment {
        let json = serde_json::to_string(environment).with_kind(ErrorKind::Serialization)?;
        command.envs(json_env_chunks(&json));
    } else {
        command.arg("--env-file").arg(
            Path::new("/media/startos/images")
                .join(image_id)
                .with_extension("env"),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::env::parse_json_env;

    #[test]
    fn image_environment_reaches_attach_without_command_line_values() {
        let image_id = "main".parse().unwrap();
        let environment =
            Environment::from([("MULTILINE".into(), Some("first\nsecond=value\n".into()))]);
        let mut command = Command::new("lxc-attach");
        configure(&mut command, &image_id, Some(&environment)).unwrap();
        assert_eq!(command.as_std().get_args().count(), 0);
        let overrides = parse_json_env(command.as_std().get_envs().filter_map(|(name, value)| {
            value.map(|value| (name.to_os_string(), value.to_os_string()))
        }))
        .unwrap();
        assert_eq!(overrides, environment);
    }

    #[test]
    fn legacy_attach_uses_env_file_without_inheriting_json_env() {
        const CHILD: &str = "STARTOS_JSON_ENV_ATTACH_TEST";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["service::attach_environment::tests::legacy_attach_uses_env_file_without_inheriting_json_env", "--exact"])
                .env(CHILD, "1")
                .env("JSON_ENV_0", "{\"LEAKED\":\"parent\"}")
                .env("JSON_ENV_12", "stale")
                .output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            return;
        }
        let mut command = Command::new("lxc-attach");
        configure(&mut command, &"main".parse().unwrap(), None).unwrap();
        assert_eq!(
            command.as_std().get_args().collect::<Vec<_>>(),
            ["--env-file", "/media/startos/images/main.env"]
        );
        for name in ["JSON_ENV_0", "JSON_ENV_12"] {
            assert!(
                command
                    .as_std()
                    .get_envs()
                    .any(|(key, value)| key == name && value.is_none())
            );
        }
    }
}
