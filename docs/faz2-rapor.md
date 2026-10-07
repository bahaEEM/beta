# Faz 2 raporu (2026-10-07)

Hedef: mekaniği kusursuz ve dengeli hale getirmek — muharebe modeli + denge + tasarım belgesi, haberleşme ağı, topçu ve füze.
Görsel tasarım (C&C Generals çizgisi, güncel askeri teknolojiden esintiler) Faz 3'e kaldı.

## Yapılanlar
| Görev | Ne | Commit |
|---|---|---|
| F2-1 muharebe + denge | Beklenen hasar (isabet mesafeyle düşer, zırh delme çarpanı, araçlarda can), nişan süresi, ateş kuralları (serbest / karşılık / ateş yok), Tanksavar Timi, senaryo 5,6 km aralı, `docs/design.md` (K-001… kuralları + kural↔test tablosu), denge raporu testi | 2e09c53 |
| F2-2 haberleşme | Karargâh; telsiz zinciri (röle); yol boyunca kablo; karıştırıcı telsizi keser; karargâha bağlı olmayan birim yeni emir almaz | a79aa12 |
| F2-3 topçu ve füze | Obüs (3–30 km) ve Füze Rampası (10–70 km); noktaya veya izlenen (iz ≥ 2) düşmana atış görevi; yatırma süresi; uçuş süresi; alan hasarı (dost da etkilenir); zırh patlayıcıyı azaltır | 7ec2bab |
| F2-4 istemci | F: ateş kuralı, T: atış görevi (noktaya/birime), kopuk birim işareti, karargâh işareti, patlama ve namlu parlaması, yeni birimlerin görünüşü | ddf6c08 |

## Ölçümler
- Hasandağı, taraf başına 16 birim (karargâh, obüs, füze dahil), 5 tur: çözüm 8–24 ms/tur; tur başı 12 topçu mermisi.
- 3 ağ koşusu bayt düzeyinde aynı; hile testi (EW + topçu) temiz.
- 81 Rust testi + Godot ekransız testleri (smoke, main_check, terrain_check) yeşil; clippy, cargo deny temiz.
- İstemci ~144 FPS (RTX 3050).

## Kontroller (yeni)
F ateş kuralı (Serbest → Karşılık → Ateş Yok) · T atış görevi, sonra sol tık (düşmana yakın = birime, değilse noktaya), Esc iptal.
Kopuk birimler gri kesik halkalı; onlara emir verilemez. Karargâh beyaz çerçeveli.

## Bilinen eksikler / kararınızı bekleyenler
- Sayılar hâlâ yer tutucu; oyun testiyle ayarlanmalı (`cargo test --release -p server --test balance -- --ignored --nocapture`).
- Topçu atışı yayın yapmaz/duyulmaz; karşı-batarya radarı yok.
- Telsiz kullanımı ESM'e görünmez (sadece radar ve karıştırıcı yayın yapar).
- Grup emrinde tek kopuk birim bütün emri reddettirir (bilinçli: açık geri bildirim).
- Hedef seçimi (T + tık) gerçek fareyle ekranda denenmedi.

## Faz 3 önerileri
1. Sizinle oyun testi ve denge turu (sayılar).
2. Görsel tasarım: C&C Generals çizgisinde birim modelleri, efektler, arayüz.
3. Karşı-batarya radarı, telsiz yayınının ESM'e görünmesi, hava birimleri.
