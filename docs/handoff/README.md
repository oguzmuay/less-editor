# LESS-rs: oturum devri (2026-11-11)

Plan: [`../WAVES.md`](../WAVES.md). Her ajan kendi durumunu `docs/handoff/<ajan>.md` dosyasına yazar.
Hiçbir şey commit edilmedi (`less-rs/` hâlâ untracked).

## Dalga A durumu

| Ajan | Model | Durum | Not dosyası |
|---|---|---|---|
| A1 core-input (`less-core`, `less-input`) | Opus | **bitti**, 383 test, clippy `-D warnings` temiz; editor + Vim/modeless motor + ex | `A1.md` |
| A2 lang (`less-lang`) | Opus | **bitti**, 36 test, clippy `-D warnings` temiz; tam highlight snapshot'ları + süreç içi LSP protokol testleri | `A2.md` |
| A3 run (`less-run`) | Opus | **bitti**, 25 test, clippy temiz (DAP sahte adaptör, pty, kapanış testleri) | `A3.md` |
| A4 term-ai (`less-ai`) | Opus | **bitti**, 32 test, clippy temiz | `A4.md` |
| A5 git (`less-git`) | Sonnet | **bitti**, 26 test, clippy temiz | aşağıda |
| A6 db (`less-db`, `less-driver-sqlite`, `less-driver-postgres`) | Sonnet | **bitti**, 60 test, clippy temiz; protokol `docs/DRIVERS.md` | aşağıda |
| A7 shell (`less-workbench`, `less-workspace`, `less-theme`) | Sonnet | **bitti**, 57 test, clippy temiz, `less-app` derleniyor | aşağıda |

Bir `<ajan>.md` yoksa ya da "yarım" diyorsa o crate'i `cargo test -p <crate>` ile kontrol et, eksikleri aynı brief'le
(`WAVES.md` tablosu) yeni ajana tamamlat.

## Dalga B durumu

| Ajan | Model | Durum | Not dosyası |
|---|---|---|---|
| B0 host (`less-app`) | Opus | **bitti**, B0 kapsamı 40 test, clippy `-D warnings` temiz; panel sözleşmesi, düzenlenebilir editör, kısayollar, durum satırı | `B0.md` |
| B1 git-ui (`less-app/src/panels/git`) | Sonnet | **bitti**, 16 test, clippy `-D warnings` temiz; Git + History/graph/Branches, diff/compare/merge, blame özel tab'ı; editör-içi blame ve diğer host sınırları B1.md'de | `B1.md` |
| B2 run-ui (`less-app/src/panels/run`) | Opus | **bitti**, 11 test, clippy `-D warnings` temiz (kendi dosyaları); Run ağacı + butonlar + menü, log tab'ları (terminal ızgarası), ayarlar formu, `(Y/n)`, kapanış listesi, Variables + düzenleme, F7–F9 / Cmd+F8; JavaDebugStarter bağlı değil | `B2.md` |
| B3 ai-ui (`less-app/src/panels/ai`) | Opus | **bitti**, 16 test, clippy `-D warnings` temiz; AI + Terminal tabları, tür seçici, terminal render/giriş, özet + maskot, #81 açılış | `B3.md` |
| B4 db-ui (`less-app/src/panels/db`) | Sonnet | **bitti**, 17 test, clippy `-D warnings` temiz; bağlantılar, konsol + autosave, veri/sonuç/yapı tab'ları, onaylı sürücü kurulumu ve transaction düzenleme; kalan sınırlar B4.md'de | `B4.md` |
| B5 lang-ui (`less-app/src/editor/lang/`) | Opus | **bitti**, 8 test, clippy `-D warnings` temiz; LSP eşitleme, sayaç + işaretler, tooltip, gd/gr/K/]d, rename, code action, oluk breakpoint, sağ tık menüsü; host istekleri B5.md'de | `B5.md` |

B1–B5 yalnız kendi klasörlerine yazar (`src/panels/{git,run,ai,db}/`, B5 `src/editor/lang/`); kayıt her klasörün
`panels()` listesinde, `panels/mod.rs` değişmez. Sözleşme `B0.md`.

## Dalga C durumu

| Ajan | Model | Durum | Not dosyası |
|---|---|---|---|
| C1 mcp | Opus | **başlatılmadı** | `C1.md` |
| C2 extras | Sonnet | **bitti**, C2 odaklı 20 test; güncel ortak ağaçta 160 test + 1 ignored ve strict clippy temiz; Notes, autorestore/layout/session/pin, File menüsü, tema/font, sysinfo, `.app` + ZIP + DMG. Native macOS menüsü ve gerçek GUI doğrulaması eksik | `C2.md` |
| C3 integrate | Opus | **başlatılmadı; C1'i bekliyor** | `C3.md` |

## A5 git — özet

