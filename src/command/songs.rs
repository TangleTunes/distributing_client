use crate::{library::app::App, library::util::SongId, BYTES_PER_CHUNK_USIZE};
use ethers::types::{Address, H160, U256};
use eyre::Context;
use num_integer::div_ceil;
use std::{fs::OpenOptions, io::Write, path::PathBuf};

const ZERO_ADDRESS: Address = H160([0; 20]);

pub async fn remove(ids: Vec<String>, cfg: &'static App) -> eyre::Result<()> {
    println!("Removing songs: {ids:?}\n");
    for id in &ids {
        let song_id = match SongId::try_from_hex(id) {
            Ok(song_id) => song_id,
            Err(_) => cfg
                .database
                .get_song_id_by_index(
                    id.parse()
                        .wrap_err("Identifier was not a valid song-id or song-index")?,
                )
                .await?
                .ok_or_else(|| eyre!("Song-index not found"))?,
        };

        if cfg.database.remove_song(&song_id).await? {
            println!("Succesfully removed song {id:?}");
        } else {
            println!("Song with id {id:?} does not exist, cannot be removed");
        }
    }
    Ok(())
}

pub async fn add(paths: Vec<String>, cfg: &'static App) -> eyre::Result<()> {
    println!("Adding songs: {paths:?}");

    for path in paths {
        let path = PathBuf::from(path);
        let Some(ext) = path.extension() else {
            bail!("File name must be <SONG_ID>.mp3")
        };
        if ext != "mp3" {
            bail!("File name must be <SONG_ID>.mp3")
        }
        let data = std::fs::read(&path)?;
        let song_id = path.file_stem().unwrap().to_str().unwrap();
        cfg.database
            .add_song(&SongId::try_from_hex(song_id)?, &data)
            .await?;
        println!("Added song with id {}", song_id);
    }
    Ok(())
}

pub(crate) async fn run_list(app: &'static App) -> eyre::Result<()> {
    println!("Songs stored locally:");
    for song_id in app.database.get_all_downloaded_song_ids().await? {
        let index = match app.database.get_index_by_song_id(&song_id).await? {
            Some(index) => format!("index: {index}"),
            None => "index not found".to_string(),
        };
        println!("{song_id} - {index}");
    }
    Ok(())
}

pub async fn download(
    app: &'static App,
    song_id: String,
    to_file: Option<String>,
    max_price: U256,
) -> eyre::Result<()> {
    let song_id = song_id.parse()?;
    let song_info = app.client.get_song_info(song_id).await?;
    let user_info = app
        .client
        .get_user_info(app.client.wallet_address())
        .await?;

    if song_info.price > max_price {
        bail!(
            "Selected song is too expensive. (price = {}, max_price = {})",
            song_info.price,
            max_price
        )
    }

    if song_info.total_price() > user_info.balance {
        bail!(
            "Not enough funds! (song = {}, balance = {})",
            song_info.total_price(),
            user_info.balance
        )
    }

    let distribution = app.client.get_rand_distributor(song_id).await?;

    if distribution.distributor == ZERO_ADDRESS {
        bail!("No distributor found for song {song_id}");
    }

    let song = app
        .client
        .download_from_distributor(
            distribution.server.parse()?,
            song_id,
            0,
            div_ceil(song_info.len.as_usize(), BYTES_PER_CHUNK_USIZE),
            distribution.distributor,
        )
        .await?;

    match to_file {
        Some(to_file) => {
            let mut file = OpenOptions::new().write(true).create(true).open(&to_file)?;
            file.write_all(&song)?;
            file.flush()?;
            println!("Wrote mp3 to {}", to_file)
        }
        None => {
            app.database.add_song(&song_id, &song).await?;
            println!("Succesfully added song {song_id} to the database");
        }
    }

    Ok(())
}

pub async fn download_direct(
    app: &'static App,
    socket_address: String,
    song_id: String,
    file: String,
    first_chunk_id: usize,
    chunks_requested: usize,
    distributor_address: String,
) -> eyre::Result<()> {
    let song = app
        .client
        .download_from_distributor(
            socket_address.parse()?,
            SongId::try_from_hex(&song_id)?,
            first_chunk_id,
            chunks_requested,
            distributor_address.parse()?,
        )
        .await?;

    let mut file = OpenOptions::new().write(true).create(true).open(file)?;
    file.write_all(&song)?;
    file.flush()?;

    Ok(())
}
