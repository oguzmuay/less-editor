# B1 git-ui

**Önerilen model: Sonnet (A5 `less-git` API'si hazır, iş çoğunlukla UI bağlama)**
**Ön koşul:** B0 bitmiş olmalı. B1–B5 birbirine paralel.
**Sahiplik:** yalnız `crates/less-app/src/panels/git/` (panel `panels()` listesine kendi klasöründe eklenir; `panels/mod.rs`e dokunma) · hedef `target/agent-b1`

**Panel sözleşmesi:** önce `less-rs/docs/handoff/B0.md` oku (Panel trait, Cx, olaylar, `panels::testing::Harness`).

## İş
- Git paneli (Cmd+0; stage/unstage/discard, #35), Git History + grafik + Branches sütunu (#58 #72).
- Diff tab'ı (#17), yan yana diff + rollback (#85), merge ekranı (#86), blame sütunu + lens (#22).
- API: `less-git` (`status`, `watch`, `log`/`graph`, `branches`, `diff`, `compare`, `merge`, `blame`) — özet `docs/handoff/README.md` A5.

## Her oturumda geçerli kurallar

- Önce oku: `less-rs/docs/WAVES.md` ("Ortak kurallar"), `less-rs/docs/PLAN.md`, kök `CLAUDE.md` karar tablosu (#NN).
- Yalnız sahip olduğun crate/dosyalara yaz; başka crate gerekirse değiştirme, raporda yaz. Commit / `git add` yok.
- `CARGO_TARGET_DIR=target/agent-<ad>`; yalnız kendi crate: `cargo check|test|clippy -p <crate>`.
- GUI sürülmez (fare/klavye sentezi, ekran görüntüsü yok). Doğrulama = `cargo test` + `cargo clippy`.
- `unsafe` yok; açıklama `///` ile öğenin üstünde; kullanıcıya görünen metin İngilizce; test adında karar no.
- Bitince `less-rs/docs/handoff/<ajan>.md` notunu güncelle (durum, public API, testler, eksikler) ve
  `docs/handoff/README.md` tablosundaki satırı düzelt.
