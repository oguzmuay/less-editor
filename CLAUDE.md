# nvim-workbench

macOS için Ghostty + LazyVim tabanlı, mouse + klavye ile kullanılan, "agentic development" odaklı
Neovim kurulumu. Ağırlıklı diller: Java (Spring Boot), JavaScript/TypeScript; ayrıca SQL, Markdown.

Bu repo **canlı yapılandırmanın kendisidir**: `~/.config/nvim` ve `~/.config/ghostty/config.ghostty`
bu repodaki dosyalara symlink'tir. Buradaki değişiklik anında geçerli olur.

## Yapı

```
nvim/                         LazyVim config (~/.config/nvim -> buraya symlink)
  lua/config/lazy.lua         LazyVim + açık extra listesi
  lua/config/options.lua      mouse, clipboard, whichwrap, DB istemci PATH'i
  lua/config/keymaps.lua      Cmd kısayolları, ok tuşu hareketleri, sağ tık menüsü
  lua/plugins/workbench.lua   eklenti override'ları (claudecode, neo-tree, bufferline, diffview, jdtls kökü)
  lua/workbench/init.lua      panel yöneticisi: Cmd+1..7, sağ sütun, alt şerit yerleşimi
  lua/spring/init.lua         Spring Boot uygulama paneli (özel modül)
  lua/githistory/init.lua     Git geçmişi paneli (özel modül)
  KISAYOLLAR.md               kullanıcıya yönelik kısayol özeti
  lazy-lock.json              eklenti sürümleri (commit'le)
ghostty/config.ghostty        Ghostty config (~/.config/ghostty/config.ghostty -> buraya symlink)
Brewfile, install.sh          yeni makinede kurulum
```

## Kod kuralları

- Metot **içinde** uzun açıklama bloğu yazma. Açıklama gerekiyorsa fonksiyonun **üstüne** `---` docstring yaz.
- Metot içi yorum sadece gerçekten şaşırtıcı bir karar varsa ve tek satırsa kalabilir.
- Lua stili `stylua.toml`'a uyar (2 boşluk, 120 kolon).
- Kullanıcıya görünen metinler Türkçe.
- Yeni bir kısayol/panel eklenince `nvim/KISAYOLLAR.md` ve aşağıdaki karar listesi güncellenir.

## Önemli teknik bilgiler

