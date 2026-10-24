//! Worlds on the local network: offered by their game master, found by
//! everyone else, joined on request.
//!
//! A game master's app announces the worlds it has explicitly shared — name,
//! icon and the game master's name, nothing of their content. Another app
//! finds them, connects over QUIC and asks to join — or, as a member, swaps
//! records: each side names what it has, and gets what it lacks. It proves
//! who it is by signing a fresh challenge with its identity; the host checks
//! that the identity's id belongs to its keys. Every record is signed on its
//! own; each side checks each one before it keeps it, no matter who passed
//! it on.
//!
//! Firewalls let out what a device sends and the answers to it, but nothing
//! that starts outside. So before connecting, the asking side knocks: it
//! announces over mDNS — which passes — where it is and whom it wants. The
//! host answers the knock by connecting towards it, which opens its firewall
//! for exactly that peer; the asking side's own attempt then gets through.
//! No port has to be opened by hand.
//!
//! Over the internet the same talk runs, once two devices know where the
//! other is: each one asks its router to open its port (UPnP, PCP, NAT-PMP),
//! and may ask a STUN server how it looks from outside. Where they are, the
//! application passes on through the group's shared folder (`add_remote`);
//! the host answers by connecting towards the one coming (`punch_to`), as on
//! the local network — both firewalls then let the talk through.
//!
//! A server — `aw-server`, on a NAS say — is one more side that holds
//! worlds and answers, always on: the same talk, at an address the group
//! is given as a code (`Server`). It holds no keys; it passes on what is
//! signed, to whom the world itself names as GM or member. Its admin, a GM
//! named when it starts, manages it (`Ask::Manage`).
//!
//! Separate from the text experiment in the crate root: its own service, its
//! own protocol. No relay, no address lookup.

use aw_crypto::{Identity, IdentityPublic};
use iroh::{
    endpoint::{presets, Connection, RecvStream, SendStream},
    Endpoint, EndpointAddr, EndpointId, RelayMode,
};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    net::{Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Semaphore,
    task::JoinHandle,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const ALPN: &[u8] = b"anotherworldbuilder/world/1";
const SERVICE: &str = "_awworld._udp.local.";
/// Someone about to connect: where they are and whom they want.
const KNOCK: &str = "_awknock._udp.local.";
/// Knocks answered at the same time — a knock costs the host a few packets.
const PUNCHES: usize = 4;
/// After talking to a host, its firewall stays open for a while — no knock
/// needed within this time.
const OPEN: Duration = Duration::from_secs(20);
/// A whole world travels in one answer.
const MAX_ANSWER: usize = 64 * 1024 * 1024;
const MAX_SMALL: usize = 16 * 1024;
/// How long a peer may take for each step.
const STEP: Duration = Duration::from_secs(10);

/// A world as it is announced — nothing of its content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offer {
    pub world: String,
    pub name: String,
    pub icon: String,
    /// The game master's display name.
    pub gm: String,
}

/// What the asking side wants.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Ask {
    /// To become a member.
    Join,
    /// Only a digest of what the host would give — asked often, answered
    /// cheaply. Brings along where the asking side is writing, if anywhere,
    /// and the dice it rolled lately for everyone to see.
    Digest {
        presence: Option<String>,
        #[serde(default)]
        rolls: Vec<String>,
    },
    /// Takes `give`, then answers with the records not in `have`.
    Swap { have: Vec<String>, give: Vec<String> },
    /// Takes records, and answers nothing back — a world brought over a
    /// little at a time, without naming everything held each time.
    Give { records: Vec<String> },
    /// An online command, staged in encrypted pieces. Only `finish` publishes
    /// the complete command, provided the world still has `base` as its digest.
    /// A lost connection before finish leaves the world unchanged.
    Commit {
        transaction: String,
        base: String,
        records: Vec<String>,
        parts: Vec<(String, String)>,
        finish: bool,
    },
    /// Discards an unfinished online command belonging to the asking member.
    Abort { transaction: String },
    /// Retires a receipt after the client has observed a successful commit.
    Release { transaction: String },
    /// The world itself, game master's part included — when the host hands
    /// it over to the asking side.
    Handover,
    /// Parts of images: takes `give` (name, base64), answers with those in
    /// `want` it has.
    Parts { want: Vec<String>, give: Vec<(String, String)> },
    /// The worlds a server holds — asked for no world in particular.
    Worlds,
    /// A command of the app, done by the host in the asking member's name —
    /// for a world that stays with the host. `args` as JSON.
    Call { command: String, args: String },
    /// What a server holds and who uses it — asked by its admin, for no
    /// world in particular; with an order, carried out first. A GM may
    /// order one thing too: their own world gone from it (`Order::Delete`).
    Manage { order: Option<Order> },
    /// To bring worlds to a server — the admin decides.
    HostWorlds,
    /// How a server backs this world up — asked by its GM or the server's
    /// admin; with a plan (`off`, `daily`, `weekly`; how many stay), set first.
    Backups { plan: Option<BackupPlan> },
}

/// How often a server backs a world up by itself, and how many backups stay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupPlan {
    pub every: String,
    pub keep: u32,
}

/// A world's backups on its server: the plan, and those there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerBackups {
    pub plan: BackupPlan,
    pub count: u32,
    /// Milliseconds since 1970 of the newest — 0: none yet.
    pub last: u64,
}

