//! Real host-to-Android join test, using the production QUIC client and world
//! format. `prepare <test-dir>`, then
//! `join <test-dir> <gm-address> [player-address]`.
//! Only generated test profiles are used; no existing user profile is read.
use aw_crypto::{Identity, IdentityPublic, Id};
use aw_store::db::{Access, Document, Layer, Member, Role, WorldDb};
use aw_sync::world::{Ask, Host, Offer, Reply, Server, WorldNet};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::{path::{Path, PathBuf}, sync::Arc};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const NAME: &str = "Android-VM-Testwelt";
const PUBLIC: &str = "Diese Welt liegt auf dem Android-Server. Willkommen am Hafen!";
const SECRET: &str = "Nur Spielleitung: Unter dem Hafen liegt ein schlafender Drache.";

struct Nobody;
impl Host for Nobody {
    fn offers(&self) -> Vec<Offer> { Vec::new() }
    fn answer(&self, _: &str, _: &IdentityPublic, _: Ask) -> Reply { Reply::NotOffered }
}

fn identity(dir: &Path, name: &str) -> Result<Identity> {
    let path = dir.join("profile/identity.json");
    if path.exists() { return Ok(Identity::load(&path)?); }
    let me = Identity::create(name)?;
    me.save(&path)?;
    Ok(me)
}

fn prepare(root: &Path) -> Result<Value> {
    let marker = root.join("android-join-test.json");
    if marker.exists() { return Ok(serde_json::from_slice(&std::fs::read(marker)?)?); }
    if root.exists() && root.read_dir()?.next().is_some() {
        return Err("Use a new empty directory for generated test profiles.".into());
    }
    let gm = identity(&root.join("gm"), "Android-VM-Spielleitung")?;
    let player = identity(&root.join("player"), "Android-VM-Spieler")?;
    for person in ["gm", "player"] {
        let dir = root.join(person);
        std::fs::create_dir_all(dir.join("worlds"))?;
        std::fs::write(dir.join("locations.json"), serde_json::to_vec_pretty(&json!({
            "profile": dir.join("profile"), "worlds": dir.join("worlds"), "additional": []
        }))?)?;
    }
    let world = Id::derive("world", &[&aw_crypto::random_bytes::<32>()?]).to_string();
    let article = Id::derive("art", &[&aw_crypto::random_bytes::<32>()?]).to_string();
    let member = Member { id: gm.id().to_string(), name: gm.name().into(), role: Role::Gm,
        verify_key: gm.public().verify_key, ..Default::default() };
    let db = WorldDb::create(&root.join("fixture"), &world, NAME, &member)?;
    db.set("listed", "true")?;
    db.set("icon", "🌍")?;
    db.set("home", &article)?;
    let layer = |id: &str, base: bool, title: &str, text: &str, view: &str, edit: &str| Layer {
        id: id.into(), base, title: title.into(), text: text.into(), section: String::new(),
        access: Access { view: view.into(), edit: edit.into() }, fields: Default::default(), images: vec![],
    };
    db.put_document(&Document {
        id: article.clone(), doc_type: String::new(), slug: "android-hafen".into(),
        layers: vec![layer("base", true, "Ankunft am Android-Hafen", "", "all", "gm"),
            layer("public", false, "Willkommen", PUBLIC, "all", "players OR gm"),
            layer("secret", false, "Geheimnis", SECRET, "gm", "gm")],
        created: 1, changed: 1, created_by: gm.id().to_string(), changed_by: gm.id().to_string(), proposed: false,
    })?;
    db.copy_to(&root.join("world.sqlite"))?;
    let info = json!({"world": world, "article": article, "gm": gm.id().to_string(), "player": player.id().to_string()});
    std::fs::write(marker, serde_json::to_vec_pretty(&info)?)?;
    Ok(info)
}

async fn call(net: &WorldNet, server: &Server, world: &str, who: &Identity, command: &str, args: Value) -> Result<Value> {
    match net.ask_server(server, world, who, Ask::Call { command: command.into(), args: args.to_string() }).await? {
        Reply::Called { value: Some(raw), error: None } => Ok(serde_json::from_str(&raw)?),
        other => Err(format!("{command}: {other:?}").into()),
    }
}

