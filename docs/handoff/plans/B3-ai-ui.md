# B3 ai-ui

**Önerilen model: Opus (terminal render + giriş kodlaması)**
**Ön koşul:** B0 bitmiş olmalı. B1–B5 birbirine paralel.
**Sahiplik:** yalnız `crates/less-app/src/panels/ai/` (panel `panels()` listesine kendi klasöründe eklenir; `panels/mod.rs`e dokunma) · hedef `target/agent-b3`

**Panel sözleşmesi:** önce `less-rs/docs/handoff/B0.md` oku (Panel trait, Cx, olaylar, `panels::testing::Harness`).

## İş
- AI + Terminal sağ sütun (#6 #20 #64), tab çubuğu, Cmd+T tür seçici, özet tab'ı + maskot (#27 #60).
- Terminal render: `Terminal::snapshot()` yalnız `generation()` değişince; tuş `send_key`, yapıştırma `paste`,
  fare `send_mouse` (false → seçim/scroll UI'da).
- API: `less-ai` (`term`, `tabs`, `agents`, `mailbox`, `sessions`, `accounts`, `summary`) — `docs/handoff/A4.md`.

## Her oturumda geçerli kurallar

- Önce oku: `less-rs/docs/WAVES.md` ("Ortak kurallar"), `less-rs/docs/PLAN.md`, kök `CLAUDE.md` karar tablosu (#NN).
- Yalnız sahip olduğun crate/dosyalara yaz; başka crate gerekirse değiştirme, raporda yaz. Commit / `git add` yok.
- `CARGO_TARGET_DIR=target/agent-<ad>`; yalnız kendi crate: `cargo check|test|clippy -p <crate>`.
- GUI sürülmez (fare/klavye sentezi, ekran görüntüsü yok). Doğrulama = `cargo test` + `cargo clippy`.
- `unsafe` yok; açıklama `///` ile öğenin üstünde; kullanıcıya görünen metin İngilizce; test adında karar no.
- Bitince `less-rs/docs/handoff/<ajan>.md` notunu güncelle (durum, public API, testler, eksikler) ve
  `docs/handoff/README.md` tablosundaki satırı düzelt.