/// What a server's admin orders.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Order {
    /// A GM may bring worlds — one who asked, or anyone by id.
    Allow { id: String },
    /// May bring no more — or the request is turned down.
    Refuse { id: String },
    /// Nobody gets it any more, its GM neither; it stays as it is.
    Close { world: String },
    Open { world: String },
    /// A closed world, gone from the server — it stays closed.
    Delete { world: String },
    /// Turned away, whatever they ask.
    Block { id: String },
    Unblock { id: String },
}

/// A world on a server, as its admin sees it — nothing of its content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldWorld {
    pub world: String,
    pub name: String,
    pub gm: String,
    pub gm_id: String,
    /// On disk.
    pub bytes: u64,
    pub encrypted: bool,
    pub closed: bool,
}

/// Someone a server knows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub id: String,
    pub name: String,
    /// Last seen, seconds since 1970; 0: not yet.
    pub last: u64,
    pub blocked: bool,
}

/// What a server's admin sees.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerState {
    pub worlds: Vec<HeldWorld>,
    /// The GMs who may bring worlds, the admin first.
    pub gms: Vec<Person>,
    /// GMs who would bring a world.
    pub asking: Vec<Person>,
    /// Everyone who came.
    pub users: Vec<Person>,
}

/// What the host answers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Reply {
    /// Of everything the host would give, by name — where the others are
    /// writing right now, and whether the host hands the world over to the
    /// asking side.
    Digest {
        digest: String,
        presence: Vec<String>,
        /// The dice the others rolled lately.
        #[serde(default)]
        rolls: Vec<String>,
        handover: bool,
        /// Who asked a server to join — told the GM, who decides.
        #[serde(default)]
        requests: Vec<IdentityPublic>,
        /// A member keeps no copy: the world is theirs while they are online.
        #[serde(default)]
        no_copy: bool,
    },
    /// The world handed over, sealed for the asking side alone.
    Handover { package: String },
    /// The records the asking side lacks, exactly as signed; the names of
    /// those the host lacks; and the digest after this swap.
    Records { records: Vec<String>, wanted: Vec<String>, digest: String },
    /// Parts of images asked for (name, base64) — as many as fit — and the
    /// parts the host still lacks.
    Parts { parts: Vec<(String, String)>, want: Vec<String> },
    /// Not a member; one may ask to join.
    NotMember,
    /// The request waits for the game master — or, a world brought to a
    /// server, for its admin.
    Requested,
    /// This world is not offered (any more).
    NotOffered,
    /// The worlds a server holds — and who the server is: what it writes
    /// in a member's name it signs as itself. `admin`: the asking side is
    /// its admin; `may_bring`: it may bring worlds; `asked`: it asked to.
    Worlds {
        worlds: Vec<Offer>,
        #[serde(default)]
        host: Option<IdentityPublic>,
        #[serde(default)]
        admin: bool,
        #[serde(default)]
        may_bring: bool,
        #[serde(default)]
        asked: bool,
    },
    /// What a server holds and who uses it — for its admin.
    Managed { state: ServerState },
    /// How a server backs a world up.
    Backups { backups: ServerBackups },
    /// What a call came to: its answer as JSON, or why not.
    Called {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// This world stays with its host: a member works on it online, with
    /// `Call`, and gets no copy.
    OnlineOnly,
    /// What was given is taken; the digest after it.
    Taken { digest: String },
    /// The world is being written right now — ask again in a while.
    Busy,
    /// Encrypted pieces are staged, but have not changed the world yet.
    Staged,
    /// The complete online command is durable on the server.
    Committed { digest: String },
    /// Another command changed the world before this one was committed.
    Conflict,
}

/// The side that holds worlds — implemented by the application.
pub trait Host: Send + Sync + 'static {
    /// The worlds offered right now — announced on the local network.
    fn offers(&self) -> Vec<Offer>;
    /// Whether a world is answered for here — those offered, and those
    /// reachable from the internet without being announced.
    fn serves(&self, world: &str) -> bool {
        self.offers().iter().any(|o| o.world == world)
    }
    /// Answers someone whose identity is proven.
    fn answer(&self, world: &str, who: &IdentityPublic, ask: Ask) -> Reply;
}

/// Where a server answers: its endpoint and its addresses, as the code the
/// group is given — `<endpoint>@<host:port>[,<host:port>…]`. Hosts may be
/// names; they are looked up when asking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Server {
    pub endpoint: String,
    pub addrs: Vec<String>,
}

impl Server {
    pub fn parse(code: &str) -> Result<Self> {
        let (endpoint, addrs) = code.trim().split_once('@').ok_or("A server code reads <id>@<host:port>.")?;
        endpoint.parse::<EndpointId>().map_err(|_| "This is not a server's id.")?;
        let addrs: Vec<String> = addrs.split(',').map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect();
        if addrs.is_empty() || addrs.iter().any(|a| a.rsplit_once(':').is_none_or(|(_, p)| p.parse::<u16>().is_err())) {
            return Err("A server's address reads host:port.".into());
        }
        Ok(Server { endpoint: endpoint.to_string(), addrs })
    }

    pub fn code(&self) -> String {
        format!("{}@{}", self.endpoint, self.addrs.join(","))
    }

