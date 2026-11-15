# Protokol

Bu dosya `protocol/src/lib.rs` testlerinden üretilir; elle düzenleme. Güncelleme: `UPDATE_DOCS=1 cargo test -p protocol`.

- Taşıma: WebSocket ikili (binary) mesaj. İçerik: JSON, sonra zstd (seviye 3).
- Godot'ta açma: `bytes.decompress(1048576, FileAccess.COMPRESSION_ZSTD)` (`decompress_dynamic` zstd desteklemez) sonra `JSON.parse_string(...)`.
- JSON en fazla 1 MB (1048576 bayt); büyüğü hata.
- Her mesajın `type` alanı vardır. Bağlantıdaki ilk mesaj `hello` olmalı; `protocol_version` sunucununkiyle (şu an 10) aynı değilse bağlantı `error` ile kapatılır.
- Konumlar metre cinsinden; x doğuya, y güneye artar. `position.kind`: `exact` (kesin), `area` (iz kalitesi 1: merkez + yarıçap) veya `bearing` (tek ESM kerterizi: `origin` noktasından `angle_rad` yönünde bir ışın; konum bilinmez). Açılar radyan, x eksenine göre, y güneye.
- Görmediğiniz birimin kimliği hiçbir mesajda yer almaz (`shot_fired` içinde `null` olur).
