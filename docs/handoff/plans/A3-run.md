# A3 run — tamamlama

**Önerilen model: Opus** (DAP sahte adaptör ve pty/sinyal testleri)
**Sahiplik:** `crates/less-run` · hedef `target/agent-a3`
**Ayrıntı:** `docs/handoff/A3.md`

## Mevcut durum
- Tüm modüller yazılmış ve bağlı: paths, model, providers, config, command, output, ports, process, runner,
  shutdown, `dap/` (protocol, types, client, adapter, manager), variables, breakpoints. Derleniyor, 19 test geçiyor.

## Adımlar (yalnız test + temizlik)
1. DAP çerçeveleme + sahte adaptör testi (TcpListener thread; initialize/attach/setBreakpoints/stackTrace/variables).
2. Shutdown testi: `sh -c 'trap … INT; sleep'` → SIGINT ile kapanma, `kill`/`kill_all` (#80).
3. variables (#91 #97) ve breakpoints testleri; komut kurulumu + run_configs testleri (#69).
4. Runner ile gerçek pty: `printf` + `(Y/n)` sorusu → `answer` (#77).
5. `cargo clippy -p less-run` temiz; `A3.md`'yi son API ile güncelle.

## Her oturumda geçerli kurallar

- Önce oku: `less-rs/docs/WAVES.md` ("Ortak kurallar"), `less-rs/docs/PLAN.md`, kök `CLAUDE.md` karar tablosu (#NN).
- Yalnız sahip olduğun crate/dosyalara yaz; başka crate gerekirse değiştirme, raporda yaz. Commit / `git add` yok.
- `CARGO_TARGET_DIR=target/agent-<ad>`; yalnız kendi crate: `cargo check|test|clippy -p <crate>`.
- GUI sürülmez (fare/klavye sentezi, ekran görüntüsü yok). Doğrulama = `cargo test` + `cargo clippy`.
- `unsafe` yok; açıklama `///` ile öğenin üstünde; kullanıcıya görünen metin İngilizce; test adında karar no.
- Bitince `less-rs/docs/handoff/<ajan>.md` notunu güncelle (durum, public API, testler, eksikler) ve
  `docs/handoff/README.md` tablosundaki satırı düzelt.
