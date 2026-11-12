# B4 db-ui

**Önerilen model: Sonnet (A6 `less-db` API'si hazır)**
**Ön koşul:** B0 bitmiş olmalı. B1–B5 birbirine paralel.
**Sahiplik:** yalnız `crates/less-app/src/panels/db/` (panel `panels()` listesine kendi klasöründe eklenir; `panels/mod.rs`e dokunma) · hedef `target/agent-b4`

**Panel sözleşmesi:** önce `less-rs/docs/handoff/B0.md` oku (Panel trait, Cx, olaylar, `panels::testing::Harness`).

## İş
- Database paneli sağ sütunda Claude/terminal yerine (#45), bağlantı formu (#46), konsol + Cmd+Enter (#47 #89).
- Sonuç/veri tablosu (#50), düzenleme + onay penceresi (#88).
- API: `less-db` (`connections`, `classifier`, `client`, `manager`, `edit`, `grid`) — `docs/DRIVERS.md`, README A6.

## Her oturumda geçerli kurallar

- Önce oku: `less-rs/docs/WAVES.md` ("Ortak kurallar"), `less-rs/docs/PLAN.md`, kök `CLAUDE.md` karar tablosu (#NN).
- Yalnız sahip olduğun crate/dosyalara yaz; başka crate gerekirse değiştirme, raporda yaz. Commit / `git add` yok.
- `CARGO_TARGET_DIR=target/agent-<ad>`; yalnız kendi crate: `cargo check|test|clippy -p <crate>`.
- GUI sürülmez (fare/klavye sentezi, ekran görüntüsü yok). Doğrulama = `cargo test` + `cargo clippy`.
- `unsafe` yok; açıklama `///` ile öğenin üstünde; kullanıcıya görünen metin İngilizce; test adında karar no.
- Bitince `less-rs/docs/handoff/<ajan>.md` notunu güncelle (durum, public API, testler, eksikler) ve
  `docs/handoff/README.md` tablosundaki satırı düzelt.
