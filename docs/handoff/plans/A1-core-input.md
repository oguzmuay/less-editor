# A1 core-input — tamamlama

**Önerilen model: Opus** (Vim motoru + undo/seçim modeli; en büyük ve en zor kalan iş)
**Sahiplik:** `crates/less-core`, `crates/less-input` · hedef `target/agent-a1`
**Ayrıntılı tasarım:** `docs/handoff/A1.md` ("Planned design") — ona uy.

## Mevcut durum
- Diskte yazılmış ama **derlenmemiş**: `buffer.rs`, `history.rs`, `registers.rs`, `search.rs`, `command.rs`,
  `motion.rs`, `textobj.rs`, `selection.rs`, `text.rs` (~1800 satır). `lib.rs` yalnız `pub mod document;`.
- `document.rs` dokunulmaz (less-app `Document`, `LoadError`, `MAX_BYTES` kullanıyor).
- `editor.rs` yok, `less-input` boş.

## Adımlar
1. Modülleri `lib.rs`'e ekle, derleme hatalarını düzelt; dosyaları tasarıma göre gözden geçir.
2. `editor.rs`: `Editor` + `Session`, `Editor::apply(cmd, &mut Session) -> Option<AppCommand>`, motion çözümü
   (exclusive/inclusive/linewise, `cw`), text object'ler, blok imleç sınırı, scroll, seçim span'leri.
3. `less-input`: `KeyEvent` + `parse_keys`, Vim motoru (Normal/Insert/Visual/V-Line/Command, sayaç, register,
   makro, `.`), Modeless motor, `Output`/`Status`, `ex.rs` ayrıştırıcı.
4. 150+ test, `cargo test -p less-core -p less-input`, clippy temiz.

## Her oturumda geçerli kurallar

- Önce oku: `less-rs/docs/WAVES.md` ("Ortak kurallar"), `less-rs/docs/PLAN.md`, kök `CLAUDE.md` karar tablosu (#NN).
- Yalnız sahip olduğun crate/dosyalara yaz; başka crate gerekirse değiştirme, raporda yaz. Commit / `git add` yok.
- `CARGO_TARGET_DIR=target/agent-<ad>`; yalnız kendi crate: `cargo check|test|clippy -p <crate>`.
- GUI sürülmez (fare/klavye sentezi, ekran görüntüsü yok). Doğrulama = `cargo test` + `cargo clippy`.
- `unsafe` yok; açıklama `///` ile öğenin üstünde; kullanıcıya görünen metin İngilizce; test adında karar no.
- Bitince `less-rs/docs/handoff/<ajan>.md` notunu güncelle (durum, public API, testler, eksikler) ve
  `docs/handoff/README.md` tablosundaki satırı düzelt.
