use std::{convert::TryInto, net::SocketAddr, time::Duration};

use async_std::stream;
use chrono::{DateTime, NaiveDateTime, Utc};
use futures::{channel::mpsc::unbounded, stream::once, Sink, SinkExt, StreamExt};
use log::{error, info};
use sled::{IVec, Tree};
use warp::{ws::Message, Filter};

use serde::{Deserialize, Serialize};

use super::{Data, Sensors};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Response {
    #[serde(rename = "new_data")]
    NewData(Data),
    #[serde(rename = "sync")]
    Sync(Vec<Data>),
}

fn conv_data(data: (IVec, IVec)) -> Data {
    let ts = i64::from_be_bytes(data.0.as_ref().try_into().unwrap());
    let sensors: Sensors = bincode::deserialize(data.1.as_ref()).unwrap();
    Data {
        sensors,
        timestamp: DateTime::from_utc(NaiveDateTime::from_timestamp(ts, 0), Utc),
    }
}

async fn send_data<E>(
    data: (IVec, IVec),
    sink: &mut (impl Sink<Message, Error = E> + Unpin),
) -> Result<(), E> {
    let resp = Response::NewData(conv_data(data));
    let resp = serde_json::to_string(&resp).unwrap();
    sink.send(Message::text(resp)).await
}

pub async fn serve(tree: Tree, bind: SocketAddr) {
    warp::serve(
        warp::path("data")
            .and(warp::ws())
            .map(move |ws: warp::ws::Ws| {
                ws.on_upgrade({
                    let tree = tree.clone();
                    move |websocket| async move {
                        let (tx, rx) = websocket.split();

                        let (sender, receiver) = unbounded();

                        tokio::spawn(async move {
                            if let Err(e) = receiver.map(Ok).forward(tx).await {
                                info!("forwarding error: {:?}", e);
                            }
                        });

                        tokio::spawn({
                            let mut sender = sender.clone();
                            let tree = tree.clone();
                            async move {
                                let mut timer = Box::pin(
                                    once(async { () })
                                        .chain(stream::interval(Duration::from_secs(11))),
                                );
                                while let Some(()) = timer.next().await {
                                    match tree.last() {
                                        Ok(Some(data)) => {
                                            if let Err(e) = send_data(data, &mut sender).await {
                                                info!("sending error {:?}", e);
                                                break;
                                            }
                                        }
                                        Err(e) => error!("sled error: {:?}", e),
                                        _ => {}
                                    }
                                }
                            }
                        });

                        rx.for_each({
                            let tree = tree.clone();
                            move |message| {
                                let tree = tree.clone();
                                let mut sender = sender.clone();
                                async move {
                                    if let Ok(message) = message {
                                        let message =
                                            String::from_utf8_lossy(&message.into_bytes())
                                                .into_owned();
                                        match message.as_str() {
                                            "sync" => {
                                                let mut iter = tree.iter();
                                                let mut data = vec![];
                                                while let Some(Ok(d)) = iter.next() {
                                                    data.push(conv_data(d));
                                                }
                                                if let Err(e) = sender
                                                    .send(Message::text(
                                                        serde_json::to_string(&Response::Sync(
                                                            data,
                                                        ))
                                                        .unwrap(),
                                                    ))
                                                    .await
                                                {
                                                    info!("sending error: {:?}", e);
                                                }
                                            }
                                            a => {
                                                if a.starts_with("since") {
                                                    if let Some(ts) = a.split(" ").nth(1) {
                                                        let timestamp: Result<DateTime<Utc>, _> =
                                                            ts.parse();
                                                        if let Ok(ts) = timestamp {
                                                            let ts = ts.timestamp().to_be_bytes();
                                                            let mut iter = tree.range(ts..);
                                                            while let Some(Ok(d)) = iter.next() {
                                                                if let Err(e) =
                                                                    send_data(d, &mut sender).await
                                                                {
                                                                    info!("sending error: {:?}", e);
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        })
                        .await;
                    }
                })
            })
            .or(warp::any().and(warp::fs::dir("./frontend"))),
    )
    .run(bind)
    .await;
}
