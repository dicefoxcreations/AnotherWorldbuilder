use aw_crypto::{Identity, IdentityPublic};
use aw_sync::world::{Ask, Host, Offer, Reply, WorldNet};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

/// A game master with one world: who is a member, who asked to join.
struct Gm {
    offered: Mutex<bool>,
    members: Mutex<Vec<String>>,
    requests: Mutex<Vec<String>>,
}

impl Host for Gm {
    fn offers(&self) -> Vec<Offer> {
        if !*self.offered.lock().unwrap() {
            return Vec::new();
        }
        vec![Offer { world: "world:eldermere".into(), name: "Eldermere".into(), icon: "🏰".into(), gm: "Marla".into() }]
    }
    fn answer(&self, _world: &str, who: &IdentityPublic, ask: Ask) -> Reply {
        let id = who.id.to_string();
        if self.members.lock().unwrap().contains(&id) {
            return match ask {
                Ask::Swap { give, .. } => Reply::Records { records: vec![format!("the world for {}", who.name)], wanted: give, digest: "d".into() },
                _ => Reply::Digest { digest: "d".into(), presence: Vec::new(), rolls: Vec::new(), handover: false, requests: Vec::new(), no_copy: false },
            };
        }
        match ask {
            Ask::Join => {
                self.requests.lock().unwrap().push(id);
                Reply::Requested
            }
            _ => Reply::NotMember,
        }
    }
}

struct Nobody;
impl Host for Nobody {
    fn offers(&self) -> Vec<Offer> {
        Vec::new()
    }
    fn answer(&self, _: &str, _: &IdentityPublic, _: Ask) -> Reply {
        Reply::NotOffered
    }
}

