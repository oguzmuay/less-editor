# C3 integrate — bütünleştirme

**Önerilen model: Opus** (tüm workspace'te kırıkları çözmek, en son ve tek başına)
**Ön koşul:** C1 ve C2 bitmiş.
**Sahiplik:** tüm `less-rs/` (artık paralel ajan yok) · hedef varsayılan `target/`

## İş
- `cargo build --release`, `cargo test`, `cargo clippy -- -D warnings` tüm workspace'te temiz.
- B3 notu: sağ tık "Send to AI" `cx.send("ai.send", text)` ile göndermeli (AI paneli dinliyor).
- B2 notları: `less-app/Cargo.toml`'a B2'nin eklediği `less-run` / `less-ai` bağımlılıklarını onayla; less-lang'den
  `JavaDebugStarter` sağlayıp run paneline `dbg.set_java_starter(..)` kancası (Spring debug attach şu an başarısız);
  çıkışta kaydedilmemiş dosya için "Save all / Discard / Cancel" sorusu host'ta (#80).
- B5 host değişiklikleri (`docs/handoff/B5.md`): `Event::Message`'ı `lang.event`'e ilet (Run panelinden gelen
  breakpoint noktaları); `Event::Leader`'ı önce `lang`'e ver (`Space c a`, `Space c r`); `less-app/Cargo.toml`'a
  `serde_json` + `lsp-types` ekle ve code action'daki toml dönüşümü `shortcut:`'ını kaldır; semantic token'ları iste ve
  çizimde bindir (#71 eşlemesi hazır); `git.blame` mesajını Git paneline ya da `Ex("GitBlame")`'e bağla.
- L notları (`docs/handoff/B5.md`): açılışta login-shell ortamını bir kez yükle (nvim launcher'ın `$SHELL -lic`
  önbelleği gibi; nvm/fnm node'u LSP, Run ve AI için); `:e!` yeniden yüklemede `lang.closed`/`lang.opened` çağır;
  LSP meşgulken yavaş `request_repaint_after` (geç gelen diagnostikler girdi olmadan çizilmiyor).
- `FEATURES.md` var/yok işaretleri; `less-rs/README.md` ve kök `CLAUDE.md` yapı bölümü güncellemesi.
- Sonra kullanıcı dener: `cd less-rs && cargo run -p less-app --release` (ya da paket).

## Her oturumda geçerli kurallar

- Önce oku: `less-rs/docs/WAVES.md` ("Ortak kurallar"), `less-rs/docs/PLAN.md`, kök `CLAUDE.md` karar tablosu (#NN).
- Yalnız sahip olduğun crate/dosyalara yaz; başka crate gerekirse değiştirme, raporda yaz. Commit / `git add` yok.
- `CARGO_TARGET_DIR=target/agent-<ad>`; yalnız kendi crate: `cargo check|test|clippy -p <crate>`.
- GUI sürülmez (fare/klavye sentezi, ekran görüntüsü yok). Doğrulama = `cargo test` + `cargo clippy`.
- `unsafe` yok; açıklama `///` ile öğenin üstünde; kullanıcıya görünen metin İngilizce; test adında karar no.
- Bitince `less-rs/docs/handoff/<ajan>.md` notunu güncelle (durum, public API, testler, eksikler) ve
  `docs/handoff/README.md` tablosundaki satırı düzelt.
