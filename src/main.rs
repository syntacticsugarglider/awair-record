use async_std::{fs::OpenOptions, path::PathBuf, stream};
use awair_record::api;
use futures::{stream::once, AsyncReadExt, AsyncWriteExt, StreamExt};
use log::error;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize, Deserialize, Default)]
pub struct Config {
    awair_local_uri: String,
    bind_addr: String,
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let config_exists = PathBuf::from("config.toml").exists().await;

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open("config.toml")
        .await
        .expect("cannot read or create config.toml");

    if !config_exists {
        file.write_all(toml::to_string(&Config::default()).unwrap().as_bytes())
            .await
            .expect("cannot write default config file");
        error!("config not present, default config file generated");
        return;
    }

    let mut buffer = String::new();
    file.read_to_string(&mut buffer)
        .await
        .expect("cannot read config.toml");

    let config: Config = toml::from_str(&buffer).expect("invalid config file");

    let db_path = PathBuf::from("./db");
    let db = sled::open(&db_path).expect(&format!(
        "failed to create database directory at {:?}",
        db_path
    ));

    let tree = db
        .open_tree("data")
        .expect("cannot open database tree, corrupted?");

    let tree_handle = tree.clone();

    let mut timer = Box::pin(once(async { () }).chain(stream::interval(Duration::from_secs(11))));

    let bind_addr = config.bind_addr;
    tokio::spawn(async move {
        api::frontend::serve(tree_handle, bind_addr.as_str().parse().unwrap()).await;
    });

    while let Some(()) = timer.next().await {
        let data: api::Data = surf::get(&config.awair_local_uri)
            .recv_json()
            .await
            .unwrap();

        let ts = data.timestamp.timestamp().to_be_bytes();
        if let Ok(data) = bincode::serialize(&data.sensors) {
            if let Err(e) = tree.insert(&ts, data) {
                error!("insertion error: {:?}", e);
            }
        }
    }
}