- **Cmd tuşları:** Ghostty, Cmd+X kombinasyonlarını `csi:` action'ı ile kitty/CSI-u dizisi olarak gönderir
  (`super+1=csi:49;9u` → Neovim'de `<D-1>`; Shift eklenince mod 10 → `<D-F>` gibi büyük harf).
  Oklar xterm biçiminde: `super+arrow_left=csi:1;9D` → `<D-Left>`. Yeni bir Cmd kısayolu = Ghostty'de keybind + Neovim'de map.
  Ghostty config'i `ghostty +validate-config --config-file=...` ile doğrula.
- **Kısayollar terminal modunda da çalışmalı:** panel kısayolları `{ "n", "t", "i", "v" }` modlarında map'lenir.
- **Alt şerit:** `workbench.open_bottom(buf, opts)` ile açılır; buffer'a `vim.b.workbench_bottom = true` işaretlenir.
  Şerit yoksa `split=below, win=-1` ile tam genişlikte açılır, sonra `pin_right_column()` sağ sütunu
  (`wincmd L` + `win_splitmove`) tam boya taşır. Yeni bir alt panel eklerken bu fonksiyonu kullan.
- **Panel davranışı:** `workbench.panel(find, open, close)` → kapalıysa aç, açıksa odakla, odaktaysa kapat ve koda dön.
- **Monorepo:** Neovim repo kökünden açılır. LSP'ler kökü dosyadan bulur. jdtls kökü `plugins/workbench.lua`
  içindeki `java_root()` ile belirlenir (settings.gradle → en üstteki pom/build.gradle zinciri → mvnw/.git).
  Spring ve Git panelleri `getcwd()`'den çalışır, `LazyVim.root()`'tan değil.

## Test yöntemi

- Headless test: `nvim --headless -c "luafile test.lua"`; LazyVim kısayolları `VeryLazy` ile yüklendiği için
  testte önce `vim.api.nvim_exec_autocmds("User", { pattern = "VeryLazy" })` çağır.
- macOS'ta `timeout` yok: `perl -e 'alarm 30; exec @ARGV' nvim --headless ...` kullan.
- Ghostty'nin gönderdiği tuş dizilerinin Neovim'de hangi tuşa dönüştüğünü test etmek için Python `pty.fork()`
  ile `nvim --clean` başlatıp diziyi yaz, map'lenmiş fonksiyonun dosyaya yazdığını kontrol et.
- Pencere yerleşimi testi: `nvim_win_get_position` / genişlik / yükseklik ile sağ sütunun `row=0` ve tam boy olduğunu,
  alt şeridin sağ sütunun sol kenarında bittiğini doğrula.

## Kullanıcının aldığı kararlar

| # | Karar | Gerekçe / not |
|---|---|---|
| 1 | Terminal olarak **Ghostty** (iTerm2 ve Neovide yerine) | Config metin dosyası; Cmd tuşları Neovim'e yönlendirilebiliyor. |
| 2 | Dağıtım olarak **LazyVim** | Java/TS/SQL/Markdown/Claude Code extra'ları hazır. |
| 3 | Açık extra'lar: lang java, typescript, json, yaml, sql, markdown, docker; dap.core; test.core; editor.neo-tree; ai.claudecode; formatting.prettier; linting.eslint | |
| 4 | Dosya gezgini **neo-tree** (snacks explorer yerine) | Mouse desteği daha iyi; çift tıkla aç, Sağ/Sol ok ile aç/kapat. |
| 5 | **Cmd+1** dosyalar, **Cmd+2** kod, **Cmd+3** Claude Code, **Cmd+4** terminal, **Cmd+5** DB, **Cmd+6** Spring, **Cmd+7** Git geçmişi | IntelliJ tarzı: aç / odakla / odaktaysa kapat. `Space 1..7` karşılıkları da var. |
| 6 | Terminal ve Claude **sağda**; terminal, Claude açıksa onun altında | Sağ sütun her zaman tam boy. |
| 7 | **Spring paneli ve Git geçmişi altta yatay** (alt şerit) | Alt şerit sol panellerin altına uzanabilir, sağ sütunun altına **giremez**. Birden fazla alt panel yan yana dizilir; Spring logları panelin sağında açılır. |
| 8 | Ghostty sekme geçişi Cmd+1..9 yerine **Ctrl+Tab** | Cmd+1..9 Neovim'e verildi. |
| 9 | **Cmd+F = dosya ara**, **Cmd+Shift+F = projede metin ara**; **Cmd+P kaldırıldı** | Ghostty'nin kendi Cmd+F aramasından vazgeçildi. |
| 10 | Diğer Cmd kısayolları: Cmd+S kaydet, Cmd+E son dosyalar, Cmd+W buffer kapat, Cmd+/ yorum | |
| 11 | Hareket **ok tuşlarıyla** (hjkl kullanılmayacak) | `whichwrap` ile satır sonunda sağ/sol ok satır değiştirir. |
| 12 | **Cmd+Sol/Sağ** satır başı/sonu (Sol: önce ilk dolu karakter, tekrar basınca sütun 1), **Cmd+Yukarı/Aşağı** dosya başı/sonu, **Option+Sol/Sağ** kelime atlama | Terminalde Cmd+Sol/Sağ shell'e Ctrl+A/Ctrl+E olarak iletilir. |
| 13 | Mouse: Ctrl/Option+tık tanıma git; sağ tık menüsünde References, Rename, Code Action, Claude'a gönder | macOS Ctrl+tık'ı sağ tık yapabilir, Option+tık yedek. |
| 14 | **Monorepo** (BE + FE farklı modüllerde) kökten açılır | jdtls kökü özel `java_root()`; TS kökü lock dosyasından; Spring/Git panelleri cwd'den. FE'de lock dosyası olmalı. |
| 15 | DB arayüzü **vim-dadbod-ui** (solda) | `mysql-client` keg-only olduğu için Neovim PATH'ine eklendi. Proje bazlı bağlantılar `.lazy.lua` içinde `vim.g.dbs` (gitignore). |
| 16 | **Spring Boot paneli özel modül** | `@SpringBootApplication` taraması (rg), `mvnw`/`gradlew` öncelikli, profil seçimi, `Started ... in` ve port yakalama. |
| 17 | **Git geçmişi paneli özel modül + diffview.nvim** | Enter/çift tık commit diff'i yeni sekmede; `f` dosya geçmişi, `a` tüm dallar; `.git/logs/HEAD` izlenip otomatik yenilenir. |
| 18 | Yapılandırma bu repoda, `~/.config` altına symlink | Yedek ve yeni makine kurulumu için. |

## Açık konular / öneriler

- PostgreSQL istemcisi (`psql`) kurulu değil; Postgres kullanılacaksa `brew install libpq` (PATH'e zaten ekli).
- `application.yml/properties` tamamlama için `spring-boot.nvim` eklenebilir.
- Shift+Ok ile seçim (mac tarzı) henüz yok.
- Node.js Homebrew'dan değil `~/.local/bin` altından geliyor; Mason'ın npm tabanlı araçları için Node gerekli.
