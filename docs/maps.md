# Haritalar: Hasandağı yükseklik haritası nasıl üretilir

Bu belge, Hasandağı zirvesini merkeze alan 40x40 km'lik oyun haritasının
gerçek yükseklik verisinden nasıl üretildiğini adım adım anlatır.
Programcı olmanız gerekmez; komutları sırayla terminale yapıştırın.

## Kaynak ve zirve

- Yükseklik verisi: **Copernicus GLO-30 DEM** (30 m çözünürlük). AWS'deki açık
  bucket'tan indirilir; hesap veya anahtar gerekmez.
- Zirve: **38°07′39″K, 34°10′00″D** (38,1275 K, 34,1667 D), **3268 m**.
  Kaynak: <https://en.wikipedia.org/wiki/Mount_Hasan> (Wikipedia, "Mount Hasan").
- Aynı nokta UTM 36K (EPSG:32636) projeksiyonunda yaklaşık
  **D 602250 m, K 4220600 m**.
- Kesilen kutu (zirve ortada, 40,02 km x 40,02 km, 30 m'nin tam katı):
  `582240 4200590 622260 4240610` (xmin ymin xmax ymax, metre).

## 1. GDAL'ı kurun (bir kez)

```sh
sudo apt install gdal-bin
```

## 2. Çalışma klasörü açın (repo DIŞINDA)

Ham DEM dosyaları büyüktür ve repoya **eklenmez**.

```sh
mkdir -p ~/dem-hasandag && cd ~/dem-hasandag
```

## 3. Dört DEM karosunu indirin

Kutu 37–38 K ve 33–34 D derecelerine taşar, bu yüzden 4 karo gerekir:

```sh
for t in N37_00_E033_00 N37_00_E034_00 N38_00_E033_00 N38_00_E034_00; do
  curl -fL -C - -O "https://copernicus-dem-30m.s3.amazonaws.com/Copernicus_DSM_COG_10_${t}_DEM/Copernicus_DSM_COG_10_${t}_DEM.tif"
done
```

## 4. Karoları birleştirin

```sh
gdalbuildvrt hasandag.vrt Copernicus_DSM_COG_10_*_DEM.tif
```

## 5. Metrik projeksiyona çevirip kesin

```sh
gdalwarp -t_srs EPSG:32636 -tr 30 30 \
  -te 582240 4200590 622260 4240610 \
  -r bilinear -ot Float32 -overwrite \
  hasandag.vrt hasandag_utm36.tif
```

Sonuç: 1334 x 1334 piksel, her piksel 30 m.

## 6. Oyun dosyalarına dönüştürün

Repo klasöründe:

```sh
cargo run --release -p tools -- convert \
  --input ~/dem-hasandag/hasandag_utm36.tif \
  --out-dir maps/hasandag --cell 50
```

Ekrana boyut ve en düşük/en yüksek yükseklik yazılır.

**Deniz (2026-11-14'ten beri kullanılan komut).** Haritanın doğusu deneme amaçlı denize çevrilir. Üç ek ayar birlikte verilir:

```sh
cargo run --release -p tools -- convert \
  --input ~/Masaüstü/calisma-alani/veri/dem-hasandag/hasandag_utm36.tif \
  --out-dir maps/hasandag --cell 50 \
  --sea-x0 21000 --sea-slope 100 --sea-level 950
```

- `--sea-x0 21000`: batı kenarından 21 km sonra arazi doğuya doğru alçaltılmaya başlar.
- `--sea-slope 100`: her km'de 100 m alçalır (dağ yerinde kalır, doğu yaylası suyun altına iner).
- `--sea-level 950`: 950 m'nin altında kalan her yer 950 m'de düz su olur. `map.toml`'a `sea_level_m` yazılır.

Kıyıyı kaydırmak için `--sea-x0` değerini değiştirin; sonra aşağıdaki `roads` ve `cover` komutlarını yeniden çalıştırın.

## Çıkan dosyalar (`maps/hasandag/`)

| Dosya | Ne |
|---|---|
| `sim_height.bin` | Simülasyon yükseklikleri: 50 m ızgara, f32 little-endian, satır satır, 0. satır = kuzey kenarı. 40 km için 800 x 800. |
| `terrain_r16.raw` | Godot/Terrain3D için 16-bit (little-endian, işaretsiz) yükseklik, kaynak çözünürlükte (30 m). En düşük → 0, en yüksek → 65535. |
| `map.toml` | Genişlik, yükseklik, hücre boyu, başlangıç noktası (0,0 örneğinin UTM koordinatı), CRS, min/max yükseklik ve `[terrain_r16]` bölümü. |

Terrain3D içe aktarmada: genişlik/yükseklik `[terrain_r16]` bölümünden, yükseklik
aralığı `min_height_m`..`max_height_m`, piksel boyu `pixel_size_x_m` (30 m).

## Bilinmesi gerekenler

- Araç sadece **metre** cinsinden projeksiyonlu GeoTIFF kabul eder. Derece cinsinden
  (5. adım atlanmış) dosya verilirse hata verir.
- Veri olmayan (nodata/NaN) pikseller **en düşük geçerli yükseklikle** doldurulur.
  Hasandağı kutusunda nodata beklenmez.
- Bu dosyalar Copernicus DEM lisansına tabidir, oyunun CC BY-SA varlık lisansına
  **değil**. Ayrıntı: `ASSETS.md`.

## GDAL kurulu değilse (sudo'suz yol)

`rasterio` Python paketi GDAL'ı kendi içinde getirir:

```sh
python3 -m venv venv-rio && venv-rio/bin/pip install rasterio
export LC_ALL=C   # Türkçe dil ayarında GDAL gereksiz uyarılar basar
venv-rio/bin/rio merge Copernicus_DSM_COG_10_*_DEM.tif merged.tif --overwrite
venv-rio/bin/rio warp merged.tif hasandag_utm36.tif --dst-crs EPSG:32636 --res 30 \
  --dst-bounds 582240 4200590 622260 4240610 --resampling bilinear --overwrite
```

2026-10-07 üretimi: kaynak 1334 x 1334 @ 30 m, sim 800 x 800 @ 50 m,
yükseklik 953,98 – 3262,29 m, zirve sim hücresi (397, 402) ≈ (19850, 20100) m.

## Yol ağı (OpenStreetMap)

Yollar OpenStreetMap'ten Overpass API ile indirilir (hesap gerekmez). Ham dosya repoya
**eklenmez**. Kutu, harita alanını kapsayan enlem/boylam zarfıdır (G, B, K, D).

```sh
mkdir -p ~/Masaüstü/calisma-alani/veri/osm && cd ~/Masaüstü/calisma-alani/veri/osm
curl -sS -A 'beta-game-tools/0.1' -H 'Accept: application/json' -o hasandag_roads.json \
  --data-urlencode 'data=[out:json][timeout:120]; way["highway"~"^(motorway|trunk|primary|secondary|tertiary|unclassified|residential|track|service)$"](37.93,33.90,38.32,34.43); out geom;' \
  https://overpass-api.de/api/interpreter
```

`-A` ve `-H` başlıkları olmadan sunucu 406 döner. Ana sunucu yoğunsa (504) aynı komutu
`https://overpass.kumi.systems/api/interpreter` ile deneyin (2026-10-07 verisi oradan).

Repo klasöründe:

```sh
cargo run --release -p tools -- roads \
  --osm ~/Masaüstü/calisma-alani/veri/osm/hasandag_roads.json --map-dir maps/hasandag
```

Araç enlem/boylamı kendi Transverse Mercator kodu ile UTM 36K'ye, sonra `map.toml`
başlangıcına göre simülasyon metresine çevirir (x doğu, y güney; (i, j) örneği = (i·50, j·50)).

| Dosya | Ne |
|---|---|
| `roads.bin` | Hücre başına 1 bayt, `sim_height.bin` ile aynı 800 x 800 ızgara. 0 yol yok, 1 patika/servis, 2 küçük yol (tertiary/unclassified/residential), 3 ana yol (secondary ve üstü). |
| `roads_lines.json` | `{"lines": [{"class": 2, "points": [[x, y], ...]}]}`, sim metresi, haritaya kırpılmış, noktalar arası ≥ 25 m. İstemci çizimi için. |
| `roads.toml` | Sınıf başına yol ve hücre sayısı, kaynak, OSM tarihi, lisans. |

Bu dosyalar **ODbL 1.0** lisansındadır (© OpenStreetMap contributors), CC BY-SA değil. Ayrıntı: `ASSETS.md`.

Haritada deniz varsa yollar kıyıda kesilir (su hücresinde yol olmaz).

## Arazi örtüsü ve yer adları (OpenStreetMap)

Orman ve yerleşim alanları ile yer adları tek sorguyla indirilir:

```sh
cd ~/Masaüstü/calisma-alani/veri/osm
curl -sS -A 'beta-game-tools/0.1' -H 'Accept: application/json' -o hasandag_cover.json \
  --data-urlencode 'data=[out:json][timeout:120];(node["place"~"^(city|town|village|hamlet|suburb|neighbourhood)$"](37.93,33.90,38.32,34.43);way["landuse"~"^(forest|residential|industrial|commercial|farmland|orchard|meadow)$"](37.93,33.90,38.32,34.43);way["natural"~"^(wood|scrub|water)$"](37.93,33.90,38.32,34.43);relation["landuse"~"^(forest|residential)$"](37.93,33.90,38.32,34.43);relation["natural"~"^(wood|water)$"](37.93,33.90,38.32,34.43););out geom;' \
  https://overpass-api.de/api/interpreter
```

Repo klasöründe (önce `convert`, çünkü su `sim_height.bin` ve `sea_level_m`'den okunur):

```sh
cargo run --release -p tools -- cover \
  --osm ~/Masaüstü/calisma-alani/veri/osm/hasandag_cover.json --map-dir maps/hasandag
```

| Dosya | Ne |
|---|---|
| `terrain.bin` | Hücre başına 1 bayt, 800 x 800. 0 açık arazi, 1 orman (`landuse=forest`, `natural=wood`), 2 yerleşim (`landuse=residential/industrial/commercial`), 3 su (yükseklik ≤ `sea_level_m`). Yerleşim ormanı, su ikisini de ezer. |
| `terrain.toml` | Tip başına hücre sayısı, kaynak, OSM tarihi, lisans. |

2026-11-14 üretimi: açık 402 122, orman 8 372, yerleşim 12 436, su 217 070 hücre (haritanın %34'ü deniz). **ODbL 1.0.**

## Adlı bölgeler

Aynı `hasandag_cover.json` dosyasındaki yer adlarından (en son, `roads` ve `cover`'dan sonra çalıştırın):

```sh
cargo run --release -p tools -- regions \
  --osm ~/Masaüstü/calisma-alani/veri/osm/hasandag_cover.json --map-dir maps/hasandag \
  --min-dist 2000 --extra "Hasandağı Zirvesi:19850:20100"
```

- Merkezler: karadaki adlı yerleşimler; önce kasaba, sonra köy, mahalle, mezra sırasıyla alınır, seçilmiş bir merkeze
  `--min-dist` metreden yakın olan atlanır. `--extra ad:x:y` (sim metresi) ile elle merkez eklenir.
- Her kara hücresi en yakın merkezin bölgesine girer. Kimlikler kuzeyden güneye 1'den başlar.
- Aynı adlı ikinci yerleşim mahalleyse "… Mahallesi" olur (Gözlükuyu köyü ve Gözlükuyu Mahallesi).

| Dosya | Ne |
|---|---|
| `regions.toml` | Bölge başına kimlik, ad, tür, merkez (x, y), hücre sayısı, sınır komşuları, yolla bağlı bölgeler. |
| `regions.bin` | Hücre başına 1 bayt bölge kimliği, 800 x 800; 0 = su. |
| `regions_lines.json` | `{"borders": [{"a": 1, "b": 3, "segments": [[x0, y0, x1, y1], ...]}]}`: bölge çiftleri arasındaki sınır parçaları, istemci çizimi için. |

2026-11-14 üretimi: 30 bölge, 71 sınır, 48 yol bağlantısı. **ODbL 1.0.**

**Dikkat:** bölgeler yeniden üretilirse `data/scenarios/hasandag.toml` içindeki `[[zone]]` kayıtları (kimlik, ad, merkez) elle eşitlenmelidir.
