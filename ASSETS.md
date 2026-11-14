# Varlık kaydı

Kod AGPL-3.0-or-later (LICENSE), varlıklar CC BY-SA 4.0 (LICENSE-ASSETS) — aksi yazmadıkça.
DEM ve OSM kaynaklı harita dosyaları (maps/hasandag/) bu CC BY-SA lisansının dışındadır; kendi satırlarına bakın.

| Dosya | Kaynak | Araç | Lisans | Tarih |
|---|---|---|---|---|
| maps/hasandag/sim_height.bin, terrain_r16.raw, map.toml | Copernicus GLO-30 DEM, karolar N37/N38 x E033/E034, <https://registry.opendata.aws/copernicus-dem/> (adımlar: docs/maps.md) | gdalwarp (EPSG:32636, 30 m, bilinear) + `tools convert` | **CC BY-SA DEĞİL.** Copernicus DEM GLO-30 Public lisansı (ücretsiz, dünya çapında; ESA/Copernicus koşulları, <https://dataspace.copernicus.eu/explore-data/data-collections/copernicus-contributing-missions/collections-description/COP-DEM>). Atıf: "produced using Copernicus WorldDEM-30 © DLR e.V. 2010-2014 and © Airbus Defence and Space GmbH 2014-2018 provided under COPERNICUS by the European Union and ESA; all rights reserved". "Copernicus Digital Elevation Model (DEM) was accessed on 2026-10-07 from https://registry.opendata.aws/copernicus-dem." DOI: https://doi.org/10.5270/ESA-c5d3d65 | 2026-10-07 (henüz üretilmedi, GDAL yok) |
| client/addons/terrain_3d/ | https://github.com/TokisanGames/Terrain3D v1.0.2-stable (sadece Linux ikilileri) | — | MIT (addons/terrain_3d/LICENSE.txt) | 2026-10-07 |
| maps/hasandag/roads.bin, roads_lines.json, roads.toml | OpenStreetMap yolları (highway = motorway…service), Overpass API ayna sunucusu overpass.kumi.systems, veri tabanı anı 2026-07-24T11:04:51Z (adımlar: docs/maps.md) | `tools roads` | **CC BY-SA DEĞİL.** © OpenStreetMap contributors, ODbL 1.0 (https://www.openstreetmap.org/copyright). Bu dosyalar ODbL altındadır (türetilmiş veri tabanı); oyunun CC BY-SA varlık lisansı bunlara uygulanmaz. | 2026-10-07 |
| maps/hasandag/terrain.bin, terrain.toml | OpenStreetMap arazi kullanımı (landuse = forest/residential/industrial/commercial, natural = wood), Overpass API overpass-api.de, veri tabanı anı 2026-11-14T09:23:04Z; su = `sim_height.bin` içinde deniz seviyesi altı (adımlar: docs/maps.md) | `tools cover` | **CC BY-SA DEĞİL.** © OpenStreetMap contributors, ODbL 1.0 (https://www.openstreetmap.org/copyright). | 2026-11-14 |

2026-11-14: `sim_height.bin`, `terrain_r16.raw` ve `map.toml` denizli olarak yeniden üretildi (`tools convert --sea-x0 21000 --sea-slope 100 --sea-level 950`):
Copernicus yükseklikleri 21. km'den doğuya doğru km başına 100 m alçaltıldı, 950 m altı düz su yapıldı. Yani bu dosyalar artık Copernicus DEM'in
**değiştirilmiş** bir türevidir. Yollar (`roads.*`) aynı OSM verisinden, kıyıda kesilerek yeniden üretildi.
