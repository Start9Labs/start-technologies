use std::collections::BTreeMap;
use std::time::SystemTime;

use imbl_value::InternedString;
use openssl::pkey::{PKey, Private};
use openssl::x509::X509;

use crate::db::model::DatabaseModel;
use crate::hostname::{ServerHostnameInfo, generate_hostname, generate_id};
use crate::net::ssl::{CertBranding, gen_nistp256, make_root_cert};
use crate::prelude::*;
use crate::util::serde::Pem;

fn hash_password(password: &str) -> Result<String, Error> {
    argon2::hash_encoded(
        password.as_bytes(),
        &rand::random::<[u8; 16]>()[..],
        &argon2::Config::rfc9106_low_mem(),
    )
    .with_kind(crate::ErrorKind::PasswordHashGeneration)
}

#[derive(Clone)]
pub struct AccountInfo {
    pub server_id: String,
    pub hostname: ServerHostnameInfo,
    pub password: String,
    pub root_ca_key: PKey<Private>,
    pub root_ca_cert: X509,
    pub ssh_key: ssh_key::PrivateKey,
    pub developer_key: ed25519_dalek::SigningKey,
}
impl AccountInfo {
    pub fn new(
        password: &str,
        start_time: SystemTime,
        hostname: Option<ServerHostnameInfo>,
    ) -> Result<Self, Error> {
        let server_id = generate_id();
        let hostname = if let Some(h) = hostname {
            h
        } else {
            ServerHostnameInfo::from_hostname(generate_hostname())
        };
        let root_ca_key = gen_nistp256()?;
        let branding = CertBranding::start_os(hostname.hostname.as_ref());
        let root_ca_cert = make_root_cert(&root_ca_key, &branding, start_time)?;
        let ssh_key = ssh_key::PrivateKey::from(ssh_key::private::Ed25519Keypair::random(
            &mut crate::util::crypto::os_rng(),
        ));
        let developer_key = ed25519_dalek::SigningKey::generate(&mut crate::util::crypto::os_rng());
        Ok(Self {
            server_id,
            hostname,
            password: hash_password(password)?,
            root_ca_key,
            root_ca_cert,
            ssh_key,
            developer_key,
        })
    }

    pub fn load(db: &DatabaseModel) -> Result<Self, Error> {
        let server_id = db.as_public().as_server_info().as_id().de()?;
        let hostname = ServerHostnameInfo::load(db.as_public().as_server_info())?;
        let password = db.as_private().as_password().de()?;
        let key_store = db.as_private().as_key_store();
        let cert_store = key_store.as_local_certs();
        let root_ca_key = cert_store.as_root_key().de()?.0;
        let root_ca_cert = cert_store.as_root_cert().de()?.0;
        let ssh_key = db.as_private().as_ssh_privkey().de()?.0;
        let compat_s9pk_key = db.as_private().as_developer_key().de()?.0;

        Ok(Self {
            server_id,
            hostname,
            password,
            root_ca_key,
            root_ca_cert,
            ssh_key,
            developer_key: compat_s9pk_key,
        })
    }

    pub fn save(&self, db: &mut DatabaseModel) -> Result<(), Error> {
        let server_info = db.as_public_mut().as_server_info_mut();
        server_info.as_id_mut().ser(&self.server_id)?;
        self.hostname.save(server_info)?;
        server_info
            .as_pubkey_mut()
            .ser(&self.ssh_key.public_key().to_openssh()?)?;
        server_info.as_password_hash_mut().ser(&self.password)?;
        db.as_private_mut().as_password_mut().ser(&self.password)?;
        db.as_private_mut()
            .as_ssh_privkey_mut()
            .ser(Pem::new_ref(&self.ssh_key))?;
        db.as_private_mut()
            .as_developer_key_mut()
            .ser(Pem::new_ref(&self.developer_key))?;
        let key_store = db.as_private_mut().as_key_store_mut();
        let cert_store = key_store.as_local_certs_mut();
        if cert_store.as_root_cert().de()?.0 != self.root_ca_cert {
            cert_store
                .as_root_key_mut()
                .ser(Pem::new_ref(&self.root_ca_key))?;
            cert_store
                .as_root_cert_mut()
                .ser(Pem::new_ref(&self.root_ca_cert))?;
            let int_key = crate::net::ssl::gen_nistp256()?;
            let branding = CertBranding::start_os(self.hostname.hostname.as_ref());
            let int_cert = crate::net::ssl::make_int_cert(
                (&self.root_ca_key, &self.root_ca_cert),
                &int_key,
                &branding,
            )?;
            cert_store.as_int_key_mut().ser(&Pem(int_key))?;
            cert_store.as_int_cert_mut().ser(&Pem(int_cert))?;
            cert_store.as_leaves_mut().ser(&BTreeMap::new())?;
        }
        Ok(())
    }

    pub fn set_password(&mut self, password: &str) -> Result<(), Error> {
        self.password = hash_password(password)?;
        Ok(())
    }

    pub fn hostnames(&self) -> impl IntoIterator<Item = InternedString> + Send + '_ {
        [
            (*self.hostname.hostname).clone(),
            self.hostname.hostname.local_domain_name(),
        ]
    }
}
