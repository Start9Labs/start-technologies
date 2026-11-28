use super::*;

async fn dependency_archive(payloads: &[&[u8]]) -> tokio::fs::File {
    let file = anonymous_file().unwrap().into_std().await;
    let mut archive = tar::Builder::new(file);
    let mut header = tar::Header::new_gnu();
    header.set_mode(0o600);
    header.set_size(0);
    header.set_cksum();
    archive
        .append_data(&mut header, CAPTURE_FILE, std::io::empty())
        .unwrap();
    append_json(&mut archive, KEYS_FILE, &Vec::<OsString>::new()).unwrap();
    append_profiles(&mut archive, &crate::net::wifi::NetworkProfiles::new()).unwrap();
    for payload in payloads {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o600);
        header.set_size(payload.len() as u64);
        header.set_cksum();
        archive
            .append_data(&mut header, BACKINGS_FILE, *payload)
            .unwrap();
    }
    tokio::fs::File::from_std(archive.into_inner().unwrap())
}

#[tokio::test]
async fn backing_reference_transport_requires_one_valid_guid_set_without_an_empty_fallback() {
    for payloads in [
        vec![],
        vec![b"{}".as_slice()],
        vec![b"[\"not-a-guid\"]".as_slice()],
        vec![b"[]".as_slice(), b"[]".as_slice()],
    ] {
        assert!(receive(dependency_archive(&payloads).await).await.is_err());
    }
    let (_, _, _, backings) = receive(dependency_archive(&[b"[]"]).await).await.unwrap();
    assert!(backings.is_empty());
}
