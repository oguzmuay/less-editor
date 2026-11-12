# LESS-rs: Uygulama Dalgaları (alt ajan planı)

Kalan fazlar (PLAN.md §6, Faz 1–8) alt ajanlara dağıtılır. Ana oturum yalnız planlar, ajanları başlatır ve
sonuçları birleştirir. Kullanıcı her şey bitince uygulamayı kurup kendisi dener.

## Ortak kurallar (her ajan için)

- Çalışma klasörü `less-rs/`. `less-rs/` henüz commit edilmedi → **git worktree kullanılmaz**, herkes aynı ağaçta
  çalışır; bu yüzden **yalnız kendine verilen dosya/crate'lere yaz**. Başka crate'in kodunu değiştirme ihtiyacı
  doğarsa değiştirme, raporunda yaz.
- **Commit atma**, `git add` yapma.
- Derleme hedefini ayır: `CARGO_TARGET_DIR=target/agent-<ad>` (paylaşılan `target/` kilidinde beklememek için).
  Yalnız kendi crate'ini derle/test et: `cargo check -p <crate>`, `cargo test -p <crate>`, `cargo clippy -p <crate>`.
  Başka ajanın yarım crate'i workspace derlemesini kırabilir; bu senin hatan değil.
- Yeni dış bağımlılık: kararlı, en az iki haftalık bir sürüm. Cargo.lock'u herkes paylaşır; çakışma olursa
  `cargo update -p <paket>` yerine yalnız `cargo check` ile yeniden üret.
- **GUI'yi sürme**: uygulamayı açıp fare/klavye sentezleme, ekran görüntüsü alma yok (kullanıcının girişini
  ele geçiriyor). Doğrulama = `cargo test` + `cargo clippy`. Mantığı UI dışı, test edilebilir koda koy.
- Kod kuralları: `unsafe` yasak; metot içinde uzun yorum yok, açıklama öğenin üstünde `///` (Türkçe olabilir);
  metot içi yorum yalnız tek satır ve şaşırtıcı karar için. Kullanıcıya görünen her metin İngilizce. `rustfmt.toml`'a uy.