Modüller: `status` (repos, porcelain v2, stage/unstage/discard), `watch` (`RepoWatcher`, 300 ms), `log` + `graph`
(`layout(&[Commit]) -> Vec<Vec<Cell>>`, graph.lua portu), `branches` (`list`, `checkout`), `diff` (`DiffDoc`, `Jump`,
`Source::{Working,Staged,Commit}`), `compare` (`blocks`, `rollback`, Patience), `merge` (`parse`, `resolve`,
`accept_all`, `sides`, `mark_resolved`), `blame` (`blame(file, content)`, `author_lens`). Bağımlılıklar notify 8,
similar 2. Eksikler: blame tarihi UTC; histogram yerine Patience; sondaki-satır-sonu farkı görünmez.

## A7 shell — özet

- `less-workbench::layout`: `Panel`, `Layout::{press,toggle,open,close,compute,reset,clamp_sizes,show_variables…}`,
  `Effect` (Opened/Closed/Focused/Hidden/Restored), `Metrics::from_font`, limitler #96, reset #42, flex #102.
  UI pencere boyutunu bilince bir kez `reset(w,h)` / `clamp_sizes(w,h)` çağırmalı.
- `less-workbench::persist`: `Snapshot`, `LayoutStore` (layouts.json, px), `PinStore` (pins.json).
- `less-workspace`: `root`, `autorestore` (`decide`, `Instance::register`), `notes`, `session`, `sysinfo`.
- `less-theme`: `HighlightKind` (21, `from_capture`), `Palette::color/git_lane/mascot`, 14 tema (yeni paletler
  ezberden, orijinalle karşılaştırılmadı), `fonts::FONTS`.

## A6 db — özet

`less-db` modülleri: `types` (`DbType`, `value_literal`, `transaction_sql`), `connections` (`ConnectionStore`, 0600,
LESS-nvim'den salt okunur içe aktarma, `public_summary()` AI için), `classifier` (`is_read`, `classify`; tek geçişli
tarayıcı, Lua'nın `select '--'; drop` açığı kapalı), `protocol`/`serve`/`client` (`DriverClient::spawn`, zaman
aşımları, çökme = hata), `manager` (`find_driver`, `plan_install`, `install(plan, approved)` — onaysız ağ yok,
SHA-256), `edit` (`EditState` → UPDATE/INSERT, #88), `grid` (`layout`, `fit`). Sürücüler: sqlite (rusqlite bundled),
postgres (TLS yok). Eksik: MySQL/MSSQL/Redis/Mongo sürücüleri, `drivers-v*` release iş akışı, pakete sürücü gömme.
B4 db-ui ve C1 MCP bu API'yi kullanır.

## A4 term-ai — özet

Ayrıntı `A4.md`. `term` (`Terminal::spawn/feed_only`, `snapshot() -> GridSnapshot`, `generation()`, `send_key`,
`send_mouse`, seçim, scroll), `tabs` (`TabsModel`, `TermTabs::poll`), `agents` (`launch`, `McpLaunch`, `mcp_argv`,
`gemini_settings`), `mailbox` (6/10 dk sınırı), `sessions` (artımlı transcript, `message.id` tekilleştirme),
`accounts`, `summary` (maskot kareleri, `MascotClock`). Eksik: claudecode IDE entegrasyonu (WebSocket, openDiff),
MCP sunucusu (C1), kitty klavye protokolü, OSC 52.

## Güncel ortak doğrulama

2026-11-11'de B1 + B4 ve diğer Dalga B kodları aynı ağaçta ana oturum tarafından doğrulandı:
`CARGO_TARGET_DIR=target/agent-b1 cargo test -p less-app` → **127 geçti, 0 başarısız, 1 ignored**;
`cargo clippy -p less-app --all-targets -- -D warnings` → temiz. B1/B4 ajan raporlarındaki daha eski host derleme/test
hataları güncel ağaçta giderilmişti. GUI sürülmedi; görsel ve gerçek dış servis doğrulamaları ayrıca gerekli.

## Eski kullanıcı notu (A1–A3 artık tamamlandı)

Kullanıcı isteğiyle A1, A2, A3 **yarıda durduruldu** (Dalga B başlatılmadı). Not dosyaları durdurulma anından
eskidir: diskte notta yazandan fazla kod olabilir (A1 son olarak "main editor module" yazıyordu, A2 "tooltip assembly",
A3 derleniyordu ve devir notunu güncellemek üzereydi). Yeni oturumda her biri için önce
`CARGO_TARGET_DIR=target/agent-<ad> cargo check/test -p <crate>` çalıştır, dosyaları gözden geçir, notu güncelle,
sonra kalan işi aynı brief'le (`WAVES.md`) bir ajana tamamlat.

## Sonraki adım

C2 bitti; sıradaki **C1 MCP (Opus)**, ardından C3 bütünleştirme (`WAVES.md`). C3 final release build'i ve paketleri güncel kaynaklarla yeniden üretmeli.
Kural hatırlatma: GUI sürülmez (kullanıcı fare/klavyesi kullanılmaz), doğrulama `cargo test` / `clippy`.
