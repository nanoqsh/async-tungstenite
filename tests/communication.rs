#![cfg(feature = "handshake")]

use async_std::net::{TcpListener, TcpStream};
use async_std::task;
use async_tungstenite::{accept_async, client_async, WebSocketStream};
use futures::prelude::*;
use log::*;
use tungstenite::Message;

async fn run_connection<S>(
    connection: WebSocketStream<S>,
    msg_tx: futures::channel::oneshot::Sender<Vec<Message>>,
) where
    S: AsyncRead + AsyncWrite + Unpin,
{
    info!("Running connection");
    let mut connection = connection;
    let mut messages = vec![];
    while let Some(message) = connection.next().await {
        info!("Message received");
        let message = message.expect("Failed to get message");
        messages.push(message);
    }
    msg_tx.send(messages).expect("Failed to send results");
}

#[async_std::test]
async fn communication() {
    let _ = env_logger::try_init();

    let (con_tx, con_rx) = futures::channel::oneshot::channel();
    let (msg_tx, msg_rx) = futures::channel::oneshot::channel();

    let f = async move {
        let listener = TcpListener::bind("127.0.0.1:12345").await.unwrap();
        info!("Server ready");
        con_tx.send(()).unwrap();
        info!("Waiting on next connection");
        let (connection, _) = listener.accept().await.expect("No connections to accept");
        let stream = accept_async(connection).await;
        let stream = stream.expect("Failed to handshake with connection");
        run_connection(stream, msg_tx).await;
    };

    task::spawn(f);

    info!("Waiting for server to be ready");

    con_rx.await.expect("Server not ready");
    let tcp = TcpStream::connect("127.0.0.1:12345")
        .await
        .expect("Failed to connect");
    let url = "ws://localhost:12345/";
    let (mut stream, _) = client_async(url, tcp)
        .await
        .expect("Client failed to connect");

    for i in 1..10 {
        info!("Sending message");
        stream
            .send(Message::text(format!("{}", i)))
            .await
            .expect("Failed to send message");
    }

    stream.close(None).await.expect("Failed to close");

    info!("Waiting for response messages");
    let messages = msg_rx.await.expect("Failed to receive messages");
    assert_eq!(messages.len(), 10);
}

#[async_std::test]
async fn split_communication() {
    let _ = env_logger::try_init();

    let (con_tx, con_rx) = futures::channel::oneshot::channel();
    let (msg_tx, msg_rx) = futures::channel::oneshot::channel();

    let f = async move {
        let listener = TcpListener::bind("127.0.0.1:12346").await.unwrap();
        info!("Server ready");
        con_tx.send(()).unwrap();
        info!("Waiting on next connection");
        let (connection, _) = listener.accept().await.expect("No connections to accept");
        let stream = accept_async(connection).await;
        let stream = stream.expect("Failed to handshake with connection");
        run_connection(stream, msg_tx).await;
    };

    task::spawn(f);

    info!("Waiting for server to be ready");

    con_rx.await.expect("Server not ready");
    let tcp = TcpStream::connect("127.0.0.1:12346")
        .await
        .expect("Failed to connect");
    let url = url::Url::parse("ws://localhost:12346/").unwrap();
    let (stream, _) = client_async(url, tcp)
        .await
        .expect("Client failed to connect");

    let (tx, rx) = stream.split();

    for i in 1..10 {
        info!("Sending message");
        tx.send(Message::text(format!("{}", i)))
            .await
            .expect("Failed to send message");
    }

    tx.close(None).await.expect("Failed to close");

    info!("Waiting for response messages");
    let messages = msg_rx.await.expect("Failed to receive messages");
    assert_eq!(messages.len(), 10);

    assert!(tx.is_pair_of(&rx));
    assert!(rx.is_pair_of(&tx));
    WebSocketStream::reunite(tx, rx).expect("Failed to reunite the stream");
}

#[async_std::test]
async fn concurrent_send() {
    let _ = env_logger::try_init();

    let (con_tx, con_rx) = futures::channel::oneshot::channel();
    let (msg_tx, msg_rx) = futures::channel::oneshot::channel();

    let f = async move {
        let listener = TcpListener::bind("127.0.0.1:12347").await.unwrap();
        info!("Server ready");
        con_tx.send(()).unwrap();
        info!("Waiting on next connection");
        let (connection, _) = listener.accept().await.expect("No connections to accept");
        let stream = accept_async(connection).await;
        let stream = stream.expect("Failed to handshake with connection");
        run_connection(stream, msg_tx).await;
    };

    task::spawn(f);

    info!("Waiting for server to be ready");

    con_rx.await.expect("Server not ready");
    let tcp = TcpStream::connect("127.0.0.1:12347")
        .await
        .expect("Failed to connect");
    let url = url::Url::parse("ws://localhost:12347/").unwrap();
    let (stream, _) = client_async(url, tcp)
        .await
        .expect("Client failed to connect");

    let (tx, _rx) = stream.split();

    // the `WebSocketSender::send` takes a shared `&self`, so you can call it concurrently.
    // this test case checks that it works
    let results = futures::future::join_all((1..10).map(|i| {
        info!("Sending message");
        tx.send(Message::text(format!("{}", i)))
    }))
    .await;

    assert!(results.iter().all(Result::is_ok));

    tx.close(None).await.expect("Failed to close");

    info!("Waiting for response messages");
    let messages = msg_rx.await.expect("Failed to receive messages");
    assert_eq!(messages.len(), 10);
}
