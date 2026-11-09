//! Which of UPnP, PCP and NAT-PMP the router here answers — and whether it
//! opens a port. `cargo run -p aw-sync --example portmap_probe`
use std::{num::NonZeroU16, time::Duration};

#[tokio::main]
async fn main() {
    let client = portmapper::Client::new(portmapper::Config::default());
    match client.probe().await {
        Ok(Ok(found)) => println!("router answers: {found:?}"),
        Ok(Err(e)) => println!("probe failed: {e}"),
        Err(e) => println!("probe gone: {e}"),
    }
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    client.update_local_port(NonZeroU16::new(port).unwrap());
    client.procure_mapping();
    let mut watch = client.watch_external_address();
    let got = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Some(a) = *watch.borrow() {
                return a;
            }
            if watch.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
    })
    .await;
    match got {
        Ok(a) => println!("port {port} opened as {}.x.x.{}:{}", a.ip().octets()[0], a.ip().octets()[3], a.port()),
        Err(_) => println!("port {port}: no mapping within 15 s"),
    }
}
