# Protokol

Bu dosya `protocol/src/lib.rs` testlerinden üretilir; elle düzenleme. Güncelleme: `UPDATE_DOCS=1 cargo test -p protocol`.

- Taşıma: WebSocket ikili (binary) mesaj. İçerik: JSON, sonra zstd (seviye 3).
- Godot'ta açma: `bytes.decompress(1048576, FileAccess.COMPRESSION_ZSTD)` (`decompress_dynamic` zstd desteklemez) sonra `JSON.parse_string(...)`.
- JSON en fazla 1 MB (1048576 bayt); büyüğü hata.
- Her mesajın `type` alanı vardır. Bağlantıdaki ilk mesaj `hello` olmalı; `protocol_version` sunucununkiyle (şu an 2) aynı değilse bağlantı `error` ile kapatılır.
- Konumlar metre cinsinden; x doğuya, y güneye artar. `position.kind`: `exact` (kesin), `area` (iz kalitesi 1: merkez + yarıçap) veya `bearing` (tek ESM kerterizi: `origin` noktasından `angle_rad` yönünde bir ışın; konum bilinmez). Açılar radyan, x eksenine göre, y güneye.
- Görmediğiniz birimin kimliği hiçbir mesajda yer almaz (`shot_fired` içinde `null` olur).

## İstemci → sunucu

```json
{
  "type": "hello",
  "protocol_version": 2,
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
      "units": [
        7,
        8
      ],
      "waypoints": [
        {
          "x": 1200.0,
          "y": 850.5
        }
      ]
    },
    {
      "type": "stop",
      "units": [
        9
      ]
    },
    {
      "type": "set_emitter",
      "units": [
        10
      ],
      "on": true
    },
    {
      "type": "set_stance",
      "units": [
        7,
        8
      ],
      "stance": "return"
    }
  ]
}
```

```json
{
  "type": "request_preview",
  "units": [
    7,
    8
  ],
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
        "state": "healthy",
        "emitting": false,
        "fuel": 1100.0,
        "ammo": 38,
        "supply_fuel": 0.0,
        "supply_ammo": 0,
        "stance": "free"
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
      },
      {
        "unit": 14,
        "quality": 1,
        "type_id": null,
        "position": {
          "kind": "bearing",
          "origin": {
            "x": 1200.0,
            "y": 850.5
          },
          "angle_rad": 0.8
        },
        "last_seen_tick": 150
      }
    ],
    "kbg_budget": 8,
    "zones": [
      {
        "id": 1,
        "name_tr": "Helvadere",
        "center": {
          "x": 1200.0,
          "y": 850.5
        },
        "radius_m": 400.0,
        "points": 2,
        "depot": true,
        "owner": 0
      }
    ],
    "scores": [
      14,
      6
    ]
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
      "tick": 120,
      "type": "zone_captured",
      "zone": 1,
      "player": 1
    },
    {
      "tick": 90,
      "type": "track_lost",
      "unit": 13,
      "last_known": {
        "kind": "area",
        "center": {
          "x": 1200.0,
          "y": 850.5
        },
        "radius_m": 500.0
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
  "type": "orders_accepted",
  "turn": 3,
  "kbg_spent": 2,
  "kbg_budget": 8
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
  "message": "protocol version mismatch: server 2, client 1"
}
```

```json
{
  "type": "game_over",
  "winner": 1,
  "scores": [
    88,
    102
  ]
}
```
