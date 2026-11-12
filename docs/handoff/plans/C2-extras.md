# C2 extras — kalan özellikler + paketleme

**Önerilen model: Sonnet** (dağınık ama iyi tanımlı işler; çekirdek API'ler hazır)
**Ön koşul:** Dalga B bitmiş.
**Sahiplik:** ilgili `less-app` dosyaları (B ajanlarının klasörleri hariç), `less-rs/scripts/`, README · hedef `target/agent-c2`

## İş
- Notlar penceresi Cmd+N (#98; depo `less-workspace::notes`).
- Autorestore + panel düzeni uygulama (#32 #66 #99; `less-workspace::autorestore`, `less-workbench::persist`).
- Menü çubuğu (#61), tema/font seçici (#36 #71) (durum satırı sysinfo ajan HS ile yapıldı).
- B3 notu: #99 kayıtlı düzen gelince #81 isteğini AI panelinden (`panels/ai`, ilk `tick`) kaldır ya da `autostart`'ı kapat.
- `.app` paketleme + DMG betiği (`less-rs/scripts/`), `make` / README güncellemesi.

## Her oturumda geçerli kurallar

- Önce oku: `less-rs/docs/WAVES.md` ("Ortak kurallar"), `less-rs/docs/PLAN.md`, kök `CLAUDE.md` karar tablosu (#NN).
- Yalnız sahip olduğun crate/dosyalara yaz; başka crate gerekirse değiştirme, raporda yaz. Commit / `git add` yok.
- `CARGO_TARGET_DIR=target/agent-<ad>`; yalnız kendi crate: `cargo check|test|clippy -p <crate>`.
- GUI sürülmez (fare/klavye sentezi, ekran görüntüsü yok). Doğrulama = `cargo test` + `cargo clippy`.
- `unsafe` yok; açıklama `///` ile öğenin üstünde; kullanıcıya görünen metin İngilizce; test adında karar no.
- Bitince `less-rs/docs/handoff/<ajan>.md` notunu güncelle (durum, public API, testler, eksikler) ve
  `docs/handoff/README.md` tablosundaki satırı düzelt.
