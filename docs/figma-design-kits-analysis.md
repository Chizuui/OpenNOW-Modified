# Analisa Design Kit Figma untuk Rombak UI OpenNOW

> **Tanggal:** 5 Agustus 2026 · **Sumber:** Figma API (token Personal Access milik akun Chizui)
> **File yang dibaca:**
> - [TV Design Kit (Community)](https://www.figma.com/design/BnR9ORzMG7VdnT0VnipUzK/TV-Design-Kit--Community-) — `BnR9ORzMG7VdnT0VnipUzK`
> - [Material 3 Design Kit (Community)](https://www.figma.com/design/oWnFmxbHzmOy5VFDdEng4s/Material-3-Design-Kit--Community-) — `oWnFmxbHzmOy5VFDdEng4s`

---

## 1. Keputusan desain (5 Agustus 2026)

> **Material 3 Design Kit = referensi redesign UI app OpenNOW (HP & tablet).**
> **TV Design Kit = referensi untuk versi TV.**

| Form factor OpenNOW | Referensi design | Catatan |
|---|---|---|
| **HP** (portrait) | **Material 3 Design Kit** — redesign utama: Buttons, Cards, Navigation, Sliders, Text fields, Sheets, dll. | Fondasi M3 sudah dipakai OpenNOW (`OpenNowTheme`); redesign = perbarui tema & komponen sesuai kit |
| **Tablet / HP landscape** | Material 3 (layout adaptive: `WindowSizeClass`/`BoxWithConstraints` + NavigationRail) | M3 punya pola resmi untuk layar lebar; kartu lebar bisa mengadopsi gaya dari TV kit bila perlu |
| **TV** | **TV Design Kit** → Immersive list, Featured Carousel, Wide cards, Long button, Dialog, Navigation drawer, Tabs | Implementasi: custom Compose / `androidx.tv:tv-material` (Miuix TIDAK support TV — lihat [ui-overhaul-miuix-analysis.md](./ui-overhaul-miuix-analysis.md)) |

**Implikasi:** dua bahasa desain dalam satu APK — M3 untuk HP/tablet, Google TV untuk TV. Pemisahannya lewat profil TV yang **sudah ada** di OpenNOW (`tvProfile`, deteksi leanback, `NavigationBar ↔ NavigationRail`).

---

## 2. TV Design Kit (Community) — bahasa desain Android TV Google

Kit resmi Google untuk UI Android TV. 3 halaman: **Getting Started**, **Styles**, **Components**.

### Styles (halaman 0:1)
| Kelompok | Isi |
|---|---|
| **Elements** | primitif visual dasar TV |
| **Layout** | `Grids` — sistem grid khas TV (margin/kolom jauh lebih lebar dari HP) |
| **Elevation** | level elevasi (untuk fokus & layering) |
| **Color** | `Tonal palettes` · `Light theme` · `Dark theme` |
| **Typography** | `Display` · `Headline` · `Android + Web` (skala tipe) |

### Komponen (halaman 22:532) + varian
| Kategori | Varian utama |
|---|---|
| **Buttons** | Filled button, Outline button, Icon button, Outline icon button, Image button, **Long button** (tombol lebar khas TV) |
| **Cards** | Standard, Classic, Compact, **Wide standard**, **Wide classic** (kartu lebar khas TV) |
| **Controls** | Radio button, Switch, Switch with icon, Checkbox, Checkbox rounded, Checkmark |
| **Tabs** | Primary, Secondary, Navigation bar, Tab row |
| **Lists** | varian list |
| **Immersive list** | list full-screen dengan focus expand (khas TV) |
| **Featured Carousel** | carousel hero (khas TV) |
| **Dialog / Modal drawer / Navigation drawer / Snackbar** | overlay & navigasi TV |
| **Text fields / Chips / Content block / Primitives / Background image** | pendukung |

### Token yang terdeteksi (dari extraction)
- **Tipografi (ukuran jauh lebih besar dari HP):**
  - Display/Title: **32px** (Roboto Medium 500) — bahkan 104px Google Sans Text Bold untuk hero
  - Subtitle: **24px** (Roboto Regular)
  - Action/Label: **22px** · Overline: **22px** · Body: 16px
- **Warna (sampling dari komponen):**
  - Primary biru: `#0B57D0` · `#00639B` (progress/bar)
  - Background gelap: `#444746` · `#282A2C` · `#131314` (keyboard)
  - Aksen terang: `#C2E7FF` · `#7FCFFF` · `#D3E3FD`
  - Surface terang: `#F2F2F2`

---

## 3. Material 3 Design Kit (Community) — bahasa desain M3

Kit resmi Google untuk Material 3. **33 halaman** — pustaka komponen M3 lengkap:

- **Fundasi:** Getting started, Table of contents, **Styles** (Light/Dark scheme, typescale, shape 1–9), Shape, Utilities, Avatars, Icons, Examples
- **Komponen:** App bars, Badges, Buttons, Cards, Carousel, Checkboxes, Chips, Date & time pickers, Dialogs, Dividers, Lists, Loading & progress, Menu, Navigation, Radio button, Search, Sheets, Sliders, Snackbar, Switch, Tabs, Text fields, Toolbars, Tooltips

Skema warna Light (`50732:11519`) & Dark (`50732:11518`) punya 5 level elevasi masing-masing.

---

## 4. Render PNG (untuk referensi visual)

Disimpan di **`docs/figma-renders/`** (diambil via Figma API, bisa dibuka di browser/file manager):

| File | Isi | Untuk |
|---|---|---|
| `tvkit_color.png` | Palet warna TV (tonal palettes + light/dark theme) | TV |
| `tvkit_typography.png` | Skala tipografi TV | TV |
| `tvkit_buttons.png` | Semua varian tombol TV (termasuk Long button) | TV |
| `tvkit_cards.png` | Varian kartu TV (Standard/Classic/Compact/Wide) | TV |
| `tvkit_controls.png` | Switch, checkbox, radio khas TV | TV |
| `tvkit_lists.png` | Komponen list | TV |
| `tvkit_immersive-list.png` | Immersive list (focus expand) | TV |
| `tvkit_featured-carousel.png` | Featured carousel hero | TV |
| `tvkit_dialog.png` | Dialog TV | TV |
| `m3_light.png` / `m3_dark.png` | Skema warna Material 3 Light & Dark | HP/tablet |

> Catatan: render M3 kit baru 2 frame (light/dark). Bisa ditambahkan render komponen M3 lain (Buttons, Cards, Navigation, dll.) dari halaman komponennya bila diperlukan.

---

## 5. Catatan keamanan

Token Figma Personal Access (`figd_...`) sudah dipakai untuk proses ini. **Disarankan revoke** setelah selesai: Figma → Settings → Security → Personal access tokens → Revoke. Simpan token tidak boleh masuk ke git/commit.
