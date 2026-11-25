use super::*;
use crate::trusted_keys::Registries;
use crate::trusted_keys::connection::is_connected;

#[test]
fn install_names_the_registry_as_the_server_saved_it() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = CliContext::init(ClientConfig {
        registry: Some("https://registry.example/base/".parse().unwrap()),
        id_key_path: Some(temp.path().join("id.pem")),
        ..Default::default()
    })
    .unwrap();
    let saved = Registries::from([("https://registry.example/base".into(), None)]);

    let base = ctx.registry_url.as_ref().unwrap().unpinned();
    assert_eq!(base.as_str(), "https://registry.example/base/");
    assert!(is_connected(&saved, base.as_str()));
}
