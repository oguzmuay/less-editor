# B2 run-ui

**Önerilen model: Opus (pty log ızgarası + debug akışı, A3 API'si geniş)**
**Ön koşul:** B0 bitmiş olmalı. B1–B5 birbirine paralel.
**Sahiplik:** yalnız `crates/less-app/src/panels/run/` (panel `panels()` listesine kendi klasöründe eklenir; `panels/mod.rs`e dokunma) · hedef `target/agent-b2`

**Panel sözleşmesi:** önce `less-rs/docs/handoff/B0.md` oku (Panel trait, Cx, olaylar, `panels::testing::Harness`).

## İş
- Run ağacı, log tab'ları (terminal ızgarası, #101), satır sonu butonları / sağ tık (#34), ayarlar formu (#69).
- Variables tablosu ve düzenleme (#91 #93 #97), debug tuşları F7/F8/F9/Shift+F8, Cmd+F8 (#92 #95), son hal (#94).
- API: `less-run` (`Runner`, `RunEvent`, `dap::DebugManager`, `variables`, `shutdown`); ızgara için `less-ai::term`.

## Her oturumda geçerli kurallar

- Önce oku: `less-rs/docs/WAVES.md` ("Ortak kurallar"), `less-rs/docs/PLAN.md`, kök `CLAUDE.md` karar tablosu (#NN).
- Yalnız sahip olduğun crate/dosyalara yaz; başka crate gerekirse değiştirme, raporda yaz. Commit / `git add` yok.
- `CARGO_TARGET_DIR=target/agent-<ad>`; yalnız kendi crate: `cargo check|test|clippy -p <crate>`.
- GUI sürülmez (fare/klavye sentezi, ekran görüntüsü yok). Doğrulama = `cargo test` + `cargo clippy`.
- `unsafe` yok; açıklama `///` ile öğenin üstünde; kullanıcıya görünen metin İngilizce; test adında karar no.
- Bitince `less-rs/docs/handoff/<ajan>.md` notunu güncelle (durum, public API, testler, eksikler) ve
  `docs/handoff/README.md` tablosundaki satırı düzelt.
