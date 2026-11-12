# B5 lang-ui

**Önerilen model: Opus (editör ile LSP olaylarının bağlanması)**
**Ön koşul:** B0 bitmiş olmalı. B1–B5 birbirine paralel.
**Sahiplik:** yalnız `crates/less-app/src/panels/lang/` (panel `panels()` listesine kendi klasöründe eklenir; `panels/mod.rs`e dokunma) · hedef `target/agent-b5`

**Panel sözleşmesi:** önce `less-rs/docs/handoff/B0.md` oku (Panel trait, Cx, olaylar, `panels::testing::Harness`).

## İş
- Sahiplik ayrıca `src/editor/lang.rs`.
- Problem sayacı + kenar işaretleri (#31), tooltip/hover (#52 #70), gd / references / rename / code action,
  oluk tıklamasıyla breakpoint (#92), sağ tık menüsü (#13).
- API: `less-lang` (`LspManager::poll`, `diagnostics`, `tooltip`) — `docs/handoff/A2.md`.

## Her oturumda geçerli kurallar

- Önce oku: `less-rs/docs/WAVES.md` ("Ortak kurallar"), `less-rs/docs/PLAN.md`, kök `CLAUDE.md` karar tablosu (#NN).
- Yalnız sahip olduğun crate/dosyalara yaz; başka crate gerekirse değiştirme, raporda yaz. Commit / `git add` yok.
- `CARGO_TARGET_DIR=target/agent-<ad>`; yalnız kendi crate: `cargo check|test|clippy -p <crate>`.
- GUI sürülmez (fare/klavye sentezi, ekran görüntüsü yok). Doğrulama = `cargo test` + `cargo clippy`.
- `unsafe` yok; açıklama `///` ile öğenin üstünde; kullanıcıya görünen metin İngilizce; test adında karar no.
- Bitince `less-rs/docs/handoff/<ajan>.md` notunu güncelle (durum, public API, testler, eksikler) ve
  `docs/handoff/README.md` tablosundaki satırı düzelt.
