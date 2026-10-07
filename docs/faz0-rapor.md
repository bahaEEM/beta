# Faz 0 raporu (2026-10-07)

Hedef: Hasandağı haritasında piyade ve tankla, LAN üzerinden uçtan uca WEGO turu. **Sağlandı.**

## Çalışanlar
- **Simülasyon (sim/):** veri dosyaları (birimler, kurallar), A* yol bulma + eğim sınırı, görüş hattı ve iz kalitesi (optik),
  doğrudan ateş + zırh açısı + hasar, tur motoru (KBG bütçesi), oyuncu başına görünürlük filtresi. Rastgelelik yok, saat okuma yok.
- **Protokol (protocol/):** JSON + zstd, 1 MB sınırı, sürüm kontrolü. `docs/protocol.md` testten üretilir.
- **Sunucu (server/):** 0.0.0.0:7777, 2 oyuncu, 45 sn planlama veya iki "Hazır", çözüm ayrı iş parçacığında,
  her tur sonu `saves/<maç>/turn_<n>.json.zst`, `--resume`, aynı isimle yeniden bağlanma.
- **Harita (tools/, maps/hasandag/):** Copernicus GLO-30'dan 40 x 40 km; sim 800 x 800 @ 50 m, Terrain3D 1334 x 1334 @ 30 m,
  yükseklik 954–3262 m, zirve haritanın ortasında.
- **İstemci (client/):** Godot 4.6.3 + Terrain3D 1.0.2. Bağlan, birim seç (sol tık), rota noktası (sağ tık, sunucudan önizleme),
  Hazır, geri sayım, 20 sn'lik çözüm oynatma, soluk temas işaretleri. Kural hesabı yok.

## Ölçümler
| Ölçüm | Sonuç | Hedef |
|---|---|---|
| Çözüm fazı (Hasandağı, 16 birim, 200 adım, release) | 1,4–4,5 ms / tur | < 1 sn |
| Aynı senaryo, ağ üzerinden 3 kez, 5 tur | olay akışları bayt düzeyinde aynı | aynı |
| Hile testi (20 rastgele senaryo + Hasandağı 5 tur + ağ) | görülmeyen birim kimliği yok | yok |
| İstemci FPS (RTX 3050, arazi üzerinde kamera uçuşu) | ~144 (ekran yenileme sınırı) | akıcı |
| Testler | 49 test yeşil; fmt, clippy -D warnings, cargo deny temiz | yeşil |

Flamegraph gerekmedi; hedefin çok altında.

## Nasıl oynanır
```sh
cargo run --release -p server                 # sunucu (LAN: 0.0.0.0:7777)
godot --path client                           # her oyuncu; adres + isim girip "Bağlan"
cargo run --release -p server -- --resume <maç_kimliği>   # kaldığı yerden
```
Kamera: WASD/oklar kaydırma, Q/E döndürme, tekerlek yakınlaştırma.

## Bilinen eksikler
- **Görsel test yapılmadı:** iki pencereyle elle bir tur oynanmalı (sahte fare/klavye kullanılmadı).
  Protokol uyumu ekransız Godot testiyle doğrulandı (`client/tests/smoke.gd`, `terrain_check.gd`).
- **Hız / ölçek:** tank 200 m/tur, piyade 30 m/tur. 40 km haritada çatışma çok yavaş; Hasandağı senaryosunda
  5 turda sadece 1 atış oldu. Tasarım kararı gerekiyor (hızlar mı, tur süresi mi).
- İz kalitesi 2 henüz hiçbir sensörde yok; sadece 1 ve 3.
- KBG bütçesi istemcide "emir sayısı / bütçe" olarak gösteriliyor; emir maliyeti protokolde yok.
- Kopan oyuncu kendi kopukken oynanan çözümün olaylarını almaz; sadece güncel TurnStart alır.
- Birim boyutları istemcide görünürlük için büyütüldü (piyade 40 m kutu).
- `client/addons/terrain_3d` sadece Linux ikilileri içerir.
- Ölü birimler görünmez olur (iz kaybı); enkaz gösterilmiyor.

## Faz 1 önerileri
1. Ölçek kararı: tur başına oyun süresini (ör. 20 sn gerçek = 2–5 dk oyun) veya hızları belirleyip `rules.toml`'a koymak.
2. docs/design.md ve K-xxx kural kimlikleri; testleri yeniden adlandırmak.
3. Yol ağı (lojistik + hız bonusu), bölge ele geçirme (piyade), ikmal.
4. Radar/ESM sensörleri: `SensorKind` genişletme, iz kalitesi 2, yayın yapan sensörlerin duyulması.
5. İstemci: emir silme, kontrol grupları (Ctrl+1–9), birim bilgisi paneli, gerçek modeller.
6. CI'da Godot ekransız testleri (smoke + terrain_check).
