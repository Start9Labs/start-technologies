use std::path::Path;

use tokio::process::Command;

use crate::ImageId;
use crate::prelude::*;
use crate::util::env::Environment;

pub(super) fn configure(
    command: &mut Command,
    image_id: &ImageId,
    environment: Option<&Environment>,
) -> Result<(), Error> {
    if let Some(environment) = environment {
        command.env(
            "JSON_ENV",
            serde_json::to_string(environment).with_kind(ErrorKind::Serialization)?,
        );
    } else {
        command.env_remove("JSON_ENV").arg("--env-file").arg(
            Path::new("/media/startos/images")
                .join(image_id)
                .with_extension("env"),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn image_environment_reaches_attach_without_command_line_values() {
        let image_id = "main".parse().unwrap();
        let environment =
            Environment::from([("MULTILINE".into(), Some("first\nsecond=value\n".into()))]);
        let mut command = Command::new("lxc-attach");
        configure(&mut command, &image_id, Some(&environment)).unwrap();
        assert_eq!(command.as_std().get_args().count(), 0);
        let json = command
            .as_std()
            .get_envs()
            .find(|(key, _)| *key == "JSON_ENV")
            .unwrap()
            .1
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Environment>(json.as_encoded_bytes()).unwrap(),
            environment
        );
    }

    #[test]
    fn legacy_attach_uses_env_file_without_inheriting_json_env() {
        let mut command = Command::new("lxc-attach");
        configure(&mut command, &"main".parse().unwrap(), None).unwrap();
        assert_eq!(
            command.as_std().get_args().collect::<Vec<_>>(),
            ["--env-file", "/media/startos/images/main.env"]
        );
        assert_eq!(
            command
                .as_std()
                .get_envs()
                .find(|(key, _)| *key == OsStr::new("JSON_ENV")),
            Some((OsStr::new("JSON_ENV"), None))
        );
    }
}
