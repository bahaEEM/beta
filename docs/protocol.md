# Protokol

Bu dosya `protocol/src/lib.rs` testlerinden üretilir; elle düzenleme. Güncelleme: `UPDATE_DOCS=1 cargo test -p protocol`.

- Taşıma: WebSocket ikili (binary) mesaj. İçerik: JSON, sonra zstd (seviye 3).
- Godot'ta açma: `bytes.decompress_dynamic(1048576, FileAccess.COMPRESSION_ZSTD)` sonra `JSON.parse_string(...)`.
- JSON en fazla 1 MB (1048576 bayt); büyüğü hata.
- Her mesajın `type` alanı vardır. Bağlantıdaki ilk mesaj `hello` olmalı; `protocol_version` sunucununkiyle (şu an 1) aynı değilse bağlantı `error` ile kapatılır.
- Konumlar metre cinsinden; x doğuya, y güneye artar. `position.kind`: `exact` (kesin) veya `area` (iz kalitesi 1: merkez + yarıçap).
- Görmediğiniz birimin kimliği hiçbir mesajda yer almaz (`shot_fired` içinde `null` olur).

## İstemci → sunucu

```json
{
  "type": "hello",
  "protocol_version": 1,
  "player_name": "baha"
}
```

```json
{
  "type": "submit_orders",
  "turn": 3,
  "orders": [
    {
      "type": "move",
      "unit": 7,
      "waypoints": [
        {
          "x": 1200.0,
          "y": 850.5
        }
      ]
    }
  ]
}
```

```json
{
  "type": "request_preview",
  "unit": 7,
  "waypoints": [
    {
      "x": 1200.0,
      "y": 850.5
    }
  ]
}
```

```json
{
  "type": "ready"
}
```

## Sunucu → istemci

```json
{
  "type": "welcome",
  "player_id": 0,
  "map_id": "hasandag"
}
```

```json
{
  "type": "turn_start",
  "turn": 3,
  "deadline_secs": 45,
  "view": {
    "own_units": [
      {
        "id": 7,
        "type_id": "mbt",
        "pos": {
          "x": 1200.0,
          "y": 850.5
        },
        "heading_rad": 1.5,
        "hp": 100.0,
        "state": "healthy"
      }
    ],
    "contacts": [
      {
        "unit": 12,
        "quality": 1,
        "type_id": null,
        "position": {
          "kind": "area",
          "center": {
            "x": 1200.0,
            "y": 850.5
          },
          "radius_m": 500.0
        },
        "last_seen_tick": 140
      }
    ],
    "kbg_budget": 8
  }
}
```

```json
{
  "type": "resolution",
  "turn": 3,
  "events": [
    {
      "tick": 1,
      "type": "unit_moved",
      "unit": 7,
      "position": {
        "kind": "exact",
        "pos": {
          "x": 1200.0,
          "y": 850.5
        }
      },
      "heading_rad": 1.5
    },
    {
      "tick": 2,
      "type": "track_gained",
      "unit": 12,
      "quality": 3,
      "position": {
        "kind": "exact",
        "pos": {
          "x": 1200.0,
          "y": 850.5
        }
      }
    },
    {
      "tick": 3,
      "type": "shot_fired",
      "shooter": 7,
      "target": 12
    },
    {
      "tick": 3,
      "type": "unit_damaged",
      "unit": 12,
      "hp": null,
      "state": "damaged"
    },
    {
      "tick": 83,
      "type": "unit_destroyed",
      "unit": 12
    },
    {
      "tick": 90,
      "type": "track_lost",
      "unit": 13,
      "last_known": {
        "x": 1200.0,
        "y": 850.5
      }
    }
  ]
}
```

```json
{
  "type": "preview_result",
  "unit": 7,
  "path": [
    {
      "x": 1200.0,
      "y": 850.5
    }
  ],
  "reason": null
}
```

```json
{
  "type": "order_rejected",
  "reason": "kbg budget exceeded"
}
```

```json
{
  "type": "error",
  "code": "version_mismatch",
  "message": "protocol version mismatch: server 1, client 0"
}
```
