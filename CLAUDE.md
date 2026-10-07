# Proje: "beta" — deterministik, tur tabanlı (WEGO) elektronik harp strateji oyunu

## Mimari
- sim/: Saf Rust kütüphanesi. TÜM oyun kuralları burada. Ağ, dosya sistemi veya motor bağımlılığı yok.
- protocol/: İstemci-sunucu mesaj tipleri (serde, JSON).
- server/: Rust (tokio + tokio-tungstenite). Maç yönetimi, emir doğrulama, çözüm fazı simülasyonu, oyuncu başına görünürlük filtresi.
- tools/: Rust komut satırı araçları (harita dönüştürme).
- client/: Godot 4 projesi (GDScript). SADECE gösterim, girdi ve arayüz.
- data/: TOML veri dosyaları (birimler, kurallar).
- docs/design.md: kurallar (K-001…) ve kural↔test tablosu; çelişkide belge kazanır. docs/GOREVLER.md: faz görevleri ve kararlar.

## Altın kurallar (asla ihlal etme)
1. Oyun kuralları sadece sim/ içinde. client/ hiçbir oyun kuralı hesaplamaz.
2. Rastgelelik yok. rand ve benzeri crate'ler sim/ ve server/ içinde yasak. Aynı girdi her zaman aynı çıktıyı verir.
3. Oyun sayıları kodda yazılmaz; data/*.toml dosyalarından okunur.
4. Çözüm fazı 200 adım; adım = 0,6 sn oyun zamanı (tur = 2 dk oyun, ekranda 20 sn). Adım sırası docs/design.md K-003:
   hareket -> sensör/iz -> ateş (eşzamanlı) -> hasar -> ele geçirme; tur sonu: puan -> ikmal -> zafer.
5. Belirleyicilik: birimler her zaman kimlik sırasıyla işlenir. Oyun durumunda HashMap yerine BTreeMap veya sıralı Vec. Simülasyon içinde saat okunmaz.
6. Görünürlük: bir oyuncuya giden hiçbir mesaj, o oyuncunun sensörlerinin görmediği bir birimi içeremez.
7. sim/ ve server/ içinde unwrap()/expect() ile panik yok; hatalar Result ile döner. Sunucu asla panikle çökmez.
8. İzinli bağımlılık lisansları: MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0, Unicode. cargo deny check geçmeli.
9. Her kaynak dosyanın başında: // SPDX-License-Identifier: AGPL-3.0-or-later (GDScript'te # ile).
10. Kod, tanımlayıcılar ve yorumlar İngilizce; kullanıcıya özetler Türkçe.

## Çalışma şekli
- Her görevde önce kısa plan, sonra uygulama.
- Her kural için test; yeni test adı kural kimliğiyle başlar (ör. k041_first_shot_only_after_aim_time) ve design.md tablosuna eklenir.
- Bitirmeden önce: cargo fmt --all, cargo clippy --all-targets -- -D warnings, cargo test --all, cargo deny check.
- Görev sonunda Türkçe özet: ne yapıldı, hangi dosyalar, nasıl test edildi, bilinen eksikler.
- Emin olmadığın tasarım sorusunda varsayım yapma; dur ve sor. Kapsam dışına çıkma; fark ettiğin sorunları sadece listele.
- git push yok. sudo yok. Repo dışındaki dosyalara dokunma.
