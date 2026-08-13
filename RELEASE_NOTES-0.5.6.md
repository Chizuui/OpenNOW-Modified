OpenNOW 0.5.6-nightly

Release ini fokus ke stabilitas streaming: jitter buffer adaptif, watchdog freeze, recording yang tidak mengganggu jalur live, dan bitrate yang benar-benar naik sesuai setting.

Kategori | Fitur | Deskripsi
Native Streamer | Recording hardware encoder | Recording kini memakai hardware encoder (D3D12, Quick Sync, Media Foundation di Windows; VAAPI/NVENC di Linux; VideoToolbox di macOS) dengan fallback otomatis ke software x264 jika device tidak tersedia. Record 1080p60 tidak lagi membebani CPU.
Native Streamer | Isolasi worker thread | Recording berjalan di worker thread terpisah, terisolasi dari jalur live. Recording yang bermasalah tidak akan memblokir input, surface, atau perintah bitrate.
Native Streamer | Fix race sticky-event | Memperbaiki race sticky-event pada rebuild branch recording (video dan audio) sehingga kasus record yang dulu membuat input ikut macet tidak terjadi lagi.
Native Streamer | Warna recording | Warna recording sudah benar: skala full ke limited (16-235) dengan LUT, output H.264 universal dengan audio AAC, MP4 faststart yang bisa di-seek tanpa glitch.
Native Streamer | Audio game | Audio game ikut terekam (remux/transcode) ke dalam file yang sama.
Native Streamer | TWCC feedback | Feedback transport-cc (TWCC) kini dipaksa periodik 100 ms sehingga estimasi bandwidth server tidak buta, ditambah observability RTCP di log NetworkHealth.
Native Streamer | Decode & input | Decode hardware D3D12/D3D11 dengan auto-fallback, dukungan H.265/H.264/AV1, input native (keyboard, mouse, gamepad), mikrofon, dan relay clipboard.
Stats HUD | Real-time | Ping real-time, decode time dengan median window, dan jitter yang dihaluskan.
Stats HUD | GPU & region | Menampilkan GPU server (bukan GPU lokal) dan region ala app resmi, contohnya "Japan (NP-TYO-01)".
Stats HUD | JitterBuf | Menampilkan metrik jitter buffer pre-decode (JitterBuf).
WebRTC | Freeze watchdog | Deteksi decode freeze dalam waktu kurang dari satu detik dan langsung meminta keyframe.
WebRTC | PLI otomatis | Permintaan keyframe otomatis (PLI) saat packet loss konsisten di atas 2 persen.
WebRTC | RED audio | Redundansi audio RED sehingga kehilangan satu paket RTP tidak memutus audio.
WebRTC | Jitter buffer adaptif | Jitter buffer adaptif dengan floor berdasarkan packet loss.
WebRTC | Diagnostik BWE | Peringatan otomatis saat estimasi bitrate penerima stuck di bawah 4000 kbps pada link yang sehat, lengkap dengan pengecekan negosiasi transport-cc.
WebRTC | Bitrate mid-session | Perubahan bitrate mid-session dikirim ulang ke server tanpa perlu reconnect penuh (WebRTC dan native), dengan pencatatan push terkirim versus perubahan bitrate yang terverifikasi.
UI dan Lainnya | Shell desktop | Desktop shell ala GFN: genre browser, pencarian di navbar, title bar kustom.
UI dan Lainnya | Overlay | Stats overlay yang didesain ulang plus kontrol recording di quick menu.
UI dan Lainnya | Login Chizui | Perbaikan login Chizui: callback, lookup subscription, dan revoke.
Build | build.bat | Mendeteksi GStreamer SDK secara otomatis (membutuhkan pkg-config dan gstreamer-1.0.pc), menyimpan konfigurasi path ke build-config.bat, dan mengecek node_modules sebelum build.

Catatan: exe native dibangun melalui "build.bat gstreamer" atau "npm run native:build" di folder opennow-stable.
