# Analisa Rombak Total UI OpenNOW dengan Miuix

> **Tanggal:** 5 Agustus 2026 · **Status:** Riset & rekomendasi (belum implementasi) · **Branch:** `refactor/android-cleanup`
> **Pertanyaan awal:** "Bisa rombak total UI pakai [compose-miuix-ui/miuix](https://github.com/compose-miuix-ui/miuix)? Terus bisa adjust sendiri buat TV / tablet / HP?"

---

## 1. Ringkasan Eksekutif

- ✅ **Bisa** dipakai di project OpenNOW Android ini. Miuix adalah library UI Compose (gaya MIUI/HyperOS) yang dipasang lewat dependency biasa (`top.yukonga.miuix.kmp:*`). Tidak memaksa project berubah jadi Compose Multiplatform penuh.
- ⚠️ **Tidak ada "auto-adjust" untuk TV.** Miuix sama sekali tidak punya komponen TV / D-pad / leanback / remote navigation. Semua dukungan TV yang *sudah ada* di OpenNOW harus dipertahankan — itu tetap kerja kita.
- ⚠️ **Tablet & HP juga tidak otomatis.** Layout adaptif tetap dibuat manual (WindowSizeClass / BoxWithConstraints), tapi fondasinya **sudah ada** di codebase ini (shell adaptif `NavigationBar ↔ NavigationRail` + `BoxWithConstraints`).
- 🚨 Miuix masih **experimental** (v0.9.3, rilis 4 Juli 2026): *"APIs may change without notice"*.
- 💡 **Rekomendasi:** jangan rombak total sekaligus. Lakukan **fase bertahap** mulai dari pilot 1 screen, lalu tema → shell → screen → panel settings. Lihat bagian [7. Fase Migrasi](#7-fase-migrasi-yang-disarankan).

---

## 2. Fakta Miuix (diverifikasi dari repo & rilis resmi)

| Aspek | Fakta |
|---|---|
| Repo | `github.com/compose-miuix-ui/miuix` |
| Versi terbaru | **v0.9.3** (rilis 4 Juli 2026) |
| Status | **Experimental** — API bisa berubah tanpa pemberitahuan |
| Dibangun di atas | Kotlin 2.4.10 · Compose Multiplatform 1.11.1 |
| Platform didukung | Android, iOS, macOS, Desktop (JVM), Web (JsCanvas & WasmJs) |
| Publikasi | Maven Central: `top.yukonga.miuix.kmp:*` |

### Modul

| Modul | Fungsi | Relevansi OpenNOW |
|---|---|---|
| `miuix-ui` | Komponen UI inti | Utama |
| `miuix-preference` | Komponen list pengaturan | **Sangat relevan** — panel Settings OpenNOW penuh list item |
| `miuix-icons` | Ikon tambahan | Opsional |
| `miuix-blur` | Efek blur | Opsional (efek glassmorphism) |
| `miuix-squircle` | Bentuk squircle (sudut mulus khas MIUI) | Opsional (bisa ubah `OpenNowShapes`) |
| `miuix-nav` | Navigasi | Opsional |
| `miuix-shader` | Shader/render effect runtime | Opsional |

### Komponen yang terkonfirmasi ada (v0.9.3, dari struktur repo + release notes)
Button, Card, Checkbox, Divider, Dropdown, FloatingActionButton, FloatingToolbar, Badge / BadgedBox, BreadcrumbBar, ColorPalette, ColorPicker, Icon, Tooltip, Snackbar, NavigationBarItem, FloatingNavigationBarItem, NavigationRail (support expand/collapse sejak v0.9.3), plus modul overlay (dialog/menu/popup) & layout.

> Catatan: daftar ini dari struktur folder `miuix-ui/.../basic`. Daftar **penuh** (termasuk TextField, TopAppBar, Slider, Switch, dll.) perlu diverifikasi saat pilot — jangan dianggap lengkap.

### Cara pakai (dari README)
```kotlin
// build.gradle.kts (dependencies)
implementation("top.yukonga.miuix.kmp:miuix-ui:0.9.3")
implementation("top.yukonga.miuix.kmp:miuix-preference:0.9.3")

// Theme
val colors = if (isSystemInDarkTheme()) darkColorScheme() else lightColorScheme()
MiuixTheme(colors = colors) { ... }

// Atau dengan ThemeController (Monet dynamic color + seed color)
val controller = remember { ThemeController(ColorSchemeMode.MonetSystem, keyColor = Color(0xFF3482FF)) }
MiuixTheme(controller = controller) { ... }
```

---

## 3. Kondisi UI OpenNOW saat ini

- **Stack:** Jetpack Compose **Material3** (Compose BOM `2026.06.00`), minSdk 23, targetSdk 36, `activity-compose`, `material-icons-extended`, Coil, dll.
- **Theme:** `OpenNowTheme` (`screens/ThemeUtils.kt`) → `MaterialTheme` + palet custom `OpenNowPalette` + dynamic color + 6 pilihan accent (`UiAccent`: OpenNow / Pixel / HotPink / Lime / Coral / Violet).
- **Shell & navigasi:** `OpenNowApp` (`OpenNowScreens.kt:380`) → `Scaffold` dengan **`NavigationBar` (HP portrait)** ↔ **`NavigationRail` (TV / HP landscape)** — sudah adaptif via `tvProfile || phoneLandscapeChrome`.
- **Responsive:** sudah pakai `BoxWithConstraints` di beberapa screen (grid 2/3 kolom, layout landscape, panel dedicated).
- **Dukungan TV (sudah ada, jangan disentuh):**
  - Deteksi TV/leanback: `hasSystemFeature("android.software.leanback")` (`Streaming.kt:343`).
  - Input D-pad/remote/gamepad: `MainActivity.dispatchKeyEvent` (termasuk virtualisasi input controller → D-pad sintetis), hat navigation.
  - Fokus D-pad custom: `ui/controls/ControlRow.kt` & `ControlRows.kt`.
  - `LocalTvLoadingProfile` untuk profil loading TV.
- **Catatan:** `screens/MainAppShell.kt` saat ini masih **stub** (tidak direferensikan); shell asli ada di `OpenNowApp`.

---

## 4. Analisis per form factor — siapa yang mengerjakan apa

| Perangkat | Dikerjakan Miuix | Tetap kerja kita (tidak bisa di-delegasikan) |
|---|---|---|
| **HP** (portrait) | Tampilan visual: tombol, kartu, list, dialog, snackbar, bottom nav | — |
| **Tablet / HP landscape** | Komponen visual tetap dipakai | Layout adaptif (panel 2 kolom, rail, grid) via `WindowSizeClass` / `BoxWithConstraints` — fondasi sudah ada |
| **TV** | ❌ **Tidak didukung Miuix** | Semua yang sudah ada: fokus D-pad, navigasi remote, `tvProfile`, layar loading TV. UI TV tetap pakai komponen custom/Material3 yang ada |

**Kesimpulan:** Miuix hanya menyentuh lapisan "tampilan" di HP/tablet. Logika TV & responsive yang rumit sudah terlanjur dibangun dan tetap dipertahankan — jadi risiko terbesarnya bukan di sana, melainkan di konsistensi visual antar-perangkat (dua set komponen).

---

## 5. Perbandingan opsi

| Opsi | Pro | Kontra |
|---|---|---|
| **A. Miuix penuh (semua screen)** | Gaya HyperOS konsisten; cepat ganti visual; `miuix-preference` pas untuk Settings | Experimental; mobile-first tanpa TV; hasil sangat "Xiaomi-like"; banyak komponen hilang (harus tetap Material3/custom) |
| **B. Miuix hybrid (HP/tablet saja)** | Zona nyaman: TV tidak tersentuh; migrasi lebih kecil | Dua "bahasa visual" dalam satu app → harus dijaga konsistensinya |
| **C. Tetap Material3 + design system custom** | Stabil, matang, sudah ada fondasinya | "Rombak" = kerja manual besar-besaran; hasil tetap kelihatan Material |
| **D. Material3 Adaptive + androidx.tv** (pelengkap resmi Google) | `androidx.compose.material3.adaptive` = jalan resmi utk tablet; `androidx.tv:tv-material` & `tv-foundation` = jalan resmi utk TV Compose | Bukan "rombak visual" — hanya pelengkap responsive/TV |

> Catatan penting: **`androidx.tv:tv-material` / `tv-foundation`** adalah library resmi Google untuk Compose di Android TV. Jika ke depan mau menambah dukungan TV yang lebih baik, itu jalur yang tepat — dan **bisa dikombinasikan** dengan Miuix (Miuix untuk HP/tablet, tv-material untuk TV).

---

## 6. Pemetaan awal komponen Miuix → screen OpenNOW

| Screen / bagian OpenNOW | Komponen Miuix kandidat |
|---|---|
| Panel Settings (semua panel settings/*) | `miuix-preference` (ListItem, PreferenceGroup, dll.) — kandidat terkuat |
| LoginScreen | Button, Card, (TextField — perlu konfirmasi) |
| Top bar / judul halaman | TopAppBar / BreadcrumbBar (perlu konfirmasi nama & fitur) |
| Grid game/kartu | Card, Badge (notif/label) |
| Kontrol streaming (slider, toggle, dsb.) | komponen basic (Slider/Switch — perlu konfirmasi) |
| Dialog konfirmasi | modul `overlay` |
| Navigasi bawah (HP) / rail (tablet) | NavigationBarItem / NavigationRail |

> Pemetaan ini **pra-konfirmasi**. Verifikasi ketersediaan tiap komponen dilakukan di fase pilot.

---

## 7. Fase migrasi yang disarankan

| Fase | Isi | Output / gate |
|---|---|---|
| **0. Pilot** | Pasang dependency Miuix; buat `MiuixTheme` yang membungkus palet `OpenNowPalette` (biar warna identitas OpenNOW tetap); rombak **1 screen contoh** (mis. panel Settings atau LoginScreen) | Build + QA di HP & tablet. **Keputusan GO/NO-GO sebelum lanjut** |
| **1. Tema** | `OpenNowTheme` → bungkus `MiuixTheme`; sesuaikan shapes/typography | Seluruh app tetap jalan, warna identitas OpenNOW dipertahankan |
| **2. Shell & navigasi** | Ganti `NavigationBar`/`NavigationRail` M3 dengan Miuix di `OpenNowApp` | Navigasi HP & tablet berfungsi; TV tidak berubah |
| **3. Screen utama** | Home / Store / Library / layar stream, per screen dengan flag migrasi | Tiap screen build + QA |
| **4. Panel Settings** | Migrasi ke `miuix-preference` | Semua setting tetap tersimpan sama (logika ViewModel tak disentuh) |
| **5. QA & hardening** | QA TV (fokus D-pad/remote di tiap screen), QA tablet/HP, cek R8/proguard & ukuran APK, hapus komponen M3 yang tidak terpakai | Rilis |

**Prinsip kunci:** logika bisnis (ViewModel/domain), input TV, dan penyimpanan **tidak boleh berubah** — Miuix hanya mengganti lapisan `@Composable`. Idealnya komponen Miuix di-*wrapping* di lapisan UI sendiri supaya kalau API-nya berubah, cukup perbaiki satu tempat.

---

## 8. Risiko & mitigasi

1. **API experimental** — pin versi (`0.9.3`); isolasi komponen Miuix di lapisan UI; siapkan upgrade path bila API berubah.
2. **Gaya MIUI bentrok dengan identitas OpenNOW** — kustomisasi lewat color scheme (pakai `OpenNowPalette` sebagai sumber warna) dan shapes; atau setujui arah visual baru "HyperOS-like".
3. **Tidak ada dukungan TV** — jangan sentuh logika input TV; uji remote/D-pad di **setiap** fase; TV tetap pakai komponen custom/M3 yang ada.
4. **R8 / ProGuard & ukuran APK** (release pakai `minifyEnabled = true`) — tambahkan aturan keep bila perlu; pantau ukuran APK.
5. **Skala codebase (±14 ribu baris di OpenNowScreens.kt)** — migrasi bertahap per screen dengan flag, tiap fase wajib build + QA, hindari PR raksasa.
6. **Kompatibilitas ikon** — `material-icons-extended` tetap ada; verifikasi Miuix menerima ikon Material (atau gunakan `miuix-icons`).
7. **`MainAppShell.kt` stub** — perlu diputuskan: dihidupkan kembali sebagai shell baru, atau dihapus (biarkan `OpenNowApp` sebagai shell).

---

## 9. Rekomendasi akhir

Pendekatan terbaik untuk OpenNOW:

1. **TV: tetap seperti sekarang** — jangan rombak, jangan sentuh input/UI TV.
2. **HP & tablet: Miuix sebagai visual refresh bertahap** — mulai pilot (Fase 0), putuskan lanjut setelah melihat hasil nyata.
3. Kalau nanti butuh dukungan TV Compose yang lebih modern, kombinasikan dengan `androidx.tv:tv-material` (bukan Miuix).
4. Pertahankan identitas warna OpenNOW dengan membuat `MiuixTheme` yang memakai `OpenNowPalette` sebagai seed/scheme.

> **Jawaban singkat untuk pertanyaan awal:** "Bisa" — tetapi Miuix **bukan** solusi auto-adaptive. Yang membuat app ini adaptif di TV/tablet/HP adalah kode OpenNOW yang sudah ada, bukan library ini. Miuix hanya mengganti tampilan visualnya.

---

## 10. Referensi

- Repo Miuix: https://github.com/compose-miuix-ui/miuix
- Demo web (JsCanvas): https://compose-miuix-ui.github.io/miuix-jsCanvas/
- Maven Central: `top.yukonga.miuix.kmp:miuix-ui` (v0.9.3)
- Pelengkap resmi Google: `androidx.compose.material3.adaptive` (tablet) · `androidx.tv:tv-material` / `tv-foundation` (TV)
