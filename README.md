# PB Runner

Prototipe launcher Point Blank resmi di macOS Apple Silicon, dengan core Rust dan antarmuka SwiftUI. Runtime kompatibilitas memakai Wine mandiri; proyek ini tidak bergantung pada aplikasi Whisky.

**Status: eksperimental. Belum terverifikasi masuk menu/lobi atau pertandingan.** Launcher resmi, login, unduhan dan patch telah berjalan. Uji grafis D3D9 dan D3D11 32-bit lulus, tetapi startup game masih ditelusuri. Ikon di Dock, proses yang hidup, dan exit code 0 tidak membuktikan game dapat dimainkan.

## Isi repository

- `src/main.rs`: pengelolaan prefix Wine khusus runner, eksekusi tanpa shell, log per sesi, penghentian prefix milik runner, dan snapshot proses.
- `macos/PBRunnerApp.swift`: antarmuka launcher dan alat diagnostik.
- `diagnostics/`: source program uji kecil; tidak berisi log pengguna atau binary game.
- `scripts/build_macos.py`: membangun dan menandatangani aplikasi lokal.
- `docs/STATUS.md`: hasil pengujian dan batasan yang diketahui.

## Build

Butuh macOS Apple Silicon, Xcode Command Line Tools, Python 3, dan Rust yang mendukung edition 2024 (Rust 1.85 atau lebih baru).

```sh
cargo test --locked
python3 scripts/build_macos.py
```

Hasil berada di `dist/PB Runner.app`. Script tidak mengunduh atau menjalankan installer/game. Tanpa runtime lokal, aplikasi tetap dapat dibangun untuk memeriksa source, tetapi belum dapat menjalankan Wine.

Untuk menambahkan runtime yang sudah disiapkan:

```sh
python3 scripts/build_macos.py --runtime-root /path/to/prepared/runtime
```

Layout runtime yang dipakai: `runtime/WineSikarugir10.bundle`, beserta library pendukung di root `runtime/`. Paket upstream mentah masih membutuhkan penyiapan dependency, rpath, Vulkan loader dan ICD. Lihat `docs/STATUS.md`; script build bukan installer otomatis dependency upstream.

Konfigurasi mesin pengembang dapat disimpan di `.local.json` (diabaikan Git), dengan key `runtime_root`, `state_dir`, `installer_file`, dan `tools_dir`. Salinan lokal proyek ini sudah memiliki konfigurasi menuju bahan build dan state pengujian yang ada. Jangan jalankan dua aplikasi terhadap prefix yang sama secara bersamaan.

Tanpa `state_dir`, GUI memakai `~/Library/Application Support/PB Runner`. Installer resmi harus disediakan sendiri dari [situs PB Indonesia](https://www.pointblank.id/game/download). Persetujuan lisensi dan login dilakukan pengguna.

## CLI

```sh
cargo run -- --help
cargo run -- --state /absolute/path/to/state --wine /absolute/path/to/wine doctor
```

CLI hanya mengelola prefix yang memiliki marker kepemilikan runner. Tidak menggunakan shell untuk mengeksekusi argumen Windows. `processes` mengambil snapshot `tasklist` dari prefix tersebut tanpa mengganti penunjuk sesi game terakhir.

## GitHub

Runtime, installer, prefix Wine, data akun, hasil build dan log lokal tidak termasuk repository. Repository lokal sudah diinisialisasi; remote GitHub belum dipasang.

```sh
git remote add origin <URL_REPOSITORY_GITHUB>
git push -u origin main
```

PB Runner tidak berafiliasi dengan Zepetto, WineHQ, CodeWeavers, Sikarugir, atau Apple. Source runner berlisensi MIT; komponen pihak ketiga memiliki lisensinya masing-masing.
