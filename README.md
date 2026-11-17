# beta

A deterministic, turn-based (WEGO) electronic warfare strategy game, played on real terrain.

*Deterministik, tur tabanlı (WEGO) elektronik harp strateji oyunu. Türkçe belgeler `docs/` altında.*

## The idea

Information is the main weapon. You find the enemy with radar, passive ESM (it only gives you a bearing, never a position) and optics, and you hide from them by jamming, staying quiet and using the terrain. Both players plan at the same time, then the server resolves the turn.

The map is real: 40 × 40 km around Hasandağı, Turkey, built from Copernicus GLO-30 elevation data and OpenStreetMap roads, land cover and place names.

## How it's built

| Part | What it does |
|---|---|
| `sim/` | Pure Rust library with every game rule. No randomness and no clock, so the same input always gives the same result. |
| `server/` | Rust (tokio). Runs matches, validates orders and filters what each player can see. A client never receives a unit its sensors haven't detected. |
| `protocol/` | Client-server messages (JSON + zstd). |
| `client/` | Godot 4 + Terrain3D. Only display and input, no rule logic. |
| `tools/` | Map conversion tools (DEM and OSM to game data). |
| `data/` | All game numbers live in TOML files, not in code. |

Working so far: line of sight, direct fire with armor angles, A* pathfinding that prefers roads, group orders, supply (fuel and ammunition), capture zones and victory, plus radar, ESM and jamming units. Rules are documented in `docs/design.md`, and each rule has a matching test.

## Run it

Needs Rust and Godot 4.6.

```bash
./oyna.sh               # server + two clients on this machine
./oyna.sh sunucu        # server only (LAN)
./oyna.sh baglan <IP>   # connect to a server on another machine
cargo test --all        # tests
```

## How it's made

I design the rules and test every build myself. Most of the code is written by AI coding agents (Claude Code) under my review, guided by strict project rules: determinism, no rule logic in the client, one test per rule.

## License

Code: AGPL-3.0-or-later. Assets: CC BY-SA 4.0. Map data has its own licenses (Copernicus DEM, OpenStreetMap ODbL); see `ASSETS.md`.