async fn join(root: &Path, address: &str, player_address: &str) -> Result<()> {
    let info: Value = serde_json::from_slice(&std::fs::read(root.join("android-join-test.json"))?)?;
    let world = info["world"].as_str().ok_or("test world missing")?;
    let gm = Identity::load(&root.join("gm/profile/identity.json"))?;
    let player = Identity::load(&root.join("player/profile/identity.json"))?;
    let mut server = Server::look_up(address).await?;
    // A VM test must use its explicit forwarding, never an advertised WAN
    // candidate that might route to another host on the same home network.
    server.addrs = vec![address.to_string()];
    let mut player_server = server.clone();
    player_server.addrs = vec![player_address.to_string()];
    let gm_net = WorldNet::start(Arc::new(Nobody), Default::default()).await?;
    let player_net = WorldNet::start(Arc::new(Nobody), Default::default()).await?;
    println!("Connecting over QUIC to Android: {}", server.code());
    let Reply::Worlds { worlds, admin: true, .. } = gm_net.ask_server(&server, "", &gm, Ask::Worlds).await? else {
        return Err("The configured Android admin must be the generated GM id.".into());
    };
    if !worlds.iter().any(|w| w.world == world) {
        assert_eq!(gm_net.ask_server(&server, "", &gm, Ask::MayIBring).await?, Reply::Joined);
        let bytes = std::fs::read(root.join("world.sqlite"))?;
        for (index, chunk) in bytes.chunks(256 * 1024).enumerate() {
            assert_eq!(gm_net.ask_server(&server, world, &gm, Ask::Put {
                file: "world.sqlite".into(), at: (index * 256 * 1024) as u64, data: STANDARD.encode(chunk),
            }).await?, Reply::Taken);
        }
        assert_eq!(gm_net.ask_server(&server, world, &gm, Ask::Bring { name: NAME.into() }).await?, Reply::Joined);
        println!("PASS: real SQLite world uploaded to Android");
    }
    assert!(call(&gm_net, &server, world, &gm, "world_connect", json!({})).await?["is_gm"].as_bool() == Some(true));
    println!("PASS: GM opens the world on Android; connecting player via {player_address}");
    match player_net.ask_server(&player_server, world, &player, Ask::Join).await? {
        Reply::Requested => {
            println!("PASS: player requests admission and waits for the GM");
            let requests = call(&gm_net, &server, world, &gm, "join_requests", json!({})).await?;
            assert!(requests.as_array().ok_or("no request list")?.iter().any(|p| p["id"] == player.id().to_string()));
            call(&gm_net, &server, world, &gm, "join_answer", json!({"id": player.id().to_string(), "accept": true})).await?;
            assert_eq!(player_net.ask_server(&player_server, world, &player, Ask::Join).await?, Reply::Joined);
        }
        Reply::Joined => println!("Player is already admitted; checking the existing membership"),
        other => return Err(format!("join: {other:?}").into()),
    }
    let connected = call(&player_net, &player_server, world, &player, "world_connect", json!({})).await?;
    assert_eq!(connected["role"], "player");
    let seen = call(&player_net, &player_server, world, &player, "view", json!({})).await?;
    assert!(seen.to_string().contains(PUBLIC));
    assert!(!seen.to_string().contains(SECRET));
    assert!(call(&gm_net, &server, world, &gm, "view", json!({})).await?.to_string().contains(SECRET));
    println!("PASS: admitted player reads the world; GM-only text stays hidden");
    let forbidden = player_net.ask_server(&player_server, world, &player, Ask::Call {
        command: "world_icon_set".into(), args: json!({"icon": "X"}).to_string(),
    }).await?;
    assert!(matches!(forbidden, Reply::Called { error: Some(_), .. }));
    call(&player_net, &player_server, world, &player, "member_colour_set", json!({"colour": "#287a4f"})).await?;
    let members = call(&gm_net, &server, world, &gm, "world_connect", json!({})).await?;
    assert!(members["members"].as_array().ok_or("no members")?.iter().any(|m| m["id"] == player.id().to_string()));
    assert_eq!(members["colours"][format!("player:{}", player.name().trim().to_lowercase())], "#287a4f");
    println!("PASS: player write reaches Android and is read by the GM; forbidden admin write is denied");
    let stranger = Identity::create("Nichtmitglied")?;
    assert_eq!(player_net.ask_server(&player_server, world, &stranger, Ask::Call { command: "view".into(), args: "{}".into() }).await?, Reply::NotMember);
    for (person, port) in [("gm", 19447), ("player", 19448)] {
        let code = if person == "gm" { server.code() } else { player_server.code() };
        std::fs::write(root.join(person).join("profile/network.json"), serde_json::to_vec_pretty(&json!({
            "enabled": false, "internet": false, "port": port, "stun": [], "servers": [code],
            "remote": {world: {"name": NAME, "icon": "🌍", "server": code, "asked": false}}
        }))?)?;
    }
    std::fs::write(root.join("connection.json"), serde_json::to_vec_pretty(&json!({
        "server": server.code(), "player_server": player_server.code(), "world": world, "admin": gm.id().to_string(), "player": player.id().to_string(),
        "transport": "real QUIC/UDP through emulator port redirection", "result": "PASS"
    }))?)?;
    println!("PASS: nonmember denied; isolated desktop profiles prepared in {}", root.display());
    player_net.shutdown().await;
    gm_net.shutdown().await;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let action = args.next().ok_or("prepare|join <test-dir> [address]")?;
    let root = PathBuf::from(args.next().ok_or("test directory required")?);
    let root = if root.is_absolute() { root } else { std::env::current_dir()?.join(root) };
    match action.as_str() {
        "prepare" => println!("{}", serde_json::to_string_pretty(&prepare(&root)?)?),
        "join" => {
            let address = args.next().ok_or("server address required")?;
            let player_address = args.next().unwrap_or_else(|| address.clone());
            join(&root, &address, &player_address).await?;
        }
        "approve" => {
            let address = args.next().ok_or("server address required")?;
            let id = args.next().ok_or("requesting member id required")?;
            let info: Value = serde_json::from_slice(&std::fs::read(root.join("android-join-test.json"))?)?;
            let world = info["world"].as_str().ok_or("test world missing")?;
            let gm = Identity::load(&root.join("gm/profile/identity.json"))?;
            let mut server = Server::look_up(&address).await?; server.addrs = vec![address];
            let net = WorldNet::start(Arc::new(Nobody), Default::default()).await?;
            call(&net, &server, world, &gm, "join_answer", json!({"id": id, "accept": true})).await?;
            net.shutdown().await;
            println!("PASS: mobile test client admitted by the test world's GM");
        }
        _ => return Err("prepare|join <test-dir> [address]".into()),
    }
    Ok(())
}
