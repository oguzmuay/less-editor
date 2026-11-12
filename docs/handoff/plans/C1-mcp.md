# C1 mcp — LESS MCP sunucusu

**Önerilen model: Opus** (güvenlik/onay kuralları #68)
**Ön koşul:** Dalga B bitmiş (araçlar panel durumuna erişir).
**Sahiplik:** `less-ai::mcp` modülü + `less-rs mcp` ikili girişi · hedef `target/agent-c1`

## İş
- stdio MCP sunucusu (`less-rs mcp`), editöre yerel soketle bağlanır (#67).
- Araçlar: editor_*, git_*, run_* (`run_set_config` dahil), db_* (okuma sınıflandırıcı + salt okunur oturum).
- Onay penceresi: DB yazma, git discard (#68); parola/URL AI'a verilmez (`ConnectionStore::public_summary()`).
- Claude / Codex / Gemini'ye bağlama: `TermTabs.mcp: Option<McpLaunch>`, `agents::gemini_settings`.

## Her oturumda geçerli kurallar

- Önce oku: `less-rs/docs/WAVES.md` ("Ortak kurallar"), `less-rs/docs/PLAN.md`, kök `CLAUDE.md` karar tablosu (#NN).
- Yalnız sahip olduğun crate/dosyalara yaz; başka crate gerekirse değiştirme, raporda yaz. Commit / `git add` yok.
- `CARGO_TARGET_DIR=target/agent-<ad>`; yalnız kendi crate: `cargo check|test|clippy -p <crate>`.
- GUI sürülmez (fare/klavye sentezi, ekran görüntüsü yok). Doğrulama = `cargo test` + `cargo clippy`.
- `unsafe` yok; açıklama `///` ile öğenin üstünde; kullanıcıya görünen metin İngilizce; test adında karar no.
- Bitince `less-rs/docs/handoff/<ajan>.md` notunu güncelle (durum, public API, testler, eksikler) ve
  `docs/handoff/README.md` tablosundaki satırı düzelt.
