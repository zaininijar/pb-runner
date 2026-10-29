# Status pengujian — 29 Oktober 2026

Target: menjalankan client PB Indonesia resmi pada Apple M1 Pro, RAM 16 GB, hingga menu/lobi dan kemudian pertandingan. Target tersebut **belum tercapai**.

## Sudah terverifikasi

- Z-Launcher resmi dapat dibuka, login, mengunduh full client dan memasang patch dengan Wine Sikarugir 10.0 revision 8.
- Rust core: tujuh tes lulus, termasuk kepemilikan prefix, penanganan argumen tanpa shell, dan penangkapan output proses turunan setelah loader keluar.
- Proses GUI native dan probe x86_64 yang dijalankannya mendeteksi Apple M1 Pro.
- Vulkan loader berhasil dimuat dan instance dibuat dari GUI; ODBC x86_64 berhasil mengalokasikan handle.
- D9VK macOS dengan MoltenVK 1.4.1: CreateDevice, Clear dan Present mengembalikan HRESULT 0.
- DXVK macOS D3D11 32-bit: CreateDevice berhasil dengan feature level 11_0.
- Snapshot tasklist mengonfirmasi PointBlank.exe masih aktif pada percobaan terbaru, tetapi pengguna hanya melihat ikon Dock.

## Perbaikan dan temuan

1. Reader output launcher harus tetap hidup sampai proses turunannya selesai. Menutup pipe saat loader keluar mengganggu unduhan updater.
2. Runtime memerlukan library pendukung dan rpath yang benar, Vulkan loader nyata, ICD MoltenVK, serta unixODBC dan libltdl x86_64.
3. Wine 10 memerlukan konfigurasi enumerasi portability pada ICD privat. Konfigurasi yang diuji memakai is_portability_driver=false sebagaimana dibahas pengembang Wine/Sikarugir. Ini hanya konfigurasi driver milik runner.
4. D9VK gagal membuat buffer dengan MoltenVK 1.4.0; probe lulus setelah memakai 1.4.1.
5. Dengan d3d9 bawaan Wine, startup sebelumnya mengalami fail-fast 0xC0000409. Setelah renderer alternatif dipakai, fail-fast itu tidak muncul pada beberapa percobaan, tetapi belum ada menu game yang terverifikasi.
6. Pesan debug [HOOK] 0 bukan bukti bahwa proses keluar. Jangan menyimpulkan penyebab kegagalan dari pesan tersebut saja.
7. Salah satu sesi berakhir karena operator menekan Hentikan. Kemunculan ikon/jendela sesaat sebelum itu tidak membuktikan crash spontan.
8. Override DLL harus memperhitungkan timestamp pada header section registry Wine; edit literal yang gagal mencocokkan header sempat membuat game tetap memakai D3D11 builtin.

## Konfigurasi percobaan terakhir

Wine Sikarugir 10.0 revision 8; Vulkan loader dan MoltenVK 1.4.1; D9VK untuk D3D9; DXVK-macOS untuk D3D11/d3d10core. Pengguna memilih DirectX 11 melalui PBConfig resmi. Override hanya untuk executable tertentu pada prefix runner. DLL C++ resmi yang sudah ada pada client dicoba sebagai native,builtin. Trace startup berat dimatikan untuk pengujian terakhir.

Wine Sikarugir 11.0 revision 1 dengan Mono 10.4.1 juga diuji pada prefix probe terpisah, tetapi wineboot --init berhenti dengan kode 1 sebelum sampel .NET. Kandidat ini belum dipakai untuk client.

Tidak ada binary game atau Cheat Blocker yang dipatch, dinonaktifkan, atau dilewati. Belum ada hasil yang membuktikan PB dapat masuk pertandingan.

## Sumber komponen

- [Wine engines Sikarugir](https://github.com/Sikarugir-App/Engines/releases/tag/v1.0)
- [D9VK macOS](https://github.com/Sikarugir-App/d9vk/releases/tag/v1.10.3-20250511)
- [DXVK macOS D3D10/11](https://github.com/Gcenx/DXVK-macOS/releases/tag/v1.10.3-20230507-repack)
- [Wine portability enumeration discussion](https://list.winehq.org/hyperkitty/list/wine-gitlab%40list.winehq.org/message/BSSP5MQNBIUHRQAWQHO3LTTG5KVXPCAR/)
- [Pengaturan DirectX resmi PB](https://www.pointblank.id/news/view?idx=6593)

Log mentah, prefix, installer, dan kredensial tetap lokal dan tidak disertakan dalam repository.
