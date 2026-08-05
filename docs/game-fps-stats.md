# Game FPS (Stats Overlay)

## Konsep

`gameFps` adalah FPS yang ditampilkan di overlay statistik dalam game (biasanya pojok layar saat streaming). Nilainya berasal dari **host GFN** (server NVIDIA), bukan dihitung di sisi klien Android.

## Cara Kerja

### 1. Pengiriman dari Host

Setiap detik, host GFN mengirimkan paket biner berisi statistik performa melalui **DataChannel** WebRTC (label: `stats`). Paket ini diklasifikasikan sebagai `InputDataChannelRole.Other` di [InputDataChannelLabels.kt](android/app/src/main/java/com/opencloudgaming/opennow/Streaming.kt).

Format paket (binary, little-endian):

| Byte | Isi |
|------|-----|
| 0 | Version (uint8) |
| 1–24 | Reserved / field lain |
| 25–32 | `avgGameFps` (double, 8 byte) |

Kondisi: hanya diparsing jika `version >= 4`.

### 2. Parsing di Klien

Di [Streaming.kt:4878–4883](android/app/src/main/java/com/opencloudgaming/opennow/Streaming.kt):

```kotlin
val version = statsBuffer.get(0).toInt() and 0xff
if (version >= 4) {
    val avgGameFps = statsBuffer.getDouble(25)
    if (avgGameFps > 0.0 && avgGameFps <= 360.0) {
        lastParsedGameFps = kotlin.math.round(avgGameFps).toInt()
    }
}
```

`lastParsedGameFps` disimpan sebagai state module-level dan dipakai setiap frame saat membangun `RuntimeStatsSnapshot`.

### 3. Fallback Chain untuk `fps` (overlay)

Urutan prioritas jika `lastParsedGameFps` tidak tersedia:

1. **`explicitFps`** — dari WebRTC stat `framesPerSecond` (sering 0 atau kosong di Android).
2. **`derivedFps`** — dihitung dari delta `framesDecoded` / elapsed time antar sampel.
3. **`settings.fps`** — nilai target FPS yang di-set pengguna (default 60).

```kotlin
fps = explicitFps?.roundToInt()?.takeIf { it > 0 }
    ?: derivedFps?.takeIf { it > 0 }
    ?: settings.fps
```

### 4. `gameFps` vs `fps`

`gameFps` menggunakan fallback yang sama dengan `fps`, tetapi ditambahkan **small random jitter** (`-1..0`) agar angka tidak terlihat statis/unnatural di overlay:

```kotlin
gameFps = lastParsedGameFps
    ?: (explicitFps?.roundToInt()?.takeIf { it > 0 }
        ?: derivedFps?.takeIf { it > 0 }
        ?: settings.fps).let { base ->
        if (base > 0) (base + (-1..0).random()).coerceAtLeast(30) else null
    }
```

Jitter ini sengaja ditambahkan agar overlay terlihat "hidup" — sesuai perilaku GFN official client.

### 5. `receivedFps` & `decodedFps`

Dua field terpisah yang juga ditampilkan di overlay (jika diaktifkan):

- **`receivedFps`** — delta `framesReceived` (dari WebRTC `framesReceived` stat) per detik.
- **`decodedFps`** — delta `framesDecoded` per detik (sama dengan `derivedFps`).

Keduanya dihitung di [Streaming.kt:5030–5033](android/app/src/main/java/com/opencloudgaming/opennow/Streaming.kt).

## Ringkasan Alur

```
Host GFN → DataChannel (binary stats packet, version ≥ 4)
    → client parses byte[25..32] as double → lastParsedGameFps
    → overlay: gameFps = lastParsedGameFps ?: fallback chain
    → jitter added: gameFps = base + random(-1, 0)
```

## Referensi Kode

- [Streaming.kt:4863–4883](android/app/src/main/java/com/opencloudgaming/opennow/Streaming.kt#L4863-L4883) — parsing paket stats biner
- [Streaming.kt:5084–5101](android/app/src/main/java/com/opencloudgaming/opennow/Streaming.kt#L5084-L5101) — pembentukan `RuntimeStatsSnapshot`
- [Streaming.kt:5025–5033](android/app/src/main/java/com/opencloudgaming/opennow/Streaming.kt#L5025-L5033) — perhitungan `derivedFps` & `receivedFps`
- [Models.kt:1564–1566](android/app/src/main/java/com/opencloudgaming/opennow/Models.kt#L1564-L1566) — definisi field `gameFps`, `receivedFps`, `decodedFps` di `StreamRuntimeStats`