    /// A server by its address alone — `host:port`, or a host with the
    /// usual port: it tells its code over TCP on the same port (`tell_code`).
    /// The address given comes first in the code, before those the server
    /// names itself. A full code is taken as it is.
    pub async fn look_up(address: &str) -> Result<Self> {
        let address = address.trim();
        if address.contains('@') {
            return Self::parse(address);
        }
        let address = with_port(address);
        let ask = async {
            let mut stream = tokio::net::TcpStream::connect(address.as_str()).await?;
            stream.write_all(format!("GET / HTTP/1.0\r\nHost: {address}\r\n\r\n").as_bytes()).await?;
            let mut answer = Vec::new();
            (&mut stream).take(MAX_SMALL as u64).read_to_end(&mut answer).await?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(answer)
        };
        let answer: Vec<u8> = tokio::time::timeout(STEP, ask)
            .await
            .map_err(|_| format!("{address} does not answer."))?
            .map_err(|e| format!("{address} does not answer: {e}"))?;
        let text = String::from_utf8_lossy(&answer);
        let body = text.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or(&text);
        let told = Self::parse(body.trim()).map_err(|_| format!("{address} is no server of this app."))?;
        let mut addrs = vec![address.clone()];
        addrs.extend(told.addrs.into_iter().filter(|a| *a != address));
        Ok(Server { endpoint: told.endpoint, addrs })
    }
}

/// `host` → `host:7447`; one with a port stays as it is.
fn with_port(address: &str) -> String {
    let has_port = match address.rsplit_once(':') {
        Some((host, port)) => port.parse::<u16>().is_ok() && (!host.contains(':') || host.ends_with(']')),
        None => false,
    };
    if has_port {
        address.to_string()
    } else if address.contains(':') && !address.starts_with('[') {
        format!("[{address}]:{DEFAULT_PORT}")
    } else {
        format!("{address}:{DEFAULT_PORT}")
    }
}

/// Where a server answers unless told otherwise.
pub const DEFAULT_PORT: u16 = 7447;

/// Tells a server's code to whoever asks over TCP on `port` — as a tiny web
/// page, so a browser shows it too. Nothing else is said there: the code is
/// what the server gives anyone anyway.
pub async fn tell_code(port: u16, code: String) -> Result<()> {
    // Both IPv6 and IPv4 where the system allows; IPv4 alone otherwise.
    let listener = match tokio::net::TcpListener::bind((std::net::Ipv6Addr::UNSPECIFIED, port)).await {
        Ok(l) => l,
        Err(_) => tokio::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).await?,
    };
    let page = format!("HTTP/1.0 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{code}\n", code.len() + 1);
    loop {
        let Ok((mut stream, _)) = listener.accept().await else { continue };
        let page = page.clone();
        tokio::spawn(async move {
            // What was asked does not matter; a little of it is read so the
            // asking side sees its request taken.
            let mut buf = [0u8; 1024];
            let _ = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf)).await;
            let _ = tokio::time::timeout(STEP, stream.write_all(page.as_bytes())).await;
            let _ = stream.shutdown().await;
        });
    }
}


#[derive(Clone)]
struct Found {
    offer: Offer,
    addr: EndpointAddr,
    fullname: String,
    /// Met through the shared folder, not on the local network.
    remote: bool,
}

/// What the network does besides answering.
#[derive(Clone, Debug, Default)]
pub struct Reach {
    /// Announce and find worlds on the local network (mDNS).
    pub lan: bool,
    /// Be reachable from the internet: the router's port, and STUN.
    pub internet: bool,
    /// STUN servers to ask how this device looks from outside (`host:port`) —
    /// none: only the router is asked.
    pub stun: Vec<String>,
    /// The endpoint's secret key — a server's, so its id stays the same.
    /// None: a fresh one each start.
    pub secret: Option<[u8; 32]>,
    /// The port to answer on — for a code and a router's forwarding. None:
    /// any free one.
    pub port: Option<u16>,
    /// Without `port` free, not to start at all — a server's port is its
    /// address. Otherwise any free port does.
    pub port_required: bool,
    /// Answer only at this IPv4 address of the machine, and name no other.
    /// A NAS has several — a second network, a VPN, containers — and a
    /// device asked at one it cannot reach does not get through. None: all.
    pub only: Option<Ipv4Addr>,
}

pub struct WorldNet {
    endpoint: Endpoint,
    /// Only on the local network.
    mdns: Option<ServiceDaemon>,
    /// Keeps the router's port open while it lives.
    portmap: Option<portmapper::Client>,
    mapping: Option<JoinHandle<()>>,
    /// How this device looked from outside to a STUN server, before the
    /// endpoint took the port over.
    outside: Option<SocketAddr>,
    punches: Arc<Semaphore>,
    host: Arc<dyn Host>,
    found: Arc<Mutex<BTreeMap<String, Found>>>,
    /// Our own announcements, to take them back.
    announced: Mutex<Vec<String>>,
    knocks: AtomicU64,
    /// When each host last answered.
    contact: Mutex<HashMap<EndpointId, Instant>>,
    task: JoinHandle<()>,
}

impl Drop for WorldNet {
    fn drop(&mut self) {
        self.task.abort();
        if let Some(m) = &self.mapping {
            m.abort();
        }
        if let Some(mdns) = &self.mdns {
            if let Ok(names) = self.announced.lock() {
                for name in names.iter() {
                    let _ = mdns.unregister(name);
                }
            }
            let _ = mdns.shutdown();
        }
    }
}

