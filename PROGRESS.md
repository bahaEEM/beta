# İlerleme — Faz 0

| Görev | Durum | Commit | Not |
|---|---|---|---|
| G-ENV | sudo bekliyor | – | Rust 1.99 + cargo-deny kuruldu (sudo'suz). Eksik: gdal-bin, mold, linux-perf (sudo). Godot G9'da ~/.local/opt'a indirilecek. |
| G0 | bitti | (bu commit) | mold yok → .cargo/config.toml sonra |
| G1 | bitti | (bu commit) | units.toml + damage_hp, World, HeightMap |
| G2 | bitti | 29e1c87 | alt ajan |
| G3 | bitti | f50ad39 | alt ajan; ölü birim görünmez olur |
| G4 | bitti | bbdc8a0 | atış için atıcının kendi görüş hattı da şart |
| G5 | bitti | (bu commit) | iz 1 = 500 m hücre merkezi; kayıp iz hep bölge |
| G6 | bitti | (bu commit) | G5 olaylarına göre küçük ayar gerekebilir; docs/protocol.md testten üretilir |
| G7 | bitti | (bu commit) | 5 uçtan uca test; data/scenarios/hasandag.toml |
| G8 | bitti | 635029a + (bu commit) | gerçek harita rasterio ile üretildi; 954–3262 m |
| G9 | kod bitti, görsel test bekliyor | (bu commit) | Godot 4.6.3; Terrain3D 1.0.2 çalışma anında import; tests/smoke.gd iki Godot istemcisiyle tam tur OK. Gerçek harita GDAL bekliyor |
| G10 | bekliyor | | |

## Açık sorular
- (yok)
