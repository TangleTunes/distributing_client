use crate::library::{
    client::TangleTunesClient,
    crypto::{self, Wallet},
    database::Database,
};
use std::{fmt::Debug, net::IpAddr, path::PathBuf};

#[derive(Debug)]
pub struct AppData {
    pub password: Option<String>,
    pub port: u16,
    pub ip_address: IpAddr,
    pub contract_address: String,
    pub node_url: String,
    pub database_path: PathBuf,
    pub chain_id: u16,
    pub fee: u32,
    pub database: Database,
    pub client: TangleTunesClient,
}

pub struct AppDataBuilder {
    pub port: u16,
    pub contract_address: String,
    pub node_url: String,
    pub database_path: String,
    pub chain_id: u16,
    pub fee: u32,
    pub ip_address: String,
    pub password: Option<String>,
}

impl AppDataBuilder {
    pub async fn build(self) -> eyre::Result<&'static AppData> {
        Self::_build(self, false).await
    }

    async fn _build(self, in_memory: bool) -> eyre::Result<&'static AppData> {
        let database = if in_memory {
            Database::initialize_in_memory().await?
        } else {
            Database::initialize(&self.database_path).await?
        };

        let wallet = {
            if let Some((key, encrypted)) = database.get_key().await? {
                let key = match (encrypted, &self.password) {
                    (true, Some(password)) => Ok(crypto::decrypt_private_key(&key, &password)?),
                    (false, None) => Ok(key),
                    (true, None) => Err(eyre!("Wallet is encrypted, please give a password.")),
                    (false, Some(_)) => Err(eyre!("Wallet is not encrypted, no password needed.")),
                }?;
                Ok(Wallet::from_private_key(&key, self.chain_id)?)
            } else {
                Err(eyre!("No private key found. Import or generate one!"))
            }
        }?;

        let client =
            TangleTunesClient::initialize(wallet, &self.node_url, &self.contract_address).await?;

        let app = AppData {
            ip_address: self.ip_address.parse()?,
            password: self.password,
            port: self.port,
            contract_address: self.contract_address,
            node_url: self.node_url,
            database,
            database_path: PathBuf::from(self.database_path),
            chain_id: self.chain_id,
            fee: self.fee,
            client,
        };

        Ok(Box::leak(Box::new(app)))
    }
}

#[cfg(test)]
pub mod test {
    use super::{AppData, AppDataBuilder};
    use crate::config::ConfigFile;
    use eyre::Context;

    impl AppData {
        /// Overrides:
        /// - database_path to ":memory:" (in memory database)
        /// - ip_address to "127.0.0.1"
        pub async fn init_for_test(port: Option<u16>, in_memory: bool) -> eyre::Result<&'static AppData> {
            let mut builder = ConfigFile::from_path("TangleTunes.toml")
                .wrap_err("Cannot run tests without config file at ./TangleTunes.toml")?
                .into_app_builder(None, "TangleTunes.toml");

            if let Some(port) = port {
                builder.port = port;
            }
            builder.ip_address = "127.0.0.1".to_string();

            AppDataBuilder::_build(builder, in_memory).await
        }
    }
}
