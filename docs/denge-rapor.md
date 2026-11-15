# Denge raporu — ilk tur (2026-11-15)

Ölçüm: `cargo test --release -p server --test balance duels -- --ignored --nocapture`
İki tarafa **eşit para** (3000) verilir; satırdaki birim, sütundaki duran birime düz açık arazide 2 km'den saldırır; 16 tur,
ikmal yok. Hücre: saldıranın kalan canı % / savunanın kalan canı %. Rapor bir sınama değildir (geçti/kaldı yok), sayı ayarı için bakılır.

## Bulunan sorunlar ve yapılan ayar
| Sorun (ölçümde) | Ayar |
|---|---|
| Tanksavar timi eşit parayla her şeyi siliyordu, piyadeyi bile (füzeyle) | K-047: füze piyadeye atılmaz; fiyat 150 → 220 |
| Tank, 3 km'den vuran tanksavara hiç karşılık veremiyordu | Tank top menzili 2000 → 2500 m (atış yapan tim 30 sn görünür olur, K-140) |
| Roketatar timi ormanda bile araçlara yetişemiyordu (menzil 500 m, görülme ~610 m) | Menzil 500 → 700 m: ormandan pusu artık işliyor |
| Makineli tüfek timi piyadeden her yönden iyiydi ve neredeyse aynı fiyattı | 120 → 150 |
| Piyade, keşif ve ZMA parasının karşılığını vermiyordu; tank fazla ucuzdu | Piyade 100 → 80, keşif 200 → 150, roketatar 130 → 120, ZMA 300 → 280, tank 500 → 550 |

## Ayardan sonra (açık arazi)
```
     att \ def  infantry x38 mg_team x20 rocket_inf x25  atgm x14  recon x20  ifv x11   mbt x5
  infantry x38       80/80       0/100       43/41       100/0      62/100     0/100    0/100
   mg_team x20       34/23       0/100       0/100       100/0      26/100     0/100    0/100
rocket_inf x25       34/37       0/100       0/100       100/0       33/90     0/100    0/100
      atgm x14      16/100       0/100       0/100     100/100       100/0      0/48     73/0
     recon x20      100/47       100/0        0/75       0/100       100/0     0/100    0/100
       ifv x11       100/0       100/0       100/0        39/0       100/0      47/0    0/100
        mbt x5       100/0       100/0        0/40        0/79       100/0     100/0     11/0
```
Savunan ormanda: tank → piyade 100/37 · tank → roketatar **0/79** · tank → tanksavar **0/86** · ZMA → piyade 100/11 ·
ZMA → roketatar 7/0 · piyade → makineli tüfek 15/100.
Hava: helikopter → tank 100/0 · helikopter → hava savunma **0/78** · SİHA → tank 100/0 · SİHA → hava savunma **0/100** · tank → hava savunma 100/0.

## Okuma
- Çark çalışıyor: piyade tanksavarı yener → tanksavar ve ormandaki roketatar tankı yener → tank ve ZMA açıktaki piyadeyi yener;
  helikopter ve SİHA zırhı yer → hava savunma onları düşürür → hava savunmayı kara birimi ezer.
- Açık arazide araçlara karşı piyadenin hiçbir şansı yok (bilerek): piyadenin değeri bölge almak, örtü, pusu ve ucuzluk.
- Piyade–piyade çatışması çok yavaş (16 turda %20 kayıp): karşılıklı baskı ikisini de yere yatırıyor. Topçu, makineli tüfek ya da
  kanattan gelen araçla çözülmesi isteniyor; fazla yavaş bulunursa `suppression` sayılarına bakılacak.

## Bakılmayanlar
Topçu ve füze (alan hasarı), deniz birimleri, EH birimlerinin fiyatı, mayın/sığınak sayıları, gelir hızı (20/bölge) ve üretim süreleri
ölçülmedi; hâlâ tahmini. Gerçek denge ancak iki kişilik maçlardan sonra oturur.
