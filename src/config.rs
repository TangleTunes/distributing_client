use std::path::PathBuf;

use crate::library::app::AppDataBuilder;
use eyre::Context;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ConfigFile {
    pub port: u16,
    pub contract_address: String,
    pub node_url: String,
    pub database_path: String,
    pub chain_id: u16,
    pub fee: u32,
    pub ip_address: String,
}

impl ConfigFile {
    pub fn into_app_builder(self, password: Option<String>, config_path: &str) -> AppDataBuilder {
        let database_path = {
            let mut database_path = PathBuf::from(config_path);
            database_path.pop();
            database_path.push(&self.database_path);
            database_path
        };
        AppDataBuilder {
            port: self.port,
            contract_address: self.contract_address,
            node_url: self.node_url,
            chain_id: self.chain_id,
            fee: self.fee,
            ip_address: self.ip_address,
            database_path: database_path.to_str().unwrap().to_owned(),
            password,
        }
    }

    pub fn from_path(path: &str) -> eyre::Result<Self> {
        Ok(toml::from_str(
            &std::fs::read_to_string(path)
                .wrap_err(format!("Config does not exist at path {:?}", path))?,
        )
        .wrap_err(format!("Could not parse config file at path {:?}", path))?)
    }
}

#[derive(clap::Parser, Debug, Clone)]
#[command(
    name = "TangleTunes distribution client",
    author = "The TangleTunes foundation",
    version = "0.0.1-beta.0",
    about = "A distribution client for TangleTunes"
)]
pub struct Args {
    /// The path to the configuration file
    #[arg(short, long, default_value = "./TangleTunes.toml", global = true)]
    pub config: String,

    /// Then optional password for an encrypted private key
    #[arg(short, long, global = true)]
    pub password: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum Command {
    /// Generate a new wallet
    GenerateWallet {
        /// Required flag for creating a plaintext wallet
        #[arg(short = 'P', long)]
        plaintext: bool,
        /// The password used to encrypt the private key
        #[arg(short, long, required_unless_present("plaintext"))]
        password: Option<String>,
    },

    /// Import an existing wallet
    ImportWallet {
        /// The private key to import
        #[arg(short, long)]
        key: String,
        /// Required flag for creating a plaintext wallet
        #[arg(short = 'P', long)]
        plaintext: bool,
        /// The password used to encrypt the private key
        #[arg(short, long, required_unless_present("plaintext"))]
        password: Option<String>,
    },

    /// Export the IOTA address
    ExportAddress,

    /// Export the private key
    ExportPrivateKey {
        /// Required flag for plaintext export
        #[arg(short = 'P', long, required = true)]
        plaintext: bool,
    },

    /// Start distributing songs
    Run,

    /// Download songs from another distributor
    Download {
        /// The song-ids
        ids: Vec<String>,
        /// Whether to distribute this song
        #[arg(long, short, default_value_t = true)]
        distribute: bool,
    },

    /// Add songs from the file-system
    AddFromPath {
        /// The path where the song is stored as "{song_id}.mp3"
        paths: Vec<String>,
        /// Whether to distribute this song
        #[arg(long, short, default_value_t = true)]
        distribute: bool,
    },

    Remove {
        /// The song-ids
        song_ids: Vec<String>,
    },

    StopDistribution {
        /// The song-ids
        song_ids: Vec<String>,
    },

    StartDistribution {
        /// The song-ids
        song_ids: Vec<String>,
    },

    DownloadLocal {
        /// The local port of the distributor
        #[arg(long, short)]
        distributor_port: u16,

        /// The id of the song to listen to
        #[arg(long, short)]
        song_id: String,

        /// The index to start listening at
        #[arg(long, short)]
        index: usize,

        /// The amount of chunks from index
        #[arg(long, short = 'C')]
        chunks: usize,

        /// The file-name to output into
        #[arg(long, short)]
        file: String,
    },

    CreateAccount {
        #[arg(long, short)]
        name: String,

        #[arg(long, short)]
        description: Option<String>,
    },

    DeleteAccount,

    Deposit {
        #[arg(long, short)]
        amount: u64,
    },

    Withdraw {
        #[arg(long, short)]
        amount: u64,
    },
}