fn gm() -> Arc<Gm> {
    Arc::new(Gm { offered: Mutex::new(true), members: Mutex::new(Vec::new()), requests: Mutex::new(Vec::new()) })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stranger_asks_to_join_and_gets_the_world_once_admitted() {
    let host = gm();
    let gm_net = WorldNet::start(host.clone(), aw_sync::world::Reach { lan: true, ..Default::default() }).await.unwrap();
    let player_net = WorldNet::start(Arc::new(Nobody), aw_sync::world::Reach { lan: true, ..Default::default() }).await.unwrap();
    let bob = Identity::create("Bob").unwrap();
    let at = gm_net.loopback_addr();

    let reply = player_net.ask_at(at.clone(), "world:eldermere", &bob, Ask::Digest { presence: None, rolls: Vec::new() }).await.unwrap();
    assert_eq!(reply, Reply::NotMember);

    let reply = player_net.ask_at(at.clone(), "world:eldermere", &bob, Ask::Join).await.unwrap();
    assert_eq!(reply, Reply::Requested);
    assert_eq!(*host.requests.lock().unwrap(), vec![bob.id().to_string()], "the request waits for the GM");

    host.members.lock().unwrap().push(bob.id().to_string());
    let big = vec!["x".repeat(100_000)];
    let reply = player_net.ask_at(at, "world:eldermere", &bob, Ask::Swap { have: vec![], give: big.clone() }).await.unwrap();
    assert_eq!(reply, Reply::Records { records: vec!["the world for Bob".into()], wanted: big, digest: "d".into() }, "big asks come through after the proof");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_world_not_offered_stays_closed() {
    let host = gm();
    host.members.lock().unwrap().push("anyone".into());
    *host.offered.lock().unwrap() = false;
    let gm_net = WorldNet::start(host.clone(), aw_sync::world::Reach { lan: true, ..Default::default() }).await.unwrap();
    let player_net = WorldNet::start(Arc::new(Nobody), aw_sync::world::Reach { lan: true, ..Default::default() }).await.unwrap();
    let bob = Identity::create("Bob").unwrap();
    let reply = player_net.ask_at(gm_net.loopback_addr(), "world:eldermere", &bob, Ask::Join).await.unwrap();
    assert_eq!(reply, Reply::NotOffered);
    assert!(host.requests.lock().unwrap().is_empty(), "nothing reached the host");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_offered_world_is_found_on_the_network() {
    let host = gm();
    let _gm_net = WorldNet::start(host.clone(), aw_sync::world::Reach { lan: true, ..Default::default() }).await.unwrap();
    let player_net = WorldNet::start(Arc::new(Nobody), aw_sync::world::Reach { lan: true, ..Default::default() }).await.unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        while !player_net.found().iter().any(|o| o.world == "world:eldermere") {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the offer was not found via mDNS");
    let offer = player_net.found().into_iter().find(|o| o.world == "world:eldermere").unwrap();
    assert_eq!((offer.name.as_str(), offer.gm.as_str()), ("Eldermere", "Marla"));

    // And it can be asked through what was found.
    let bob = Identity::create("Bob").unwrap();
    let reply = player_net.ask("world:eldermere", &bob, Ask::Join).await.unwrap();
    assert_eq!(reply, Reply::Requested);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_host_that_is_gone_is_named_as_such() {
    let player_net = WorldNet::start(Arc::new(Nobody), aw_sync::world::Reach { lan: true, ..Default::default() }).await.unwrap();
    let gone = WorldNet::start(gm(), aw_sync::world::Reach { lan: true, ..Default::default() }).await.unwrap();
    let at = gone.loopback_addr();
    drop(gone);
    let bob = Identity::create("Bob").unwrap();
    // Usually no answer at all; a test running beside this one may have taken
    // the port meanwhile — then it is the wrong host. Either way: no reply.
    let error = player_net.ask_at(at, "world:eldermere", &bob, Ask::Join).await.unwrap_err().to_string();
    assert!(!error.is_empty());
}

/// Over the internet: no mDNS. The player knows the host only from the note
/// it left in the shared folder; the host connects towards the player, then
/// the player's own connection gets through — as with a knock on the LAN.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn met_through_the_folder_they_talk_without_the_local_network() {
    let host = gm();
    let bob = Identity::create("Bob").unwrap();
    host.members.lock().unwrap().push(bob.id().to_string());
    let reach = aw_sync::world::Reach::default();
    let gm_net = WorldNet::start(host.clone(), reach.clone()).await.unwrap();
    let player_net = WorldNet::start(Arc::new(Nobody), reach).await.unwrap();
    assert!(player_net.found().is_empty(), "nothing found without the local network");
    let offer = Offer { world: "world:eldermere".into(), name: "Eldermere".into(), icon: "🏰".into(), gm: "Marla".into() };
    let at: Vec<std::net::SocketAddr> = gm_net.loopback_addr().ip_addrs().copied().collect();
    player_net.add_remote(offer, &gm_net.id(), &at).unwrap();
    assert_eq!(player_net.found().len(), 1);
    let back: Vec<std::net::SocketAddr> = player_net.loopback_addr().ip_addrs().copied().collect();
    gm_net.punch_to(&player_net.id(), &back).unwrap();
    let reply = player_net.ask("world:eldermere", &bob, Ask::Digest { presence: None, rolls: Vec::new() }).await.unwrap();
    assert!(matches!(reply, Reply::Digest { .. }), "{reply:?}");
}


#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_at_one_address_answers_there_again_and_again() {
    // Only on a machine that has a network to name.
    let Some(ip) = aw_sync::world::own_ipv4() else { return };
    let host = gm();
    let bob = Identity::create("Bob").unwrap();
    host.members.lock().unwrap().push(bob.id().to_string());
    let server_net = WorldNet::start(host.clone(), aw_sync::world::Reach { only: Some(ip), ..Default::default() }).await.unwrap();
    let at = server_net.home_address().unwrap();
    assert_eq!(at.ip(), std::net::IpAddr::V4(ip));
    let server = aw_sync::world::Server { endpoint: server_net.id(), addrs: vec![at.to_string()] };
    let player_net = WorldNet::start(Arc::new(Nobody), aw_sync::world::Reach::default()).await.unwrap();
    for _ in 0..3 {
        let reply = player_net.ask_server(&server, "world:eldermere", &bob, Ask::Digest { presence: None, rolls: Vec::new() }).await.unwrap();
        assert!(matches!(reply, Reply::Digest { .. }), "{reply:?}");
    }
}
