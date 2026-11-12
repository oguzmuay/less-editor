# LESS-rs: Mimari ve Yol Haritası

## 1. Hedef

`FEATURES.md`'deki her özellik Rust'ta yeniden yazılır. Lua/Neovim bağımlılığı kalkar; LESS-rs kendi penceresini,
terminal emülasyonunu ve düzenleme motorunu getirir (Ghostty fork'u + nvim ikilisinin yerini alır).

## 2. Modlar

- **Vim modu (varsayılan):** `less-input` içinde Rust'ta yazılmış modal motor (Normal / Insert / Visual / Visual-Line /
  Command; operatörler, motion'lar, sayaç, register, makro, `.` tekrarı). Hedef Neovim'in günlük kullanılan alt kümesi;
  Vimscript/Lua yok.
- **Normal mod:** modeless, klasik editör davranışı (Shift+ok seçim, Cmd+C/V/X/Z, Cmd+A…). Aynı düzenleme komutlarını
  çağırır, yalnız giriş katmanı farklıdır.
- İki mod da `less-core`'daki aynı `Command` kümesine çevirir; mod değişince buffer durumu kaybolmaz.
- Ayar: `mode = "vim" | "normal"`; çalışırken `:Mode` ya da durum satırından değişir.

## 3. Veritabanı sürücüleri (isteğe bağlı indirme)

- Çekirdekte hiçbir DB sürücüsü yok; yalnız bağlantı kaydı, sorgu sınıflandırıcı (okuma / yazma, MCP onayı) ve
  `less-db::driver` protokolü var.
- *(Öneri, onaylanmadı: ad ve protokol ayrıntıları.)* Her sürücü ayrı bir **yürütülebilir** (`less-driver-postgres`, `-mysql`, `-sqlite`, `-mssql`, `-redis`, `-mongodb`),
  stdio üzerinden JSON-RPC ile konuşur: `connect`, `list_schemas`, `list_tables`, `query`, `columns`, `transaction`.
- Kullanıcı bir türü ilk seçtiğinde panel "Install driver?" sorar (Install / Cancel; bugünkü istemci kurma akışı gibi).
  Onaylanırsa GitHub Release'ten platforma uygun ikili indirilir, **SHA-256 doğrulanır**, `~/.local/share/less-rs/drivers/`
  altına konur. İndirme yalnız kullanıcı onayıyla başlar; sessiz indirme yok.
- Sürücü ayrı süreç olduğu için çökmesi editörü düşürmez, parola yalnız o sürece gider (AI'a verilmez).
- Okuma sorgusu salt okunur oturumda çalışır; yazma `approve` penceresi ister (bugünkü #68 kararı korunur).

## 4. Crate sınırları ve bağımlılık yönü

```
less-app → less-workbench → (less-lang, less-git, less-run, less-db, less-ai, less-workspace, less-theme)
                           → less-input → less-core
```

Alt crate'ler UI bilmez; veri ve olay üretir, `less-workbench` onları panellere bağlar. Kuralı: `less-core`
hiçbir panel crate'ine bağımlı olmaz.

## 5. Teknoloji kararları

| # | Soru | Karar / öneri (T1 ve T4 onaylandı, diğerleri öneri) |
|---|---|---|
| T1 | Arayüz katmanı: GPU'lu yerel GUI (GPUI / egui / iced) mi, terminal TUI (ratatui) mi? | **Karar: yerel GUI.** Sebep: Ghostty'nin yerini alıyoruz; resim, tooltip, yüzen pencere, tablo çizimi GUI'de çok daha rahat. TUI daha hızlı başlar ama LESS'in pencere özelliklerini (menü çubuğu, ikon, görsel) kısıtlar. Kütüphane (GPUI / egui / iced) Faz 1'de prototiple seçilir. İlk pencere (açılış ekranı) egui/eframe 0.37 ile yazıldı; bu bir seçim değil, prototip. |
| T2 | Terminal paneli (AI + shell) | `alacritty_terminal` + `portable-pty`. |
| T3 | Vurgulama | tree-sitter (grammar'lar isteğe bağlı indirilir, sürücülerdeki gibi). |
| T4 | "nvim modu" sözü | **Karar: Rust'ta kendi Vim motoru** (§2). Gerçek `nvim --embed` bağlamak "tamamen Rust" hedefine ters düşer; eklenti uyumluluğu da hedef değil. |
| T5 | Config biçimi | TOML (`~/.config/less-rs/config.toml`). Lua yok. |
| T6 | Platform | Önce macOS; Linux sonra. Windows yok (mevcut karar #39). |

## 6. Fazlar

| Faz | İçerik | Çıkış ölçütü |
|---|---|---|
| 0 | İskelet (bu klasör) | `cargo check` temiz |
| 1 | `less-core` + `less-input` + pencere + kod penceresi, tab'lar, dosya aç/kaydet, Vim/Normal mod | Dosya düzenlenip kaydedilir, iki mod çalışır |
| 2 | `less-workbench`: Files, sağ sütun, alt şerit, boyut sınırları, panel düzeni kaydı, Cmd+0..7 | Yerleşim kuralları (#6, #19, #29, #96) testle doğrulanır |
| 3 | `less-lang`: tree-sitter, LSP (jdtls, vtsls), problem göstergesi, tooltip, Cmd+F aramaları | Java/TS projesinde gd/rename/diagnostics |
| 4 | `less-git`: Git paneli, History + grafik, blame, diff tab'ı, merge ekranı | Bugünkü Git özelliklerinin tamamı |
| 5 | `less-run`: Spring/Node keşfi, loglar, debug (DAP), Variables | Spring + Node'da debug uçtan uca |
| 6 | `less-ai` + terminal: sekmeler, özet/maskot, `.less/ai`, MCP, hesaplar | Claude/Codex/Gemini/Ollama sekmeleri |
| 7 | `less-db`: sürücü protokolü + indirme, panel, veri düzenleme, konsol | Postgres + SQLite sürücüsü indirilip çalışır |
| 8 | Workspaces, projeler, autorestore, notlar, tema/font, menü, paketleme (DMG), CI | Paritede `FEATURES.md` kontrol listesi |

## 7. Taşıma kuralları

- Davranış kaynağı `nvim/` kodu ve `CLAUDE.md` karar tablosu; her özellik için ilgili karar numarası (#NN) test adında geçer.
- Her faz sonunda `FEATURES.md` maddeleri "var / yok" işaretlenir.
- Bellek içi veri biçimleri (connections.json, layouts.json, workspaces.json, `.less/*`) mümkün olduğunca aynı kalır;
  LESS (Lua) ve LESS-rs yan yana kullanılabilsin, göç tek yönlü bir içe aktarma olsun.
- Kod kuralları: metot içine uzun yorum yok, açıklama fonksiyonun üstünde `///`; arayüz metinleri İngilizce; `unsafe` yasak.