- Davranış kaynağı: `../nvim/lua/...` (Lua uygulaması) + `../CLAUDE.md` karar tablosu (#NN). Testlerin adında ilgili
  karar numarası geçsin (`fn layout_strip_under_left_panels_29()`).
- Dosya biçimleri (connections.json, layouts.json, run_configs.json, workspaces.json, `.less/*`) LESS-nvim ile aynı;
  LESS-nvim dosyalarına **yazma**, yalnız okuyup içe aktar. Testlerde `LESS_RS_*_DIR` ile geçici klasör kullan.
- Bitince kısa rapor: ne yapıldı, public API (tip/fonksiyon imzaları), testler, yapılamayanlar, başka crate'ten beklenenler.

## Dalga A — çekirdek crate'ler (paralel, UI yok)

| Ajan | Model | Sahiplik | İş |
|---|---|---|---|
| A1 core-input | Opus | `less-core`, `less-input` | Rope tabanlı buffer, undo/redo, seçim + çoklu imleç, `Command` kümesi; Vim motoru (Normal/Insert/Visual/V-Line/Command, operatör+motion, sayaç, register, makro, `.`), Normal (modeless) mod, ex komutları ayrıştırıcı |
| A2 lang | Opus | `less-lang` | tree-sitter vurgulama (Java, TS/TSX, JS, JSON, YAML, Markdown, SQL, Lua, Rust, TOML gömülü), LSP istemcisi (stdio JSON-RPC), jdtls başlatma (Java 25+ bulma, `java_root`, runtimes), vtsls, diagnostics/hover/definition/references/rename/code action modeli |
| A3 run | Opus | `less-run` | Spring/Node keşfi, süreç yönetimi (pty), ready/port yakalama, `(Y/n)` tespiti, run_configs.json, DAP istemcisi (attach, stack, scopes, variables, setVariable, evaluate), JDWP/inspector portları, kapanış (SIGINT → SIGKILL) |
| A4 term-ai | Opus | `less-ai` | Terminal emülasyonu (`alacritty_terminal` + `portable-pty`) ve render'a hazır hücre ızgarası, AI türleri (claude/codex/gemini/ollama) argv + env, Claude sessions/transcript okuyucu (token tekilleştirme), hesaplar, `.less/ai` posta kutusu + bildirim sınırı |
| A5 git | Sonnet | `less-git` | status (çoklu repo), stage/unstage/discard, log + grafik şerit hesabı, dallar + checkout, blame porcelain, unified + yan yana diff hunk'ları, rollback, merge çakışma ayrıştırma ve çözüm |
| A6 db | Sonnet | `less-db` + yeni `crates/less-driver-sqlite`, `crates/less-driver-postgres` | connections.json, sorgu sınıflandırıcı (#68), JSON-RPC sürücü protokolü, sürücü yöneticisi (indir + SHA-256 + onay kancası), SQLite ve Postgres sürücü ikilileri, tablo/veri/yapı, düzenleme → transaction SQL (#88) |
| A7 shell | Sonnet | `less-workbench`, `less-workspace`, `less-theme` | Yerleşim modeli (sol / kod / sağ sütun / alt şerit; #6 #19 #20 #29 #45 #96), panel toggle mantığı (aç/odakla/kapat), layouts.json (#99), autorestore "son" kaydı, notlar deposu (#98), pinler, temalar (Catppuccin, GitHub, One Dark, Kanagawa, Gruvbox, Rosé Pine, Nightfox…) |

## Dalga B — arayüz (less-app, egui)

T1 için **egui/eframe 0.37** ile devam (mevcut prototip; kütüphane değiştirmek her şeyi geciktirir).

- **B0 host (Opus, tek başına, önce):** `less-app`'i panel ev sahibine çevirir: `less-workbench` yerleşimiyle bölgeler,
  `Panel` trait'i + `panels/mod.rs` kayıt listesi, düzenlenebilir kod görünümü (`less-core` + `less-input` + `less-lang`
  vurgulama), Cmd kısayol yönlendirmesi, durum satırı. Diğer paneller için boş yer tutucu dosyalar.
- **B0'dan sonra paralel**, her biri yalnız kendi `src/panels/<ad>/` klasörü + `panels/mod.rs`'te kendi satırı:
  - B1 git-ui (Sonnet): Git paneli, Git History + grafik + Branches, diff tab'ı, yan yana diff, merge ekranı, blame.
  - B2 run-ui (Opus): Run ağacı, log tab'ları (terminal ızgarası), butonlar, ayarlar formu, Variables, debug tuşları.
  - B3 ai-ui (Opus): AI + Terminal sağ sütun, tab çubuğu, tür seçici, özet tab'ı + maskot, terminal render/giriş.
  - B4 db-ui (Sonnet): Database paneli, bağlantı formu, konsol, sonuç/veri tablosu, düzenleme + onay penceresi.
  - B5 lang-ui (Opus): problem sayacı + kenar işaretleri, tooltip/hover, gd/references/rename/code action, oluk
    breakpoint'i, sağ tık menüsü (`src/editor/lang.rs` dosyasına sahip).

## Dalga C — kalanlar ve bütünleştirme

- C1 mcp (Opus): `less-ai::mcp` LESS MCP sunucusu (ikili `less-rs mcp`, editöre yerel soket), araçlar + onay (#67, #68).
- C2 extras (Sonnet): notlar penceresi (Cmd+N), autorestore + panel düzeni uygulama, menü çubuğu, tema/font seçici, durum
  satırı sysinfo, `.app` paketleme + DMG betiği (`less-rs/scripts/`), `make`/README güncellemesi.
- C3 integrate (Opus): tüm workspace `cargo build --release`, `cargo test`, `cargo clippy -- -D warnings` temizliği,
  FEATURES.md var/yok işaretleri, `less-rs/README.md` ve `../CLAUDE.md` yapı bölümü güncellemesi.

Sonra kullanıcı: `cd less-rs && cargo run -p less-app --release` (ya da paket) ile dener.
