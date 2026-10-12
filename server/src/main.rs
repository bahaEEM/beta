// SPDX-License-Identifier: AGPL-3.0-or-later
//! server [--port 7777] [--map hasandag] [--resume <match_id>] [--data data] [--maps maps] [--saves saves]

use server::setup::{load_data, load_game, load_latest, load_map};
use server::{serve, BoxError, MatchSetup};
use std::path::PathBuf;
use tokio::net::TcpListener;
use tracing::{error, info};

struct Args {
    port: u16,
    map: String,
    resume: Option<String>,
    data: PathBuf,
    maps: PathBuf,
    saves: PathBuf,
}

fn parse_args() -> Result<Args, BoxError> {
    let mut a = Args {
        port: 7777,
        map: "hasandag".into(),
        resume: None,
        data: "data".into(),
        maps: "maps".into(),
        saves: "saves".into(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("missing value for {flag}"));
        match flag.as_str() {
            "--port" => a.port = value()?.parse()?,
            "--map" => a.map = value()?,
            "--resume" => a.resume = Some(value()?),
            "--data" => a.data = value()?.into(),
            "--maps" => a.maps = value()?.into(),
            "--saves" => a.saves = value()?.into(),
            _ => return Err(format!("unknown flag '{flag}'").into()),
        }
    }
    Ok(a)
}

fn setup(a: &Args) -> Result<MatchSetup, BoxError> {
    let (catalog, rules) = load_data(&a.data)?;
    if let Some(match_id) = &a.resume {
        let mut save = load_latest(&a.saves, match_id)?;
        save.game.world.map = load_map(&a.maps.join(&save.map_id))?;
        info!(%match_id, turn = save.game.turn, "resuming");
        return Ok(MatchSetup {
            match_id: save.match_id,
            map_id: save.map_id,
            saves_dir: a.saves.clone(),
            catalog,
            rules,
            game: save.game,
            players: save.players,
        });
    }
    let map = load_map(&a.maps.join(&a.map))?;
    let scenario =
        std::fs::read_to_string(a.data.join("scenarios").join(format!("{}.toml", a.map)))?;
    let game = load_game(&scenario, map, &catalog, &rules)?;
    // The server (not the simulation) may read the clock; used only to name the match.
    #[allow(clippy::disallowed_methods)]
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ok(MatchSetup {
        match_id: format!("m{secs}"),
        map_id: a.map.clone(),
        saves_dir: a.saves.clone(),
        catalog,
        rules,
        game,
        players: [None, None],
    })
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    if let Err(e) = run().await {
        error!("{e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), BoxError> {
    let args = parse_args()?;
    let setup = setup(&args)?;
    info!(match_id = %setup.match_id, "resume later with: --resume {}", setup.match_id);
    let listener = TcpListener::bind(("0.0.0.0", args.port)).await?;
    info!("listening on 0.0.0.0:{}", args.port);
    if let Some(ip) = lan_ip() {
        info!("LAN players connect to: {ip}:{}", args.port);
    }
    serve(listener, setup).await
}

/// This machine's address on the local network (no packet is sent: connecting a UDP socket
/// only selects the outgoing interface).
fn lan_ip() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}
