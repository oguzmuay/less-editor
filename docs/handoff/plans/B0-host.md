# B0 host — less-app'i panel ev sahibine çevir

**Önerilen model: Opus** (tek başına, Dalga B'nin geri kalanından **önce**)
**Ön koşul:** A1 ve A2 bitmiş olmalı (`less-core`, `less-input`, `less-lang` API'leri sabit).
**Sahiplik:** `crates/less-app` (tamamı; B1–B5 başlayınca yalnız `src/panels/mod.rs` iskeleti ortak) · hedef `target/agent-b0`
**UI:** egui/eframe 0.37 (mevcut prototip, kütüphane değiştirilmez).

## İş
- `less-workbench` yerleşimiyle bölgeler (sol / kod / sağ sütun / alt şerit); pencere boyu bilinince bir kez
  `reset(w,h)` / `clamp_sizes(w,h)`.
- `Panel` trait'i + `panels/mod.rs` kayıt listesi; B1–B5 için boş yer tutucu klasörler
  (`git`, `run`, `ai`, `db`) ve `src/editor/lang.rs` yer tutucusu (B5'in).
- Düzenlenebilir kod görünümü: `less-core` + `less-input` + `less-lang` vurgulama, `less-theme` renkleri.
- Cmd kısayol yönlendirmesi (`docs/KEYMAP.md`), durum satırı.
- Panel kodu UI dışı test edilebilir kısımlara ayrılsın; `cargo test -p less-app`, clippy temiz.

## Her oturumda geçerli kurallar

- Önce oku: `less-rs/docs/WAVES.md` ("Ortak kurallar"), `less-rs/docs/PLAN.md`, kök `CLAUDE.md` karar tablosu (#NN).
- Yalnız sahip olduğun crate/dosyalara yaz; başka crate gerekirse değiştirme, raporda yaz. Commit / `git add` yok.
- `CARGO_TARGET_DIR=target/agent-<ad>`; yalnız kendi crate: `cargo check|test|clippy -p <crate>`.
- GUI sürülmez (fare/klavye sentezi, ekran görüntüsü yok). Doğrulama = `cargo test` + `cargo clippy`.
- `unsafe` yok; açıklama `///` ile öğenin üstünde; kullanıcıya görünen metin İngilizce; test adında karar no.
- Bitince `less-rs/docs/handoff/<ajan>.md` notunu güncelle (durum, public API, testler, eksikler) ve
  `docs/handoff/README.md` tablosundaki satırı düzelt.