impl WorldNet {
    /// Starts listening — and, as `reach` says, announcing and looking
    /// around on the local network, and opening up to the internet.
    pub async fn start(host: Arc<dyn Host>, reach: Reach) -> Result<Self> {
        // How the port looks from outside, asked before the endpoint takes
        // it: most routers keep the same outside port for it afterwards.
        let wanted = reach.port.unwrap_or(0);
        let (port, outside) = if reach.internet && !reach.stun.is_empty() {
            let servers = reach.stun.clone();
            tokio::task::spawn_blocking(move || stun_probe(&servers, wanted)).await.unwrap_or((wanted, None))
        } else {
            (wanted, None)
        };
        let secret = reach.secret;
        let builder = || {
            let b = Endpoint::builder(presets::Minimal)
                .relay_mode(RelayMode::Disabled)
                .clear_address_lookup()
                .clear_ip_transports()
                .alpns(vec![ALPN.to_vec()]);
            match secret {
                Some(bytes) => b.secret_key(iroh::SecretKey::from_bytes(&bytes)),
                None => b,
            }
        };
        // IPv6 too where the internet is wanted: there is no NAT there, and
        // most lines behind a carrier's NAT have it.
        let v6 = reach.internet;
        let only = reach.only;
        let bind = |port: u16, v6: bool| -> Result<_> {
            let b = match only {
                Some(ip) => builder().bind_addr_with_opts((ip, port), iroh::endpoint::BindOpts::default().set_is_default_route(true))?,
                None => builder().bind_addr((Ipv4Addr::UNSPECIFIED, port))?,
            };
            Ok(if v6 { b.bind_addr((std::net::Ipv6Addr::UNSPECIFIED, 0))? } else { b })
        };
        let endpoint = match bind(port, v6)?.bind().await {
            Ok(e) => e,
            // Without IPv6 here, or the port taken meanwhile: IPv4, any port.
            Err(_) => match bind(port, false)?.bind().await {
                Ok(e) => e,
                // A server's port is its address: taken, it does not start elsewhere.
                Err(e) if reach.port_required => return Err(format!("port {port}: {e}").into()),
                Err(_) => bind(0, false)?.bind().await?,
            },
        };
        let outside = outside.filter(|_| endpoint.bound_sockets().iter().any(|a| a.port() == port));
        let (portmap, mapping) = if reach.internet {
            let client = portmapper::Client::new(portmapper::Config::default());
            let local = endpoint.bound_sockets().iter().find(|a| a.is_ipv4()).and_then(|a| std::num::NonZeroU16::new(a.port()));
            // The router is found first — asked for a port before, it is not
            // asked at all. Then again every minute: a router forgets.
            let asking = client.clone();
            let mapping = tokio::spawn(async move {
                let _ = asking.probe().await;
                let Some(port) = local else { return };
                asking.update_local_port(port);
                loop {
                    asking.procure_mapping();
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }
            });
            (Some(client), Some(mapping))
        } else {
            (None, None)
        };
        let mdns = if reach.lan { Some(ServiceDaemon::new()?) } else { None };
        let events = match &mdns {
            Some(m) => Some(m.browse(SERVICE)?),
            None => None,
        };
        let knocks = match &mdns {
            Some(m) => Some(m.browse(KNOCK)?),
            None => None,
        };
        let found = Arc::new(Mutex::new(BTreeMap::<String, Found>::new()));

        let ep = endpoint.clone();
        let own = endpoint.id().to_string();
        let answering = host.clone();
        let seen = found.clone();
        let punches = Arc::new(Semaphore::new(PUNCHES));
        let punching = punches.clone();
        let task = tokio::spawn(async move {
            let slots = Arc::new(Semaphore::new(16));
            let punches = punching;
            let mut tick = tokio::time::interval(Duration::from_millis(250));
            loop {
                tokio::select! {
                    Some(incoming) = ep.accept() => {
                        let Ok(permit) = slots.clone().try_acquire_owned() else {
                            incoming.refuse();
                            continue;
                        };
                        let host = answering.clone();
                        let local = ep.id();
                        tokio::spawn(async move {
                            let _permit = permit;
                            if let Ok(Ok(conn)) = tokio::time::timeout(STEP, incoming).await {
                                let _ = serve(conn, local, host).await;
                            }
                        });
                    }
                    _ = tick.tick() => {
                        while let Some(Ok(event)) = knocks.as_ref().map(|k| k.try_recv()) {
                            let ServiceEvent::ServiceResolved(info) = event else { continue };
                            let prop = |k: &str| info.get_property_val_str(k).unwrap_or_default().to_string();
                            if prop("to") != own { continue; }
                            let Ok(id) = prop("from").parse::<EndpointId>() else { continue };
                            let Ok(permit) = punches.clone().try_acquire_owned() else { continue };
                            let mut addr = EndpointAddr::new(id);
                            for ip in info.get_addresses_v4() {
                                addr = addr.with_ip_addr(SocketAddr::from((ip, info.get_port())));
                            }
                            let (ep, host) = (ep.clone(), answering.clone());
                            tokio::spawn(async move {
                                let _permit = permit;
                                // Only a host that offers anything opens up at all.
                                let offers = tokio::task::spawn_blocking(move || !host.offers().is_empty()).await;
                                if offers.unwrap_or(false) {
                                    punch(&ep, addr).await;
                                }
                            });
                        }
                        while let Some(Ok(event)) = events.as_ref().map(|e| e.try_recv()) {
                            let Ok(mut map) = seen.lock() else { continue };
                            match event {
                                ServiceEvent::ServiceResolved(info) => {
                                    let prop = |k: &str| info.get_property_val_str(k).unwrap_or_default().to_string();
                                    let endpoint = prop("endpoint");
                                    // Our own announcement is no find.
                                    if endpoint.is_empty() || endpoint == own { continue; }
                                    let Ok(id) = endpoint.parse::<EndpointId>() else { continue };
                                    let mut addr = EndpointAddr::new(id);
                                    for ip in info.get_addresses_v4() {
                                        addr = addr.with_ip_addr(SocketAddr::from((ip, info.get_port())));
                                    }
                                    let offer = Offer { world: prop("world"), name: prop("name"), icon: prop("icon"), gm: prop("gm") };
                                    if offer.world.is_empty() { continue; }
                                    map.insert(offer.world.clone(), Found { offer, addr, fullname: info.get_fullname().to_string(), remote: false });
                                }
                                ServiceEvent::ServiceRemoved(_, fullname) => {
                                    map.retain(|_, f| f.fullname != fullname || f.remote);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        });

        let net = WorldNet {
            endpoint,
            mdns,
            portmap,
            mapping,
            outside,
            punches,
            host,
            found,
            announced: Mutex::new(Vec::new()),
            knocks: AtomicU64::new(0),
            contact: Mutex::new(HashMap::new()),
            task,
        };
        net.publish()?;
        Ok(net)
    }

    /// Announces what is offered now — and takes back what no longer is.
    pub fn publish(&self) -> Result<()> {
        let Some(mdns) = &self.mdns else { return Ok(()) };
        let mut announced = self.announced.lock().map_err(|_| "announcements poisoned")?;
        for name in announced.drain(..) {
            let _ = mdns.unregister(&name);
        }
        let id = self.endpoint.id().to_string();
        let port = self
            .endpoint
            .bound_sockets()
            .iter()
            .find(|a| a.is_ipv4())
            .ok_or("missing IPv4 socket")?
            .port();
        let host = format!("aw-{}.local.", &id[..16]);
        for offer in self.host.offers() {
            // A TXT entry holds 255 bytes; a name is cut rather than refused.
            let name: String = offer.name.chars().take(80).collect();
            let gm: String = offer.gm.chars().take(60).collect();
            let props = [
                ("world", offer.world.as_str()),
                ("name", name.as_str()),
                ("icon", offer.icon.as_str()),
                ("gm", gm.as_str()),
                ("endpoint", id.as_str()),
            ];
            let instance = format!("{}-{}", &id[..8], short(&offer.world));
            let info = ServiceInfo::new(SERVICE, &instance, &host, "", port, &props[..])?.enable_addr_auto();
            announced.push(info.get_fullname().to_string());
            mdns.register(info)?;
        }
        Ok(())
    }

    /// The worlds others offer on this network right now.
    pub fn found(&self) -> Vec<Offer> {
        self.found
            .lock()
            .map(|m| m.values().map(|f| f.offer.clone()).collect())
            .unwrap_or_default()
    }

    /// Asks the host of a found world.
    pub async fn ask(&self, world: &str, me: &Identity, ask: Ask) -> Result<Reply> {
        let addr = self
            .found
            .lock()
            .map_err(|_| "finds poisoned")?
            .get(world)
            .map(|f| f.addr.clone())
            .ok_or("This world is not on the network right now.")?;
        let host = addr.id;
        let recent = self
            .contact
            .lock()
            .ok()
            .and_then(|c| c.get(&host).copied())
            .is_some_and(|at| at.elapsed() < OPEN);
        // Knock first, so the host's firewall is open by the time we arrive.
        let knock = if recent { None } else { self.knock(host).ok() };
        let reply = self.ask_at(addr, world, me, ask).await;
        if let (Some(name), Some(mdns)) = (knock, &self.mdns) {
            let _ = mdns.unregister(&name);
        }
        if let Ok(mut contact) = self.contact.lock() {
            if reply.is_ok() {
                contact.insert(host, Instant::now());
            } else {
                contact.remove(&host);
            }
        }
        reply
    }

    /// Announces over mDNS that this side is about to connect to `to`.
    fn knock(&self, to: EndpointId) -> Result<String> {
        let mdns = self.mdns.as_ref().ok_or("not on the local network")?;
        let id = self.endpoint.id().to_string();
        let n = self.knocks.fetch_add(1, Ordering::Relaxed);
        let host = format!("aw-{}.local.", &id[..16]);
        let to = to.to_string();
        let props = [("from", id.as_str()), ("to", to.as_str())];
        let instance = format!("{}-{n}", &id[..12]);
        let info = ServiceInfo::new(KNOCK, &instance, &host, "", self.port(), &props[..])?.enable_addr_auto();
        let name = info.get_fullname().to_string();
        mdns.register(info)?;
        Ok(name)
    }

    /// The UDP port this side answers on.
    fn port(&self) -> u16 {
        self.endpoint.bound_sockets().iter().find(|a| a.is_ipv4()).map(|a| a.port()).unwrap_or(0)
    }

    /// Asks a host at a known address — the way `ask` does after finding it.
    pub async fn ask_at(&self, addr: EndpointAddr, world: &str, me: &Identity, ask: Ask) -> Result<Reply> {
        let conn = tokio::time::timeout(STEP, self.endpoint.connect(addr, ALPN))
            .await
            .map_err(|_| "The game master's device does not answer.")??;
        let remote = conn.remote_id();
        let (mut send, mut recv) = tokio::time::timeout(STEP, conn.open_bi()).await??;
        let hello = Hello { world: world.to_string(), identity: me.public() };
        let challenge: Challenge = timed(exchange(&mut send, &mut recv, &hello, MAX_SMALL)).await?;
        let proof = Proof {
            signature: hex::encode(me.sign(&proof_bytes(world, self.endpoint.id(), remote, &challenge.nonce)?)),
        };
        write(&mut send, &proof).await?;
        // What is asked comes only after the proof — nobody unknown gets to
        // send more than a few bytes.
        write(&mut send, &ask).await?;
        let _ = send.finish();
        let reply: Reply = tokio::time::timeout(Duration::from_secs(120), read(&mut recv, MAX_ANSWER)).await??;
        conn.close(0u32.into(), b"done");
        Ok(reply)
    }

    /// Asks a server, at the addresses its code names.
    pub async fn ask_server(&self, server: &Server, world: &str, me: &Identity, ask: Ask) -> Result<Reply> {
        let mut addr = EndpointAddr::new(server.endpoint.parse::<EndpointId>()?);
        for a in &server.addrs {
            if let Ok(found) = tokio::net::lookup_host(a.as_str()).await {
                for a in found {
                    addr = addr.with_ip_addr(a);
                }
            }
        }
        self.ask_at(addr, world, me, ask).await.map_err(|e| format!("The server does not answer: {e}").into())
    }

    /// This device's endpoint, as others name it.
    pub fn id(&self) -> String {
        self.endpoint.id().to_string()
    }

    /// Where this device may be reached from the internet.
    pub fn reachable(&self) -> Vec<SocketAddr> {
        let mut out: Vec<SocketAddr> = Vec::new();
        for (_, a) in self.reach_by() {
            if !out.contains(&a) {
                out.push(a);
            }
        }
        out
    }

    /// The same, with how each was found: `router` — the port the router
    /// opened; `stun` — how a STUN server saw it, and with the same port as
    /// here too, which most routers keep; `ipv6` — this device's own
    /// address, with no NAT in between.
    pub fn reach_by(&self) -> Vec<(&'static str, SocketAddr)> {
        let mut out = Vec::new();
        if let Some(mapped) = self.portmap.as_ref().and_then(|p| *p.watch_external_address().borrow()) {
            out.push(("router", SocketAddr::V4(mapped)));
        }
        if let Some(seen) = self.outside {
            out.push(("stun", seen));
            if seen.port() != self.port() {
                out.push(("stun", SocketAddr::new(seen.ip(), self.port())));
            }
        }
        if self.portmap.is_some() {
            if let (Some(ip), Some(port)) = (own_ipv6(), self.endpoint.bound_sockets().iter().find(|a| a.is_ipv6()).map(|a| a.port())) {
                out.push(("ipv6", SocketAddr::new(ip.into(), port)));
            }
        }
        out
    }

    /// Where this device answers in its own network: its address there and
    /// the endpoint's port. For a note in a shared folder — members in the
    /// same network reach each other by it, also where the router does not
    /// lead its public address back inside (most home routers do not).
    pub fn home_address(&self) -> Option<SocketAddr> {
        let ip = own_ipv4()?;
        (self.port() != 0).then(|| SocketAddr::new(ip.into(), self.port()))
    }

    /// The endpoint asked for a world, if it is known.
    pub fn host_of(&self, world: &str) -> Option<String> {
        self.found.lock().ok()?.get(world).map(|f| f.addr.id.to_string())
    }

    /// Forgets a world met through the shared folder — none of its devices answers.
    pub fn drop_remote(&self, world: &str) {
        if let Ok(mut map) = self.found.lock() {
            if map.get(world).is_some_and(|f| f.remote) {
                map.remove(world);
            }
        }
    }

    /// A world held by a device met through the shared folder — asked like
    /// one found on the local network, at the addresses it left there.
    pub fn add_remote(&self, offer: Offer, endpoint: &str, addrs: &[SocketAddr]) -> Result<()> {
        let id: EndpointId = endpoint.parse()?;
        let mut map = self.found.lock().map_err(|_| "finds poisoned")?;
        // Found on the local network too: that stays, the outside addresses join it.
        let mut addr = match map.get(&offer.world) {
            Some(f) if f.addr.id == id => f.addr.clone(),
            _ => EndpointAddr::new(id),
        };
        for a in addrs {
            addr = addr.with_ip_addr(*a);
        }
        let remote = map.get(&offer.world).is_none_or(|f| f.remote);
        let fullname = map.get(&offer.world).map(|f| f.fullname.clone()).unwrap_or_else(|| format!("remote:{endpoint}"));
        map.insert(offer.world.clone(), Found { offer, addr, fullname, remote });
        Ok(())
    }

    /// Opens the way for someone about to connect from outside: connects
    /// towards them, which lets their packets through the firewall here.
    pub fn punch_to(&self, endpoint: &str, addrs: &[SocketAddr]) -> Result<()> {
        let id: EndpointId = endpoint.parse()?;
        if addrs.is_empty() {
            return Ok(());
        }
        let Ok(permit) = self.punches.clone().try_acquire_owned() else { return Ok(()) };
        let mut to = EndpointAddr::new(id);
        for a in addrs {
            to = to.with_ip_addr(*a);
        }
        let ep = self.endpoint.clone();
        tokio::spawn(async move {
            let _permit = permit;
            punch(&ep, to).await;
        });
        Ok(())
    }

    /// Where this side can be reached on this very machine — for tests.
    pub fn loopback_addr(&self) -> EndpointAddr {
        EndpointAddr::new(self.endpoint.id()).with_ip_addr(SocketAddr::from((Ipv4Addr::LOCALHOST, self.port())))
    }
}

/// Answers a knock: connects towards the one knocking. What counts are the
/// packets sent — they let the firewall here take that peer's packets as
/// answers. Whatever connection comes of it is closed right away; the asking
/// side's own connection carries the talk.
async fn punch(ep: &Endpoint, to: EndpointAddr) {
    if let Ok(Ok(conn)) = tokio::time::timeout(STEP, ep.connect(to, ALPN)).await {
        conn.close(0u32.into(), b"punched");
    }
}

/// This device's global IPv6 address, as it would send from it — the system
/// picks it for a public target; nothing is sent. Not a local or link one.
fn own_ipv6() -> Option<std::net::Ipv6Addr> {
    let socket = std::net::UdpSocket::bind((std::net::Ipv6Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect("[2001:4860:4860::8888]:53").ok()?;
    let SocketAddr::V6(addr) = socket.local_addr().ok()? else { return None };
    let ip = *addr.ip();
    // Global unicast: 2000::/3.
    ((ip.segments()[0] & 0xe000) == 0x2000).then_some(ip)
}

/// This device's IPv4 address in its own network, as it would send from it —
/// nothing is sent. Not a loopback one.
/// The address this machine reaches others from — on a NAS, its home network's.
pub fn own_ipv4() -> Option<Ipv4Addr> {
    let socket = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let SocketAddr::V4(addr) = socket.local_addr().ok()? else { return None };
    let ip = *addr.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

/// Asks STUN servers (RFC 5389) how a fresh port looks from outside — the
/// first that answers. Returns the port asked from, for the endpoint to take.
fn stun_probe(servers: &[String], wanted: u16) -> (u16, Option<SocketAddr>) {
    // The port asked for, where it is free; otherwise any.
    let socket = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, wanted)).or_else(|_| std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)));
    let Ok(socket) = socket else { return (0, None) };
    let port = socket.local_addr().map(|a| a.port()).unwrap_or(0);
    let _ = socket.set_read_timeout(Some(Duration::from_millis(1500)));
    for server in servers {
        let Some(to) = std::net::ToSocketAddrs::to_socket_addrs(server.as_str()).ok().and_then(|mut a| a.find(|a| a.is_ipv4())) else { continue };
        let mut id = [0u8; 12];
        if getrandom::fill(&mut id).is_err() {
            continue;
        }
        let mut ask = vec![0x00, 0x01, 0x00, 0x00, 0x21, 0x12, 0xA4, 0x42];
        ask.extend_from_slice(&id);
        if socket.send_to(&ask, to).is_err() {
            continue;
        }
        let mut buf = [0u8; 512];
        while let Ok((n, from)) = socket.recv_from(&mut buf) {
            if from == to {
                if let Some(seen) = stun_answer(&buf[..n], &id) {
                    return (port, Some(seen));
                }
            }
        }
    }
    (port, None)
}

/// The address a STUN binding answer names: XOR-MAPPED-ADDRESS, or the old
/// MAPPED-ADDRESS. IPv4 only.
fn stun_answer(msg: &[u8], id: &[u8; 12]) -> Option<SocketAddr> {
    if msg.len() < 20 || msg[0..2] != [0x01, 0x01] || msg[4..8] != [0x21, 0x12, 0xA4, 0x42] || &msg[8..20] != id {
        return None;
    }
    let len = u16::from_be_bytes([msg[2], msg[3]]) as usize;
    let attrs = msg.get(20..20 + len)?;
    let mut at = 0;
    let mut plain = None;
    while at + 4 <= attrs.len() {
        let kind = u16::from_be_bytes([attrs[at], attrs[at + 1]]);
        let size = u16::from_be_bytes([attrs[at + 2], attrs[at + 3]]) as usize;
        let value = attrs.get(at + 4..at + 4 + size)?;
        if value.len() >= 8 && value[1] == 0x01 {
            let port = u16::from_be_bytes([value[2], value[3]]);
            let ip = [value[4], value[5], value[6], value[7]];
            match kind {
                0x0020 => {
                    let port = port ^ 0x2112;
                    let ip = [ip[0] ^ 0x21, ip[1] ^ 0x12, ip[2] ^ 0xA4, ip[3] ^ 0x42];
                    return Some(SocketAddr::from((ip, port)));
                }
                0x0001 => plain = Some(SocketAddr::from((ip, port))),
                _ => {}
            }
        }
        at += 4 + size.div_ceil(4) * 4;
    }
    plain
}

fn short(world: &str) -> &str {
    let body = world.split_once(':').map(|(_, b)| b).unwrap_or(world);
    &body[..body.len().min(12)]
}

#[derive(Serialize, Deserialize)]
struct Hello {
    world: String,
    identity: IdentityPublic,
}

#[derive(Serialize, Deserialize)]
struct Challenge {
    nonce: String,
}

#[derive(Serialize, Deserialize)]
struct Proof {
    signature: String,
}

/// What the asking side signs: the protocol, the world, both ends of this
/// very connection and the host's fresh challenge — so a proof cannot be
/// replayed elsewhere.
fn proof_bytes(world: &str, asking: EndpointId, host: EndpointId, nonce: &str) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&(ALPN, world, asking.to_string(), host.to_string(), nonce))?)
}

/// The host's side of one connection.
async fn serve(conn: Connection, local: EndpointId, host: Arc<dyn Host>) -> Result<()> {
    let remote = conn.remote_id();
    let (mut send, mut recv) = tokio::time::timeout(STEP, conn.accept_bi()).await??;
    let hello: Hello = timed(read(&mut recv, MAX_SMALL)).await?;
    let nonce = hex::encode(aw_crypto::random_bytes::<32>()?);
    write(&mut send, &Challenge { nonce: nonce.clone() }).await?;
    let proof: Proof = timed(read(&mut recv, MAX_SMALL)).await?;
    let signature: [u8; 64] = hex::decode(&proof.signature)?.try_into().map_err(|_| "invalid proof")?;
    // The id has to belong to the keys, and the keys have to have signed.
    if !hello.identity.is_genuine() {
        return Err("identity does not match its keys".into());
    }
    hello
        .identity
        .verify(&proof_bytes(&hello.world, remote, local, &nonce)?, &signature)?;
    let offered = {
        let (host, world) = (host.clone(), hello.world.clone());
        tokio::task::spawn_blocking(move || host.serves(&world)).await.unwrap_or(false)
    };
    let reply = if offered {
        let ask: Ask = tokio::time::timeout(Duration::from_secs(120), read(&mut recv, MAX_ANSWER)).await??;
        let (world, who) = (hello.world.clone(), hello.identity.clone());
        // Answering reads, checks and writes records — off the network threads.
        tokio::task::spawn_blocking(move || host.answer(&world, &who, ask)).await?
    } else {
        Reply::NotOffered
    };
    write(&mut send, &reply).await?;
    let _ = send.finish();
    // Wait for the other side to have read everything before closing.
    let _ = tokio::time::timeout(Duration::from_secs(120), conn.closed()).await;
    Ok(())
}

async fn timed<T>(f: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(STEP, f).await?
}

async fn write<T: Serialize>(send: &mut SendStream, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_ANSWER {
        return Err("frame too large".into());
    }
    send.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
    send.write_all(&bytes).await?;
    Ok(())
}

async fn read<T: DeserializeOwned>(recv: &mut RecvStream, max: usize) -> Result<T> {
    let mut prefix = [0; 4];
    recv.read_exact(&mut prefix).await?;
    let n = u32::from_be_bytes(prefix) as usize;
    if n > max {
        return Err("frame too large".into());
    }
    let mut bytes = vec![0; n];
    recv.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

async fn exchange<T: Serialize, U: DeserializeOwned>(
    send: &mut SendStream,
    recv: &mut RecvStream,
    value: &T,
    max: usize,
) -> Result<U> {
    write(send, value).await?;
    read(recv, max).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_without_a_port_gets_the_usual_one() {
        assert_eq!(with_port("worlds.example.org"), "worlds.example.org:7447");
        assert_eq!(with_port("worlds.example.org:4001"), "worlds.example.org:4001");
        assert_eq!(with_port("192.168.0.20:7447"), "192.168.0.20:7447");
        assert_eq!(with_port("[2001:db8::1]:4001"), "[2001:db8::1]:4001");
        assert_eq!(with_port("2001:db8::1"), "[2001:db8::1]:7447");
    }

    #[tokio::test]
    async fn a_server_tells_its_code_to_whoever_comes_by_its_address() {
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let endpoint = "914c606f158a7b061b3f753c44182414ab23147d6136af7ac52d4ee4cb0ae27c".to_string();
        let code = format!("{endpoint}@10.0.0.2:{port}");
        tokio::spawn(tell_code(port, code));
        tokio::time::sleep(Duration::from_millis(200)).await;
        let server = Server::look_up(&format!("127.0.0.1:{port}")).await.unwrap();
        assert_eq!(server.endpoint, endpoint);
        assert_eq!(server.addrs, vec![format!("127.0.0.1:{port}"), format!("10.0.0.2:{port}")], "the address given first");
        assert!(Server::look_up("127.0.0.1:1").await.is_err(), "nobody there");
    }

    #[test]
    fn a_stun_answer_names_the_address_seen_from_outside() {
        let id = [7u8; 12];
        // 1.2.3.4:5678, XOR'd with the magic cookie.
        let port = 5678u16 ^ 0x2112;
        let ip = [1 ^ 0x21, 2 ^ 0x12, 3 ^ 0xA4, 4 ^ 0x42];
        let mut msg = vec![0x01, 0x01, 0x00, 0x0C, 0x21, 0x12, 0xA4, 0x42];
        msg.extend_from_slice(&id);
        msg.extend_from_slice(&[0x00, 0x20, 0x00, 0x08, 0x00, 0x01]);
        msg.extend_from_slice(&port.to_be_bytes());
        msg.extend_from_slice(&ip);
        assert_eq!(stun_answer(&msg, &id), Some(SocketAddr::from(([1, 2, 3, 4], 5678))));
        assert_eq!(stun_answer(&msg, &[8u8; 12]), None, "an answer to another question");
    }
}
