use std::collections::BTreeMap;

use patch_db::HasModel;
use serde::{Deserialize, Serialize};

use crate::account::AccountInfo;
use crate::auth::AuthKeys;
use crate::backup::target::cifs::CifsTargets;
use crate::db::model::private::Private;
use crate::db::model::public::Public;
use crate::net::forward::AvailablePorts;
use crate::net::keys::KeyStore;
use crate::notifications::Notifications;
use crate::prelude::*;
use crate::ssh::SshKeys;
use crate::system::KeyboardOptions;
use crate::util::serde::Pem;

pub mod package;
pub mod private;
pub mod public;

#[derive(Debug, Deserialize, Serialize, HasModel)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct Database {
    pub public: Public,
    pub private: Private,
}
impl Database {
    pub fn init(
        account: &AccountInfo,
        kiosk: bool,
        language: Option<InternedString>,
        keyboard: Option<KeyboardOptions>,
    ) -> Result<Self, Error> {
        Ok(Self {
            public: Public::init(account, kiosk, language, keyboard)?,
            private: Private {
                key_store: KeyStore::new(account)?,
                password: account.password.clone(),
                session_pubkeys: AuthKeys::new(),
                ssh_privkey: Pem(account.ssh_key.clone()),
                ssh_pubkeys: SshKeys::new(),
                available_ports: {
                    let mut ports = AvailablePorts::new();
                    ports.set_ssl(80, false);
                    ports.set_ssl(443, true);
                    ports
                },
                notifications: Notifications::new(),
                cifs: CifsTargets::new(),
                package_stores: BTreeMap::new(),
                developer_key: Pem(account.developer_key.clone()),
            }, // TODO
        })
    }
}

pub type DatabaseModel = Model<Database>;
