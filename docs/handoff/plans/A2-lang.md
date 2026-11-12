# A2 lang — tamamlama

**Önerilen model: Opus** (LSP istemcisi/jdtls; kalan iş küçük, istenirse Sonnet de yeter)
**Sahiplik:** `crates/less-lang` · hedef `target/agent-a2`
**Ayrıntı:** `docs/handoff/A2.md`

## Mevcut durum
- Bitti ve bağlı: `language.rs`, `highlight/` (15 grammar), `lsp/` (jsonrpc, connection, registry, uri),
  `java.rs`, `diagnostics.rs`, `tooltip.rs`. `cargo test -p less-lang` → 32 test geçiyor (durdurulma anında).
- Ajan "tooltip assembly" yazarken durdu → `tooltip.rs` yarım olabilir.

## Adımlar
1. `cargo test -p less-lang` ile durumu doğrula.
2. `tooltip.rs`'i tamamla (#52 #70: diagnostik + hover birleşimi).
3. `highlight/tests.rs`'teki `dump` testini gerçek snapshot assert'leriyle değiştir (Java/TS/MD/SQL) + artımlı
   düzenleme testi (update ile sıfırdan parse aynı span'leri versin).
4. LSP sahte sunucu testlerini kontrol et/eksikse yaz (süreç içi pipe).
5. `cargo clippy -p less-lang` temiz; `A2.md`'yi son API ile güncelle.

## Her oturumda geçerli kurallar

- Önce oku: `less-rs/docs/WAVES.md` ("Ortak kurallar"), `less-rs/docs/PLAN.md`, kök `CLAUDE.md` karar tablosu (#NN).
- Yalnız sahip olduğun crate/dosyalara yaz; başka crate gerekirse değiştirme, raporda yaz. Commit / `git add` yok.
- `CARGO_TARGET_DIR=target/agent-<ad>`; yalnız kendi crate: `cargo check|test|clippy -p <crate>`.
- GUI sürülmez (fare/klavye sentezi, ekran görüntüsü yok). Doğrulama = `cargo test` + `cargo clippy`.
- `unsafe` yok; açıklama `///` ile öğenin üstünde; kullanıcıya görünen metin İngilizce; test adında karar no.
- Bitince `less-rs/docs/handoff/<ajan>.md` notunu güncelle (durum, public API, testler, eksikler) ve
  `docs/handoff/README.md` tablosundaki satırı düzelt.
