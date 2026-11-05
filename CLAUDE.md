# LESS editor (nvim-workbench)

**LESS**: sade, IDEA gibi her yerinden bir şey çıkmayan bir editör. Ghostty fork'u (`LESS.app`) + bu reponun
Neovim yapılandırması. Repo: `git@github.com:oguzmuay/less-editor.git` (`main`).

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
  lua/plugins/workbench.lua   eklenti override'ları (claudecode, neo-tree, bufferline)
  lua/plugins/java.lua        jdtls: java_root(), Java 25+ ile başlatma, runtimes, bundle filtresi
  lua/workbench/init.lua      panel yöneticisi: Cmd+0..7, sağ sütun, alt şerit yerleşimi, M.toggle(name)
  lua/workbench/commands.lua  :Files, :Code, :Claude, :Terminal, :Datasource, :Run, :Git, :GitHistory komutları
  lua/runner/init.lua         Run paneli: ağaç, aç/kapat, job/log yönetimi (özel modül)
  lua/runner/providers/       Run sağlayıcıları: spring.lua, node.lua (yeni kaynak = discover(root) döndüren dosya),
                              jdwp.gradle (Gradle bootRun'a JDWP ekleyen init script)
  lua/runner/dap.lua          nvim-dap köprüsü: boş port, runnerId etiketli oturumlar, wait(), detach
  lua/githistory/init.lua     Git History paneli (alt şerit, özel modül); repos() gitstatus tarafından da kullanılır
  lua/gitstatus/init.lua      Git paneli (sol): commit edilmemiş dosyalar, stage/unstage/discard
  lua/gitdiff/init.lua        diff'i kod editöründe tab olarak açar: open({ path?, rev?, staged?, cwd? })
  lua/gitblame/init.lua       :GitBlame sütunu + tanım satırı sonunda yazar lens'i (özel modül)
  lua/termtabs/init.lua       Claude + Terminal panellerinde tablar: gruplar, winbar tab çubuğu, geçiş, rename, komutlar
  lua/termtabs/claude.lua     claudecode.nvim özel terminal sağlayıcısı, new_session(), at_mentioned yönlendirmesi
  lua/termtabs/sessions.lua   ~/.claude/sessions/<pid>.json + transcript JSONL okuyucu (artımlı)
  lua/termtabs/summary.lua    Claude özet tab'ı
  lua/workspaces/init.lua     Workspaces (çoklu kök): kayıt, symlink klasörü, aç/kapat, oturum, :Workspace
  lua/workspaces/neotree.lua  neo-tree yamaları (reveal, çoklu repo git durumu)
  lua/workspaces/ui.lua       workspace seçici ve yönetim menüleri
  lua/plugins/workspaces.lua  workspaces setup (virtual lazy plugin), neo-tree/snacks override'ları, dashboard `w`
  lua/autorestore/init.lua    açılışta son proje/workspace'i açma, "son" kaydı, varsayılan panel düzeni
  lua/plugins/autorestore.lua autorestore setup (virtual lazy plugin), `Space q h` dashboard, NVIM_SESSIONS_DIR override
  lua/plugins/scrollbar.lua   nvim-scrollview (sağ kenar işaretleri) + `problems` virtual plugin
  lua/problems/init.lua       kod penceresi winbar'ında problem sayacı + ↑↓, jump(), is_code_window()
  lua/plugins/dashboard.lua   LESS açılış ekranı (başlık, slogan, girişlerin tek kaynağı; `w` buradan)
  lua/firstrun/init.lua       LESS ilk açılış ekranı: eklenti ilerlemesi, Mason/LSP kurulum bildirimi
  lua/previewtab/init.lua     VS Code tarzı önizleme tab'ı: open(path, open_fn, keep), get(), promote(buf), neotree()
  lua/plugins/previewtab.lua  previewtab setup (virtual plugin; bufferline opts'ta yamaları kurar)
  lua/font/init.lua           :Font — Ghostty fontunu seçer (~/.config/ghostty/font.ghostty) + reload
  lua/plugins/font.lua        font setup (virtual lazy plugin)
  KISAYOLLAR.md               kullanıcıya yönelik kısayol özeti
  lazy-lock.json              eklenti sürümleri (commit'le)
ghostty/config.ghostty        Ghostty config (~/.config/ghostty/config.ghostty -> buraya symlink)
Brewfile, install.sh          yeni makinede kurulum
Makefile                      LESS: make check / app / run / install / uninstall / clean
app/ghostty/                  Ghostty v1.4.0 (git subtree --squash; upstream'e dokunuşlar app/PATCHES.md'de)
app/less/                     defaults.ghostty, launch (nvim başlatıcı), icon.svg
app/scripts/brand.sh          Ghostty derlemesini build/LESS.app'e çevirir (ad, bundle id, ikon, imza)
app/scripts/lessed            `lessed [klasör|dosya]` CLI
app/PATCHES.md                değiştirilen upstream dosyaları
app/scripts/package-macos.sh  zip + DMG (LESS.app, lessed, install.sh, README.txt); package-linux.sh tarball
app/less/linux/               Linux sarmalayıcı (bin/less-editor), desktop dosyası, install.sh, linux.ghostty
.github/workflows/release.yml `v*` tag → macOS universal (+ deneysel Linux) derle, GitHub Release yayınla
.github/workflows/ci.yml      push/PR: lua + sh sözdizimi, actionlint
```

## LESS uygulaması (Ghostty fork'u)

- **Kimlik:** ad `LESS`, `LESS.app`, bundle id `com.oguz-dev.less-editor`, CLI `lessed` (asla `less` — Unix pager),
  sürüm 0.1.0. Ghostty adı/logosu markada kullanılmaz; About'ta ve pakette (LICENSE) Ghostty'ye atıf (MIT).
- **Upstream:** `app/ghostty` = Ghostty v1.4.0 subtree. Güncelleme:
  `git subtree pull --prefix=app/ghostty https://github.com/ghostty-org/ghostty <tag> --squash`. Upstream'e dokunuş
  minimum tutulur; **tam liste `app/PATCHES.md`'de** (subtree pull'da çakışma bu dosyalarda çıkar). Özet:
  `src/config/file_load.zig` (XDG alt klasörü `less/`), `src/config/Config.zig` + `config-template` (ilk açılış şablonu
  `~/.config/less/config.ghostty`'ye, LESS metniyle), `src/os/macos.zig` (App Support/Caches kimliği
  `com.oguz-dev.less-editor`; eskiden sabit `com.mitchellh.ghostty` olduğu için LESS kullanıcının gerçek Ghostty
  config'ini yüklüyordu), `Ghostty.Config.swift` (önce `Resources/less/defaults.ghostty`), `AboutView.swift`,
  `MainMenu.xib` (sade menüler), 6 pencere xib'i + 2 TitlebarTabs swift (ilk başlık `LESS`), `AppDelegate.swift`,
  `UntrustedURLAlert.swift`, `IntentPermission.swift`, `GhosttyIntentError.swift` (görünen uyarı metinleri).
- **Post-build (`brand.sh`):** çalıştırılabilir `Contents/MacOS/LESS` (`CFBundleExecutable`; süreç adı LESS),
  sdef başlık/açıklamaları ve Info.plist "Surface Identifier" metni LESS, entitlements imzada korunur.
  Kalan Ghostty metinleri bilerek: iç kimlikler (Swift sınıfları, `com.mitchellh.ghostty.*`, sdef kodları,
  `TERM_PROGRAM=ghostty`, `xterm-ghostty`, `GHOSTTY_*`), CLI `+help`/`--docs`, atıf (About, copyright, `GHOSTTY-LICENSE`).
- **`lessed` (tek örnek):** LESS çalışmıyorsa `open -a LESS.app --args --initial-window=false`; her çağrıda
  AppleScript `new window with configuration {initial working directory, environment variables:{"LESS_OPEN=…"}}`
  (hazır olana kadar 10×0,5 sn). `LESS_OPEN` yalnız o pencerede (Ghostty yeni pencereye env aktarmaz; launcher okuyup
  `unset` eder). İlk kullanımda macOS terminal uygulaması için Automation izni sorar.
- **Varsayılanlar:** derlemede `ghostty/config.ghostty`'den (config-file satırları hariç) + LESS ekleri
  (`app/less/defaults.ghostty`: `command` = launcher, `quit-after-last-window-closed`, `auto-update = off`,
  Cmd+D/Cmd+Shift+D kapalı, `config-file = ?~/.config/less/font.ghostty`). Cmd kısayolları tek kaynaktan gelir.
- **Launcher (`app/less/launch`, `#!/bin/zsh -f`):** `LESS_APP=1`, `NVIM_APPNAME=less/nvim`, `LESS_BIN=MacOS/LESS`.
  Kullanıcı ortamı önbellekten: `env -i` temiz tabandan bir kez `$SHELL -lic`, yalnız rc'lerin koyduğu değişkenler
  `$TMPDIR/less-env-$UID`'e (0600, sahiplik kontrolü); rc dosyası daha yeniyse ya da 24 saati geçtiyse yenilenir.
  Pencere başı 1,0–1,8 sn → 12–17 ms. Pencereye özgü değişkenler (PWD, TERM*, GHOSTTY_*, LESS_*, NVIM_APPNAME)
  ezilmez. rc'nin dışarıdan source ettiği dosyalar en geç 24 saatte yansır. `XDG_CONFIG_HOME` bilerek değiştirilmez (git/lazygit'e sızardı).
- **Konumlar:** Ghostty tarafı `~/.config/less/config.ghostty`, `font.ghostty`; Neovim `~/.config/less/nvim`,
  `~/.local/share|state/less/nvim`, `~/.cache/less/nvim`. Normal Ghostty/nvim kurulumuna dokunulmaz. `make install`
  `~/.config/less/nvim` yoksa repodaki `nvim/`'e symlink yapar (geliştirme modu); yoksa ilk açılışta pakete gömülü
  config kopyalanır (kullanıcıya ait, uygulama güncellenince kendiliğinden güncellenmez). İlk açılışta lazy/treesitter/
  Mason LESS'in veri klasörüne yeniden iner (~200 MB + ~580 MB).
- **nvim içinde LESS farkı** yalnız `LESS_APP=1` iken: `font/init.lua` hedef dosya `~/.config/less/font.ghostty`,
  reload `tell application "LESS"`, `LESS_BIN +list-fonts`; `options.lua` `title`.
- **Ön koşullar:** Zig 0.17.x (Homebrew), tam Xcode, Xcode Metal Toolchain (`xcodebuild -downloadComponent
  MetalToolchain`). İmza ad-hoc, notarization yok. xcodebuild'in DVT/CoreSimulator uyarıları ve "failed command"
  satırı zararsız (çıkış 0).

- **Markalama (nvim):** dashboard başlığı LESS wordmark + "less noise, more code", altta yalnız açılış süresi;
  "Lazy Extras" girdisi yok, "Lazy" → "Plugins"; `Space l` "Plugins", `Space L` (LazyVim Changelog) silindi,
  `news.lazyvim = false`. Dashboard girişleri yalnız `plugins/dashboard.lua`'da (workspaces.lua artık eklemez).
  Kalanlar: LazyVim uyarı başlıkları, `:Lazy` ekranı, `:checkhealth`.
- **İlk açılış (`firstrun`):** tespit = veri klasöründe `lazy/LazyVim` yok (yarım kurulum da sayılır); kurulu
  sistemde maliyet tek `fs_stat`, modül yüklenmez. Ortada "Setting up LESS" penceresi; lazy'nin ekranı bastırılır
  (`lazy.view.show` + görev listesi iç API'si — **lazy güncellenince kontrol et**), "Installing plugins n/N" + çubuk.
  Sonra dashboard; Mason registry önce hazırlanır, sonra lspconfig (yoksa LSP sunucuları kurulmuyordu); tek bildirim
  "Installing language tools (n/N)…" → "LESS is ready." / "Run :Mason to retry."; mason-lspconfig bildirimleri
  ilk açılışta susturulur. markdown-preview build terminali gizlenir (dashboard'u engelliyordu). Ağ hatası: lazy.nvim
  inmezse açıklama + çıkış 1; eklenti inmezse lazy ayrıntısı + "Could not install N plugin(s)…".
  **Dikkat:** geliştirme modunda (config repoya symlink) ağsız ilk açılış lazy'nin kendisi yüzünden repodaki
  `lazy-lock.json`'u eksik eklentiyle yeniden yazar — böyle olursa `git checkout nvim/lazy-lock.json`.

- **Sürüm hattı:** `git tag vX.Y.Z && git push origin vX.Y.Z` → release. Sürüm: `LESS_VERSION` → son `v*` tag →
  `0.1.0` (`brand.sh`). `-` içeren sürüm prerelease. `workflow_dispatch` yalnız artifact üretir.
  - macOS: runner `xcode-27` (public preview; Ghostty 1.4 Xcode 27.1 ister, `macos-26`'da yalnız 26.x), Zig 0.17.1
    (`mlugg/setup-zig@v2`), Metal Toolchain, `make app XCFW=universal` → `LESS-<ver>-macos-universal.zip/.dmg`.
    Ad-hoc imza; `APPLE_CERTIFICATE_P12`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`,
    `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD` secret'ları varsa Developer ID imza + notarytool + staple.
  - Linux (deneysel, `continue-on-error`, `ubuntu-26.04` + `-arm`): Nix değil apt (Nix çıktısı dağıtılamaz);
    blueprint-compiler ≥ 0.20 kaynaktan; upstream'e ek yama yok, kimlik `class = com.oguz-dev.less-editor`,
    `bin/less-editor` LESS varsayılanlarını `--config-file` ile verir; kurulum `~/.local/opt/less-editor`. Release yalnız
    macOS başarılıysa çıkar; Linux düşerse notlara yazılır.
  - Windows yok: Ghostty'nin Windows sürümü yok; config-only paket dürüst olmazdı (Cmd→CSI, zsh launcher, osascript).
  - Yerelde: `make dist` (arm64 zip + DMG `build/dist/`); `XCFW ?= native`.

## Kod kuralları

- Metot **içinde** uzun açıklama bloğu yazma. Açıklama gerekiyorsa fonksiyonun **üstüne** `---` docstring yaz.
- Metot içi yorum sadece gerçekten şaşırtıcı bir karar varsa ve tek satırsa kalabilir.
- Lua stili `stylua.toml`'a uyar (2 boşluk, 120 kolon).
- **Editör arayüzü tamamen İngilizce:** kullanıcıya görünen her metin (bildirim, prompt, menü/seçenek, panel başlığı,
  which-key `desc`, komut açıklaması, winbar) İngilizce. Kod içi docstring/yorumlar Türkçe olabilir.
  Dokümanlar (`CLAUDE.md`, `KISAYOLLAR.md`) Türkçe kalır.
- Yeni bir kısayol/panel eklenince `nvim/KISAYOLLAR.md` ve aşağıdaki karar listesi güncellenir.

## Önemli teknik bilgiler

- **Cmd tuşları:** Ghostty, Cmd+X kombinasyonlarını `csi:` action'ı ile kitty/CSI-u dizisi olarak gönderir
  (`super+1=csi:49;9u` → Neovim'de `<D-1>`; Shift eklenince mod 10 → `<D-F>` gibi büyük harf).
  Oklar xterm biçiminde: `super+arrow_left=csi:1;9D` → `<D-Left>`. Yeni bir Cmd kısayolu = Ghostty'de keybind + Neovim'de map.
  Ghostty config'i `ghostty +validate-config --config-file=...` ile doğrula.
- **Kısayollar terminal modunda da çalışmalı:** panel kısayolları `{ "n", "t", "i", "v" }` modlarında map'lenir.
- **Alt şerit:** `workbench.open_bottom(buf, opts)` ile açılır; buffer'a `vim.b.workbench_bottom = true` işaretlenir.
  Şeritte gösterilecek **her** buffer (ör. sonradan `nvim_win_set_buf` ile konan log buffer'ları) bu işareti taşımalı,
  yoksa şerit pencereyi tanımaz. Cmd+6/Cmd+7 `exclusive_bottom` ile açılır: önce şeritteki her şey gizlenir.
- **Genel yerleşim (`workbench.arrange_layout()`):** Hedef düzen, alt şerit varken
  `row[ col[ ana alan (sol paneller + kod), alt şerit ], col[Claude, terminal] ]`; şerit yokken `row[..., sağ sütun]`.
  Yani sağ sütun her zaman tam boy, alt şerit sol panellerin (Files, DB, ileride gelecek sol paneller) altına uzanır,
  sonradan açılan sol panel şeridin üstüne çıkamaz.
  - `layout_ok()` `vim.fn.winlayout()` ile kontrol eder; düzen doğruysa dokunmaz, sadece boyutları hatırlar
    (`state.sizes`, `remember_sizes`).
  - Düzeltme: şeridin ilk penceresi `wincmd J`, diğerleri `win_splitmove` ile sağına; sağ sütun `wincmd L`, terminal
    Claude'un altına. Taşımalar `noautocmd`, `arranging` bayrağı yeniden girişi engeller, odak geri döner.
  - Sol paneller ayrıca tanınmaz: şerit (`vim.b.workbench_bottom`) ve sağ sütun (`vim.b.termtabs`) dışındaki her pencere
    ana alandadır.
  - Boyutlar: `winfixwidth` pencereler (neo-tree 34, DBUI 40), şerit yüksekliği, şeritte son pencere hariç genişlikler,
    sağ sütun genişliği (`max(40, …)`) ve yükseklikleri korunur; daralmayı şeridin son penceresi (log) alır.
  - Tetikleyiciler: `BufWinEnter`/`TermOpen`/`WinNew`/`WinResized` → `schedule_arrange()` (tek `vim.schedule` ile
    birleşir); `files()`, `db()` ve `open_bottom()` eşzamanlı çağırır (titreme yok). `Space e` gibi başka yollar bir
    sonraki döngüde düzeltilir. `arrange_right_column` geriye uyumlu takma addır (termtabs onu çağırır).
- **Şerit koruması (`guard_strip`, BufWinEnter):** `open_bottom` pencereye de `w:workbench_bottom` koyar
  (`mark_bottom`). Yüzen olmayan `noice` penceresi (uzun mesaj split'i, `:Noice`) şeride alınır: işaretlenir,
  `winfixwidth` kapatılır, şeritteki diğer araçlar gizlenir (`hide_strip`, `exclusive_bottom` ile ortak), odak koda
  döner; kapanınca önceki araç geri gelmez. İşaretli şerit penceresine işaretsiz buffer gelirse buffer kod penceresine
  taşınır, şerit penceresi kapanır. Kök neden: noice `botright split` işaretsizdi → ana alan sayılıp Files'ın altına
  uzanıyordu, neo-tree dosyayı ona açıyordu (kopya kod penceresi); nui split'lerin `winfixwidth`'i sağ sütunu 5 sütuna
  çökertiyordu. `save_sizes` sağ sütunu hatırlanan değerden alır; geri yüklerken sağ sütun dışındaki `winfixwidth`
  geçici kapatılır.
- **Kod penceresi korunur (`workbench.close_window(cmd, bang, args)`):** sekmedeki son kod penceresi (tek pencere
  kalsa bile) `:q`/`:close`/`:wq`/`:x`/`:exit` (+ kısaltma, `!`) ile kapanmaz: `CmdlineLeave` + `nvim_parse_cmd`,
  `let v:event.abort = v:true` (Lua'dan `vim.v.event.abort` kopya olduğu için etkisiz; cabbrev blink cmdline yüzünden
  çalışmadı). `:wq` yazar, `:x`/`ZZ` `update` yapar; uyarı "The code editor stays open…". Tuşlar: `<C-w>c`, `<C-w>q`,
  `<C-w><C-q>`, `<leader>wd` (doğrudan; LazyVim remap'i ulaşmıyordu), `ZQ`, `ZZ`. `g:workbench_keep_code`, varsayılan
  argümansız açılış (`nvim dosya` ve git commit editöründe `:wq` normal çıkar). `:qa`/`:wqa`/`:xa`/`Space q q` çıkar.
- **`:UIReset` (`workbench.reset_ui()`):** `state.sizes` silinir + `wincmd =`; sağ sütun `floor(columns*0.38)`,
  Claude/terminal yarı yarıya, neo-tree 34, DBUI `g:db_ui_winwidth` (40), Git paneli 40, şerit `max(10, lines*0.3)`,
  şerit içi `open_bottom` genişliği (Run 56). İlk açılışta treesitter `info` mesajları susturulur (`firstrun.quiet_treesitter`,
  `package.preload` ile `nvim-treesitter.log`; `:TSLog`'a gider, uyarı/hata görünür; oturum boyu açık).
- **Run sağlayıcısı:** `discover(root)` → `{ id, label, icon, icon_hl, children }`; yaprak öğe `{ id, label, detail,
  icon, icon_hl, cwd, cmd = fun(item): string[], ready? (Lua pattern), profiles?, debug? }`. Çalışma durumu id ile korunur.
  - `debug = { port?, ready, env?, attach? }`: `port` boş port aramasının başlangıcı (verilirse `item.debug_port`),
    `ready` 1. yakalaması port olan pattern, `env(item)` debug ortamı, `attach(item, port, run)` → `run(config)` ile
    DAP oturumu. `cmd(item)` `item.mode` (`"run"|"debug"`) ve `item.debug_port`'u görür. `debug` yoksa "not supported".
  - Runner durum alanları: `mode`, `debug_targets`, `dap_state`, `after_exit`. API: `start(item, mode?)`, `debug`,
    `rerun(item, mode?)` (`restart` takma ad), `menu`, `refresh`, `item(id)`. Çıktı satır satır (parça sınırı birleşir).
  - **Spring debug:** JDWP her zaman `127.0.0.1` (kimlik doğrulamasız; ağa açmak uzaktan kod çalıştırma demek).
    Maven `-Dspring-boot.run.jvmArguments=-agentlib:jdwp=…` (pom'da `<jvmArguments>` varsa yok sayılır!), Gradle
    `--init-script jdwp.gradle -PrunnerJdwp=…` (`JAVA_TOOL_OPTIONS` portu Gradle JVM'ine kaptırırdı). Port 5005'ten,
    `lsof` + bind testi + panelin ayırdıkları elenir. `suspend=n` (jdtls 30–90 sn; `spring.suspend = true` ile değişir).
    Attach: ana sınıf dosyası gizli buffer'a yüklenir (jdtls'i başlatır), `vscode.java.startDebugSession` hazır olana
    kadar `jdtls_timeout` (180 sn) beklenir, `projectName` `resolveMainClass`'tan. Kotlin main → port açık, attach yok.
  - **Node debug:** `NODE_OPTIONS += --inspect=127.0.0.1:0` (sabit port paket yöneticisine giderdi). Her
    `Debugger listening` satırında `lsof` ile pid; job'ın kendisi (pm) ve kapanmış port atlanır, diğer her node süreci
    ayrı `pwa-node` attach (`autoAttachChildProcesses=false`). Rerun eski portun boşalmasını ≤10 sn bekler (EADDRINUSE).
  - Panel ilk açılışta global `<RightMouse>` expr eşlemesi kurar (yalnız Run paneli üstünde yakalar, başka yerde
    PopUp); zaten global eşleme varsa kurmaz. Satır sonu butonları metin genişliği ≥ 37 sütunsa çizilir.
- **Diff tab'ı (`lua/gitdiff`):** `require("gitdiff").open({ path?, rev?, staged?, cwd? })` → `buf|nil`.
  `rev` → o commit'in ilk ebeveyne göre değişikliği (`git show`; `path` yoksa başlık + `--stat` + tüm dosyalar, kök
  commit dahil); `rev` yok + `staged` → `git diff --cached`; `rev` yok → `git diff` (untracked dosya `/dev/null`'a
  karşı tam ekleme). `path` mutlak ya da repo-göreli; `cwd` yoksa path'in klasörü, o da yoksa Neovim cwd'si.
  Değişiklik yoksa bildirim + `nil`; aynı diff tekrar açılırsa aynı buffer yenilenir.
  - Tek unified-diff buffer'ı (`filetype=diff`, ad `gitdiff://<repo>/<yol> (<etiket>)`), `buftype=""` → workbench onu
    kod penceresi sayar (Cmd+W, pin, tab gezme değişmeden çalışır). `BufWriteCmd` yazmayı engeller ("Diff tabs are
    read-only"), `BufReadCmd` (`:e`) yeniler. `PersistenceSavePre`'de silinir (oturuma boş dosya olarak girmesin).
  - Diff buffer'ında ⏎: `--stat` satırı → o dosyanın bölümü; hunk satırı → gerçek dosya o satırda (bugünkü hali).
  - problems `excluded_filetypes`'ta `"diff"` (sayaç/scrollview yok).
  - Elenenler: diffview (kendi tabpage'ini ister), kod penceresini bölüp yan yana (iki pencere/buffer; Cmd+W, pin ve
    `arrange_layout` için kırılgan).
  - githistory `f` modunda `--follow` yüzünden dosya o commit'te eski adla değiştiyse diff boş gelir → tüm commit açılır.
- **Git paneli (`lua/gitstatus`):** filetype `gitstatus`, 40 sütun `winfixwidth`, en sola tam boy açılır. Repolar
  `githistory.repos()`'tan (workspace'te her kök) — **o fonksiyon değişirse panel kırılır.** `git status
  --no-optional-locks` (kendi yenilemesi izleyiciyi tetiklemesin). Yenileme: `BufWritePost`, `FocusGained`, repo
  köklerinin özyinelemeli `fs_event` izlemesi (macOS FSEvents, 300 ms birleştirme; `.git` altında yalnız index/HEAD/refs);
  panel görünmüyorsa yenilemez. Diff: `gitdiff.open({ path, staged = <Staged bölümü>, cwd = repo })`, modül yoksa dosya.
  Discard yalnız tek dosya (Changes/Untracked), ilk seçenek "Cancel"; staged/conflict'te "önce unstage" uyarısı.
  Pencere oturuma girmez. problems `excluded_filetypes` + bufferline offset ("Git") eklendi.
- **Ortak sağ tık (`workbench.on_right_click(name, handler)`):** panellerin global `<RightMouse>` expr eşlemesi tek
  yerde; Run ve Git paneli buraya kaydolur (eskiden ilk kuran kazanıyordu). Yeni panel sağ tıkı da bunu kullanmalı.
- **Font (`lua/font`):** Font Ghostty'de çizilir. `config.ghostty` sonunda `config-file = ?~/.config/ghostty/font.ghostty`
  (opsiyonel include; göreli yol symlink'in değil gerçek dosyanın klasörüne göre çözüldüğü için `~/` ile mutlak).
  `:Font` bu dosyaya `font-family = "..."` yazar (repo dışında, kullanıcı durumu; testte `NVIM_FONT_FILE`), sonra
  arka planda `osascript` ile Ghostty'ye `perform action "reload_config"` (Ghostty 1.4 AppleScript); hata olursa
  "Press Cmd+Shift+, to reload Ghostty". Dosya yoksa Ghostty varsayılanı (gömülü JetBrains Mono). Kurulu fontlar
  `ghostty +list-fonts` ile (~0,6 sn; PATH'te yoksa `/Applications/Ghostty.app/Contents/MacOS/ghostty`). Kurulu olmayan
  font reddedilir + `brew install --cask` önerisi. Nerd Font varyantı gerekmez: Ghostty gömülü "Symbols Nerd Font" taşır.
- **Önizleme tab'ı (`lua/previewtab`):** Files (neo-tree `<cr>`, `<LeftRelease>` tek tık, `<2-LeftMouse>`) ve Git
  paneli (`open_file`) `previewtab.open` üzerinden açar. Tek önizleme; değiştirilirken bufferline `state.custom_sort`'ta
  eski id yeniyle değiştirilir (yuva korunur; `BufAdd`'de yeni buffer sona eklenir, yoksa `table.sort` sırası kararsız),
  eski buffer `Snacks.bufdelete` ile kapanır — **modified ise asla kapatılmaz**. Çeviriciler: `BufModifiedSet`,
  `BufWritePost`, aynı dosyada ikinci açma, pin (`groups.add_element("pinned")` sarılı), Snacks picker
  `actions.jump/confirm` (aynı buffer'da kalan `lsp_*` hariç). İtalik: `highlights.for_element` sarılı, `<grup>Preview`
  kopyası. Durum yalnız bellekte (oturumda normal tab). **bufferline/Snacks iç API'lerine dayanır, güncellemede kontrol et.**
- **Git blame:** `git blame --porcelain --contents -` buffer içeriğiyle çalışır (kaydedilmemiş satırlar kaymaz).
  Lens tanımları LSP `documentSymbol`'den gelir; metot/fonksiyon içine inilmez (callback'ler lens almaz).
- **jdtls (`plugins/java.lua`):** Mason'ın kurduğu güncel jdtls **Java 25+ ister**; sistemdeki Java 21 ile başlatılırsa
  OSGi modülleri çözülemez (`osgi.ee ... version=25`) ve süreç `exit code 13` ile kapanır. Bu yüzden:
  - `find_jdks()` Homebrew `openjdk*` ve `/Library/Java/JavaVirtualMachines` altındaki JDK'ları `release` dosyasından
    sürümüyle tarar; `jdtls_java()` en yeni ≥ `JDTLS_MIN_JAVA` (25) JDK'yı `--java-executable=` ile verir
    (şu an `/opt/homebrew/opt/openjdk`, Java 26). Yoksa uyarı + `brew install openjdk` önerisi. JAVA_HOME'a dokunulmaz.
  - `java.configuration.runtimes`: JavaSE-17, JavaSE-21 (varsayılan, `DEFAULT_PROJECT_JAVA`), JavaSE-26. Projeler
    pom'daki sürüme uyan JDK ile derlenir, sunucunun Java'sıyla değil.
  - `-Djava.import.generatesMetadataFilesAtProjectRoot=false`: jdtls'in kullanıcı projesine `.project`, `.classpath`,
    `.settings/` yazmasını engeller (workspace cache'ine yazar). `target/classes` yine oluşur (gitignore'da).
  - `filter_bundles()`: LazyVim'in topladığı OSGi olmayan java-test jar'larını (runner, jacocoagent, asm) çıkarır;
    yoksa jdtls log'unda `Failed to load extension bundles`. `opts.jdtls` fonksiyon olarak ayarlanır; başka yerde
    `opts.jdtls` tanımlanırsa ezilir.
  - `project_name` = `<klasör>-<yolun sha256 ilk 8>`: farklı repolardaki aynı adlı `backend` klasörleri workspace paylaşmaz.
  - `--java-executable` LazyVim `cmd` listesine 2. eleman olarak eklenir; LazyVim `cmd` yapısını değiştirirse kontrol et.
  - Log'lar: `~/.cache/nvim/jdtls/<proje>-<hash>/workspace/.metadata/.log`, açılışta çökerse `.../config/*.log`,
    `~/.local/state/nvim/lsp.log`. Cache sıfırlama: `rm -rf ~/.cache/nvim/jdtls/<proje>-<hash>`.
  - Gelecekte jdtls Java 27+ isterse `JDTLS_MIN_JAVA` artırılır ve uygun JDK kurulur.
- **Terminal tabları (`lua/termtabs/`):**
  - Model: `groups = { claude, shell }`, her biri `{ tabs = Tab[], current, active, seq }`;
    `Tab = { buf, job, pid, n, cwd, name?, auto?, meta? }`. `current` pencerede görünen buffer (özet olabilir),
    `active` son seçilen gerçek oturum (claudecode'un `get_active_bufnr()` değeri).
  - Buffer'lar `vim.b.termtabs = "claude"|"shell"` (özet ayrıca `vim.b.termtabs_summary`) taşır; panel penceresi bu
    işaretli buffer'ı gösteren penceredir → `arrange_right_column` tab değişiminden etkilenmez.
  - Tab çubuğu pencere-yerel `winbar = "%!v:lua.require'termtabs'.winbar()"`; tıklama işleyicileri global
    `TermTabsClick` / `TermTabsClose` / `TermTabsNew` (`minwid` = buffer no).
  - Aktif olmayan pencerede buffer `noautocmd` ile değiştirilir; yoksa workbench'in `term://*` → `startinsert`
    autocmd'i kod penceresini insert moduna sokar.
  - 2 sn'lik `uv` timer sadece panel görünürken iş yapar, sekme kalmayınca durur.
  - **Snacks provider kullanılmaz:** snacks penceresinde `fixbuf=true` olduğu için aynı pencerede buffer (tab)
    değiştirilemiyor. claudecode spec'inde `provider = require("termtabs.claude").provider`. Kaybolanlar: snacks'in
    çift Esc / `q` tuşları (Ctrl+Q ve Cmd+3 var). Shift+Enter yeni satır Claude buffer'larında yeniden eklendi.
  - Yeni oturum: `new_session()` bayrak koyup `claudecode.terminal.open()` çağırır → komut ve IDE env
    (`CLAUDE_CODE_SSE_PORT` vb.) claudecode'dan gelir. Claude argv listesiyle başlatılır (shell'siz) ki pid doğrudan eşlensin.
    `:ClaudeCode <arg>` (ör. `--resume`) artık her zaman yeni tab açar.
  - claudecode çoklu oturum: tek WebSocket sunucusu, tüm oturumlar aynı porta bağlanır; araç istekleri (openDiff,
    openFile, getDiagnostics) istemci başına request/response, yani her oturumun diff'i kendi isteğine cevap.
    `diff.lua` terminal penceresini `get_active_terminal_bufnr()` ile bulur (genişlik/focus aktif tab'a uygulanır).
    `at_mentioned` (ClaudeCodeSend/Add, sağ tık Send) `server.broadcast` monkeypatch'iyle **sadece aktif tab'a** gider:
    istemcinin karşı portu `lsof` ile pid'e eşlenip önbelleklenir, eşleşme yoksa herkese yayına döner.
    `selection_changed` hâlâ tüm oturumlara gider. **claudecode iç yapısı değişirse bu yamayı kontrol et.**
  - Veri kaynakları (gerçek dosyalarla doğrulandı):
    - Durum: `~/.claude/sessions/<pid>.json` → `status` (busy/idle/waiting), `waitingFor`, `sessionId`, `cwd`
      (canlı güncellenir, `/clear` sonrası yeni oturumu izler).
    - Başlık önceliği: yerel `:R` adı → transcript `{"type":"custom-title","customTitle"}` (`/rename`) →
      sessions json'da `nameSource=="user"` ise `name` → transcript `{"type":"ai-title","aiTitle"}` → "Claude N".
    - Token: `~/.claude/projects/<cwd'de harf/rakam dışı → '-'>/<sessionId>.jsonl` (tutmazsa glob), `assistant`
      kayıtlarındaki `message.usage`. **Aynı mesaj içerik bloğu başına ayrı satıra aynı usage ile yazılır →
      `message.id` ile tekilleştir.** Alt ajanlar: `<sessionId>/subagents/*.jsonl`. `cost-state` sadece kapanışta yazılır.
    - Okuma artımlı (sadece yeni kısım, parça başı en fazla 8 MB); 38 MB transcript parça başı ~30 ms.
  - Terminal tab adı: `b:term_title` (shell başlığı) → ön plandaki alt süreç → başlangıç klasörü.
- **Workspaces (`lua/workspaces/`):** Monorepo olmayan projeleri tek projeymiş gibi açmak için.
  - Kayıt: `~/.local/share/nvim/workspaces.json` → `{"version":1,"workspaces":[{"name","roots":[realpath...],"opened"}]}`
    (son açılışa göre sıralı; elle düzenlenebilir, mtime ile algılanır). Testte `NVIM_WORKSPACES_FILE` /
    `NVIM_WORKSPACES_DIR` ya da `setup({ file, links })` ile yönlendir; gerçek kayda yazma.
  - Yöntem: her workspace için `~/.local/share/nvim/workspaces/<ad>/` altında köklere symlink'ler (aynı ad → `-2`);
    workspace açıkken cwd bu klasör. neo-tree'nin normal filesystem kaynağı kullanılır (yerleşik çoklu kök yok).
    Neovim 0.12 `:edit`/`bufadd`'de symlink'i çözdüğü için buffer'lar **gerçek yolla** açılır → jdtls/vtsls kökü ve
    `:GitBlame` doğru çalışır.
  - Açık workspace durumu tutulmaz, cwd'den hesaplanır (`current()`); neo-tree fast-event callback'lerinde de
    çağrıldığı için `vim.fn` kullanmaz.
  - neo-tree yamaları iç fonksiyonlara dayanır: `manager.get_path_to_reveal` (gerçek yol → symlink yolu; reveal/follow),
    `git.status_async` (her kökün kendi reposu), `git.find_existing_status_code` (symlink → gerçek yol) +
    `git_status_changed` işleyicisi. **neo-tree güncellenince bu fonksiyonların varlığını kontrol et.**
  - Cmd+F / Cmd+Shift+F, dashboard `f`/`g`, `<leader>fF`: Snacks picker `files`/`grep`/`grep_word` kaynaklarının
    `config` kancasıyla tüm köklerde arar. LazyVim'in root tabanlı `<leader><space>`/`<leader>ff`'i dosyanın projesinde arar.
  - Runner: `M.roots()` + `M.scan(roots)` (string ya da liste); çok kökte açıklamaya kök adı eklenir (`backend-api › …`).
    Git paneli: `M.repos()`, önce kod penceresindeki dosyanın reposu; çok repoda `g` ile repo seçimi.
  - Oturum: her workspace'in kendi persistence oturumu (cwd anahtarı). Açarken kaydedilmemiş dosya varsa durur;
    önceki oturum kaydedilir, dosya buffer'ları kapanır (Claude/terminal/Run logları kalır). PersistenceSavePre'de
    neo-tree kapatılır. Yeniden adlandırma/silme oturum dosyasını da taşır/siler. Workspace klasöründe `nvim` başlatmak
    ya da `s` ile oturum yüklemek workspace'i otomatik algılar.
  - API: `list/get/create/add_root/remove_root/rename/delete/open/close/current/roots/dirs/link_dir/to_real/to_link/
    picker_config`; arayüz `workspaces.ui`: `pick/manage/new/add_folder/remove_folder/rename/delete`.
- **Pinli tablar (`workbench`):** `M.toggle_pin()` (Cmd+P), `M.code(opts)` → `{ pinned?, force? }`
  (`panel()`/`M.toggle()` ve Cmd+Shift/Option+oklar `pinned = false` ile çağırır), `M.focus_code()` (koddayken de
  en baştaki pinli tab'ı gösterir; açılış akışı bunu çağırmalı), `M.restore_pins()`.
  - Pinler bufferline'ın `vim.g.BufferlinePinnedBuffers` global'inde; LazyVim `sessionoptions`'ında `globals` var.
    bufferline pinleri sadece ilk `SessionLoadPost`'ta ve kendisi yüklüyse geri yüklediği için `restore_pins` her
    `SessionLoadPost`'ta (workspace değişimi dahil) ve açılışta bir kez çalışır.
  - bufferline iç API'lerine dayanır: `bufferline.groups._is_pinned`, `add_element`/`remove_element`,
    `bufferline.state.components` — **bufferline güncellenince kontrol et.** Pinler dosya yoluyla saklanır (adsız
    buffer pin'i oturumda kalmaz; virgüllü yollar bozulur — bufferline sınırı). Pin ikonu varsayılan 📌.
- **Otomatik açılış (`autorestore`):** argümansız `nvim`'de sırayla: workspace klasörü → içinde bulunulan proje
  (`.git` öncelikli, yoksa en yakın pom/gradle/package.json/... işaretçisi; alt klasörden köke `cd`; `~` ve `/` proje
  sayılmaz) → cwd'nin kendi oturumu → son kayıt → (bulunamazsa) dashboard.
  - "Son" kaydı kendimiz tutarız (`stdpath("state")/autorestore.json`, `{version,kind,dir,name?,time}`); persistence
    mtime'ı kullanılmaz çünkü ev klasöründe `:e not.txt` + çıkış da oturum yazar.
  - Kayıt DirChanged + VimLeavePre'de, yalnız: argümansız ya da `nvim .` açılış, headless değil, özellik açık,
    `Space q d` ile durdurulmamış, cwd workspace ya da işaretçili klasör, cwd `/tmp`,`/private/tmp`,`/var/folders` dışı.
    `NVIM_AUTORESTORE_FILE` verilince geçici klasörler de kaydedilir (test).
  - Atlanır: dosya/klasör argümanı, `+cmd`/`-c`/`-S`/`-q`/`-d`/`-t`/`-b`/`-`, stdin, headless, `nvim +Dashboard`,
    `NVIM_NO_RESTORE=1`, `NVIM_AUTORESTORE=0`, `vim.g.autorestore = false`.
  - Zamanlama: VimEnter (`nested`, `once`) senkron yükler, sonra `Snacks.config.dashboard.enabled = false`.
    `plain_start` = Snacks dashboard koşulları ama yüzen olmayan tek pencere + buffer 1 (lazy kurulum penceresi bozmaz).
  - Swap: ölü sürecin swap'ı varsa dosya istemsiz salt okunur açılır + uyarı; yükleme `pcall`, hata → dashboard.
  - Workspace: `workspaces.open(name)`; cwd zaten workspace klasöründeyse önce oturum elle yüklenir (open() yüklemez).
    Yüklemeden sonra `workbench.restore_pins()`.
- **Varsayılan panel düzeni:** `vim.g.default_layout` (varsayılan `{ "files", "claude", "terminal" }`, `false` = kapalı;
  tek açılış `NVIM_DEFAULT_LAYOUT=0`). Açılış otomatiği, proje klasöründe başlatma, dashboard `p`/`s`/`w`,
  `:Workspace open`, `Space f p`, `Space q s`/`q l`, projeye `:cd` sonrası kurulur (`User PersistenceLoadPost` +
  `DirChanged`, `vim.schedule` ile birleşik). Açık panel aranıp dokunulmaz; gizli Claude/terminal tab'ı yeni süreç
  açmadan gösterilir. Oturumdan dosya gelmezse boş editör. Sonda `workbench.focus_code()` (pinli tab, normal mod);
  odakta yüzen pencere (picker) varsa odağa dokunulmaz. Workspace değişiminde yeni Claude oturumu **açılmaz**.
- **Problem göstergesi (`problems` + nvim-scrollview):** scrollview mouse'u `on_key` ile yakalar, `<LeftMouse>`
  map'lerini ezmez. Gruplar: diagnostics, search, conflicts, gitsigns (contrib); `marks` bilerek kapalı.
  - Sayaç kod pencerelerinde pencere-yerel `winbar = "%!v:lua.require'problems'.winbar()"` ile çizilir (float, gitblame
    lens'ini örterdi). `apply()` yalnız kendi koyduğu ya da boş winbar'ı değiştirir; termtabs winbar'ına dokunmaz.
  - Kod penceresi = `problems.is_code_window()`: float değil, `buftype==""`, `excluded_filetypes` dışında, diff kapalı,
    `vim.b.termtabs`/`vim.b.workbench_bottom` yok. Yeni bir panel filetype'ı `buftype=""` ise listeye eklenmeli.
  - GitBlame sütununa boş winbar (`" "`) verilir (scrollbind hizası); diff penceresinde winbar yok.
  - Tıklama işleyicileri global `ProblemsNext/Prev/NextError/...`, `minwid` = pencere id'si.
  - `jump(win, count, severity?)` — `win` 0 kabul etmez, gerçek pencere id'si ver.
  - ✖/⚠ sayısı kendi önem derecesine gider; "önce en yüksek önem" kuralı yalnız ↑/↓ oklarında.
- **Testlerde gerçek kullanıcı kayıtlarına yazma:** `NVIM_AUTORESTORE_FILE`, `NVIM_WORKSPACES_FILE`/`_DIR`,
  persistence oturum klasörü — test sonrası temizle. (Bir test gerçek `autorestore.json`'u ezmişti.)
- **Tarih biçimi:** arayüzde ISO `%Y-%m-%d` (GitBlame sütunu ve Git geçmişi paneli tutarlı).
- **Panel komutları:** `workbench.toggle(name)` saf toggle'dır (açıksa odakta olmasa da kapatır); Cmd tuşlarının
  `panel()` mantığından (önce odakla) farklıdır. `commands.lua` aynı adda başka eklenti komutu varsa onu ezmez,
  uyarıp atlar (ör. ileride fugitive kurulursa `:Git` çakışır).
- **Panel davranışı:** `workbench.panel(find, open, close)` → kapalıysa aç, açıksa odakla, odaktaysa kapat ve koda dön.
- **Monorepo:** Neovim repo kökünden açılır. LSP'ler kökü dosyadan bulur. jdtls kökü `plugins/java.lua`
  içindeki `java_root()` ile belirlenir (settings.gradle → en üstteki pom/build.gradle zinciri → mvnw/.git).
  Run ve Git panelleri `LazyVim.root()`'tan değil, workspace açıksa tüm köklerden, değilse `getcwd()`'den çalışır.

## Test yöntemi

- Headless test: `nvim --headless -c "luafile test.lua"`; LazyVim kısayolları `VeryLazy` ile yüklendiği için
  testte önce `vim.api.nvim_exec_autocmds("User", { pattern = "VeryLazy" })` çağır.
- `keymaps.lua`'da bir hata olursa LazyVim bunu headless'ta sessizce yutar ve dosyanın geri kalanı yüklenmez.
  Her değişiklikten sonra `pcall(dofile, vim.fn.stdpath("config") .. "/lua/config/keymaps.lua")` ile kontrol et.
  `vim.keymap.set` tek bir lhs string'i alır; birden fazla tuş için döngü kullan (liste verme).
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
| 5 | **Cmd+1** dosyalar, **Cmd+2** kod, **Cmd+3** Claude Code, **Cmd+4** terminal, **Cmd+5** DB, **Cmd+6** Spring, **Cmd+7** Git History, **Cmd+0** Git (commit edilmemiş dosyalar) | IntelliJ tarzı: aç / odakla / odaktaysa kapat. `Space 0..7` karşılıkları da var. Cmd+0 Ghostty'nin "font boyutunu sıfırla" kısayolunu ezer. |
| 6 | Terminal ve Claude **sağda**; terminal, Claude açıksa onun altında | Sağ sütun her zaman tam boy. |
| 7 | **Run paneli ve Git geçmişi altta yatay** (alt şerit) | Alt şerit sol panellerin altına uzanabilir, sağ sütunun altına **giremez**. Logları panelin sağında açılır. |
| 8 | Ghostty sekme geçişi Cmd+1..9 yerine **Ctrl+Tab** | Cmd+1..9 Neovim'e verildi. |
| 9 | **Cmd+F = dosya ara**, **Cmd+Shift+F = projede metin ara**; Cmd+P dosya aramadan kaldırıldı (sonra #30 ile pin'e verildi) | Ghostty'nin kendi Cmd+F aramasından vazgeçildi. |
| 10 | Diğer Cmd kısayolları: Cmd+S kaydet, Cmd+E son dosyalar, Cmd+W buffer kapat, Cmd+/ yorum | |
| 11 | Hareket **ok tuşlarıyla** (hjkl kullanılmayacak) | `whichwrap` ile satır sonunda sağ/sol ok satır değiştirir. |
| 12 | **Cmd+Sol/Sağ** satır başı/sonu (Sol: önce ilk dolu karakter, tekrar basınca sütun 1), **Cmd+Yukarı/Aşağı** dosya başı/sonu, **Option+Sol/Sağ** kelime atlama | Terminalde Cmd+Sol/Sağ shell'e Ctrl+A/Ctrl+E olarak iletilir. |
| 13 | Mouse: Ctrl/Option+tık tanıma git; sağ tık menüsünde References, Rename, Code Action, Claude'a gönder | macOS Ctrl+tık'ı sağ tık yapabilir, Option+tık yedek. |
| 14 | **Monorepo** (BE + FE farklı modüllerde) kökten açılır | jdtls kökü özel `java_root()`; TS kökü lock dosyasından; Spring/Git panelleri cwd'den. FE'de lock dosyası olmalı. |
| 15 | DB arayüzü **vim-dadbod-ui** (solda) | `mysql-client` keg-only olduğu için Neovim PATH'ine eklendi. Proje bazlı bağlantılar `.lazy.lua` içinde `vim.g.dbs` (gitignore). |
| 16 | **Run paneli** (eski Spring paneli genelleştirildi) | Ağaç: Spring Boot grubu + Node.js grubu → paket → script'ler. **Space ile grup/paket kapanır-açılır** (altı gizlenir). Spring: `@SpringBootApplication` taraması, `mvnw`/`gradlew` öncelikli, profil, `Started ... in` ve port yakalama. Node: tüm `package.json`'lar (node_modules hariç), script sırası dosyadaki gibi, paket yöneticisi lock dosyasından (npm/pnpm/yarn/bun). Nerd Font ikonları. Başlatınca log otomatik açılır. |
| 17 | **Git History paneli özel modül; diff'ler kod editöründe tab** (diffview.nvim kaldırıldı) | Enter/çift tık commit diff'i kod editöründe bufferline tab'ı olarak açar, paneller yerinde kalır (eskiden tam ekran sekme); `f` dosya geçmişi, `a` tüm dallar; `.git/logs/HEAD` izlenip otomatik yenilenir. |
| 18 | Yapılandırma bu repoda, `~/.config` altına symlink | Yedek ve yeni makine kurulumu için. |
| 19 | **Cmd+6 ve Cmd+7 aynı anda açık olamaz** | Alt şerit tek araç gösterir; biri açılınca diğeri (ve loglar) kapanır. |
| 20 | **Sağ sütun sırası sabit: üstte Claude, altta terminal** | Hangisi önce açılırsa açılsın; asla yan yana durmazlar. |
| 21 | **Editör tabları: Cmd+Shift+Sol/Sağ** (ayrıca Cmd+Option+Sol/Sağ) (bufferline) | Cmd+Sol/Sağ satır başı/sonu olarak kaldı. Terminal/paneldeyken önce koda geçer. |
| 22 | **`:GitBlame`** (`Space g a`, sağ tık → Annotate): IntelliJ "Annotate" gibi sol sütunda tarih + yazar | Kodla senkron kayar; Enter commit'i açar. Kapalıyken sınıf/metot/fonksiyon satırı sonunda ` yazar +N` (IntelliJ Code Vision gibi). |
| 23 | Tüm kısayollar `nvim/KISAYOLLAR.md` içinde tek dosyada | LazyVim varsayılanları kurulu sürümden doğrulanarak eklendi. |
| 24 | **Panel Ex komutları:** `:Files`, `:Code`, `:Claude`, `:Terminal`, `:Datasource`, `:Run`, `:Git` (sol Git paneli), `:GitHistory` | Saf toggle; `:Code` sadece odaklar. Çakışma kontrol edildi (fugitive yok). |
| 25 | **jdtls düzeltmesi:** jdtls Java 25+ ile (en yeni Homebrew JDK, şu an 26) başlatılır; projeler kendi Java sürümüyle derlenir | Kök neden: güncel jdtls Java 25 istiyor, Java 21 ile `exit code 13`. Proje köküne metadata yazılmaz. |
| 26 | **Workspaces** (farklı yerlerdeki klasörleri tek proje gibi açma), açılış ekranında `w` | Symlink klasörü + neo-tree; dosyalar gerçek yolla açılır. Arama, Run ve Git panelleri tüm kökleri kapsar. Her workspace'in ayrı oturumu var. |
| 27 | **Claude ve Terminal panellerinde tablar** | Cmd+T yeni tab (paneller dışında yeni boş dosya); Ghostty yeni sekmesi **Cmd+Shift+T**'ye taşındı (Ghostty'nin oradaki "undo"su Cmd+Z'de duruyor). Claude tab adı = oturum adı. Rename `:R <ad>` (terminalde `/` shell'e gittiği için "/r" birebir yapılamadı) + tab'a sağ tık. Cmd+Shift+Sol/Sağ panel içinde o panelin tablarında gezer. Claude'da en solda özet tab'ı (durum, token, son iş). Tab çubuğu mouse ile tıklanabilir. |
| 28 | **Editör arayüzü tamamen İngilizce** | Kullanıcı kararı; dokümanlar Türkçe kalır. |
| 29 | **Alt şerit her zaman sol panellerin altına uzanır** | Files/DB kapatılıp açılsa da Run/Git şeridinin üstüne çıkamaz; sağ sütun yine tam boy. Genel `arrange_layout()`. |
| 30 | **Cmd+P = tab'ı pinle / pin'i kaldır**; koda geçişte (Cmd+2, `:Code`, proje açılışı) en baştaki pinli tab gösterilir | Panelden basılırsa son kod tab'ına uygulanır. Tab gezinme/tıklama/dosya açma/fare ile koda tıklama ve odaktaki paneli kapatıp koda dönme pinli tab'a atlamaz. Pinler oturumla geri gelir. |
| 31 | **IntelliJ tarzı sağ kenar problem göstergesi** (nvim-scrollview) + kod penceresinin sağ üstünde sayaç ve ↑↓ | Sadece kod pencerelerinde. İşarete tıkla → satıra git, çubuğu sürükle → kaydır. Sayaca tıkla → o türün sonraki problemi; ↑↓ önce en yüksek önemde gezer. Sayaç winbar'da (her kod penceresinde 1 satır). satellite.nvim (işarete tıklama yok) ve nvim-scrollbar (mouse yok) elendi. |
| 32 | **Argümansız `nvim` son projeyi/workspace'i açar**; proje içinde başlatılırsa o proje (kökü) açılır | Kendi "son" kaydımız; git commit editörü, `nvim dosya`, headless ve test açılışları kaydı ezmez. Dashboard: `nvim +Dashboard`, `NVIM_NO_RESTORE=1` ya da açıkken `Space q h` / `:Dashboard` (yüzen). Kapatma `vim.g.autorestore = false`. |
| 33 | **Her proje/workspace açılışında Files + editör + Claude + terminal açık gelir** | `vim.g.default_layout` ile seçilebilir/kapatılabilir. Kapatılan Claude/terminal sonraki proje açılışında yeniden açılır. Workspace değiştirince Claude yeniden başlamaz (konuşma kaybolmasın); yeni projede Claude için Cmd+T. |
| 34 | **Run panelinde her öğe için Run / Stop / Debug / Rerun** (klavye, satır sonu butonları, sağ tık menüsü, `Space j`) | Spring (Maven/Gradle, jdtls ile attach) ve Node (npm/pnpm/yarn, alt süreçler dahil otomatik attach). Debug portu yalnız 127.0.0.1. Uygulama debugger'ı beklemez (`suspend=n`); jdtls hazır olunca bağlanır. |
| 35 | **Sol Git paneli (Cmd+0, `:Git`)**: sadece commit edilmemiş dosyalar, stage / unstage / discard; alt paneldeki geçmiş **Git History** (Cmd+7, `:GitHistory`) | Özel modül (neo-tree git_status gruplamıyor, çoklu repo yok, Files penceresini paylaşırdı). Files/DB ile yan yana durur, en solda. Discard her zaman onay ister. Commit atma yok (istenmedi). |
| 36 | **`:Font` ile font geçişi**, varsayılan JetBrains Mono | Seçenekler: JetBrains Mono, Geist Mono, Berkeley Mono (ücretli, kullanıcı kurarsa), Cascadia Code, Fira Code, Maple Mono, IBM Plex Mono, Monaspace Neon/Argon/Xenon/Radon/Krypton. Ücretsizler Brewfile'da cask olarak. Seçim kalıcı, Ghostty otomatik yeniden yüklenir. |
| 37 | **Ürün adı LESS** ("LESS editor"): sade editör, IDEA gibi kalabalık değil. Ghostty fork'u bu repoda (`app/`), `LESS.app`, `com.oguz-dev.less-editor`, CLI `lessed` | Faz 1 = yeniden markalanmış Ghostty + gömülü Neovim config'i. Fork ayrı repo değil, subtree (upstream güncellemesi kolay). |
| 38 | **Önizleme tab'ı** (VS Code gibi): Files/Git panelinde Enter / tek tık önizleme (italik, tek tane, yerine yenisi açılır); tekrar Enter / çift tık, düzenleme, kaydetme, pin → normal tab | Picker ve `gd` ile açılanlar doğrudan normal tab. |
| 39 | **Proje adı less-editor; README İngilizce ürün README'si.** Release: GitHub Actions, macOS universal zip + DMG; Linux deneysel; Windows yok | Ghostty Windows'u desteklemiyor. İmza/notarization Apple hesabı gelince secret'larla. |
| 40 | **Uzun mesaj paneli (noice) alt şeritte**, Run / Git History ile aynı yeri paylaşır | Biri açılınca diğeri gider; `q` kapatır. İlk açılışta treesitter indirme mesajları gösterilmez. |
| 41 | **Ortadaki kod penceresi `:q` vb. ile kapanmaz** | Tab kapatmak Cmd+W, çıkmak `:qa` / Cmd+Q / `Space q q`. `nvim dosya` ve git commit editöründe koruma kapalı. |
| 42 | **`:UIReset`** panel boyutlarını varsayılana döndürür | AI sütunu kodu kapatacak kadar genişlediğinde. Tuş **Cmd+Shift+0** (`csi:48;10u` → `<S-D-0>`; klavye düzeninden bağımsız, TR'de Shift+0 `=` olurdu). |
| 43 | **Lisans: MIT** (kök `LICENSE`), Ghostty ile aynı | Kullanıcı: Ghostty'den daha serbest bir şey seçilmemeli; MIT zaten aynı. Ghostty'nin MIT metni `app/ghostty/LICENSE` ve pakette `GHOSTTY-LICENSE` olarak korunur. |

## Açık konular / öneriler

Kullanıcı onayı bekleyen özellik önerileri: [ONERILER.md](ONERILER.md).

- Yerleşim: şeritte bir pencerede `:split` yapılırsa (aynı işaretli buffer) yeni pencere şeride yan yana dizilir;
  `Space e` gibi workbench dışı yollarla açılan sol paneller bir kare kayık görünebilir.

- Terminal tabları: normal modda Cmd+W bir terminal buffer'ında basılırsa panelde kod buffer'ı kalabilir (tab kapatmak
  için ✕ / `:TermTabClose`). Yeni Claude oturumunun transcript'i ilk mesaja kadar yoktur ("Claude N" + ◌).
  Durum/token panel gizliyken güncellenmez. Gerçek `claude` CLI ile uçtan uca (IDE diff, `/rename` sonrası başlık)
  ve özet tab'ında çift tık canlı test edilmedi.

- Workspaces sınırlamaları: neo-tree'de kök/dosya yeniden adlandırma-taşıma symlink yolunu kullanır (disk doğru, açık
  buffer adı ve LSP `willRename` güncellenmeyebilir); ağaç başlığında `~/.local/share/nvim/workspaces/<ad>` görünür;
  workspace değiştirince önceki projenin LSP sunucuları kapanmaz.
- Workspace'te Claude Code ve terminal symlink klasöründe başlar; Claude'un Grep/Glob araçları symlink izlemezse arama
  eksik kalabilir. Öneri: claudecode'a `terminal.cwd_provider` (ilk kök) + diğer kökler için `--add-dir`.

- PostgreSQL istemcisi (`psql`) kurulu değil; Postgres kullanılacaksa `brew install libpq` (PATH'e zaten ekli).
- `application.yml/properties` tamamlama için `spring-boot.nvim` eklenebilir.
- Shift+Ok ile seçim (mac tarzı) henüz yok.
- Git paneli: özyinelemeli izleme Linux'ta yok (`shortcut:` notu); gitignore'daki build çıktıları (`target/`,
  `node_modules`) da yenilemeyi tetikleyebilir; toplu discard yok; Ghostty'de görsel kontrol edilmedi. pty testi
  gerçek nvim açtığı için kullanıcının shada'sı (komut geçmişi) yazılmış olabilir.
- Run debug: Spring Boot Gradle eklentisiyle gerçek `bootRun` (ve configuration cache) test edilmedi; bun ve
  Next.js debug denenmedi; LazyVim dap-ui attach olunca otomatik açılır, workbench yerleşimini bozabilir; başka
  eklenti global `<RightMouse>` kurarsa panel odakta değilken sağ tık PopUp açar. İkonlar Ghostty'de görsel kontrol edilmedi.
- Otomatik açılış: birden çok Ghostty sekmesinde nvim açıksa en son çıkan / proje açan "son" olur. Workspace'ten
  workspace'e geçişte mevcut Claude/terminal tab'ları eski cwd'de kalır (yeni tab için Cmd+T). `workspaces.open()`
  bildirimleri hâlâ Türkçe (İngilizce kararına göre çevrilmeli); `open()`'a `opts.load_session` eklenirse
  autorestore'daki özel durum kalkar.
- Problem göstergesi: çubuk/işaretler en sağ sütundaki metnin üstüne yarı saydam biner; işaret üstünde sağ tık,
  sağ tık menüsü yerine işaret bilgisini açar. Sayaca sağ tık (Trouble) ve Ghostty'de görsel görünüm canlı
  doğrulanmadı. F2/Shift+F2 kısayolları önerildi, eklenmedi ([ONERILER.md](ONERILER.md)).
- Diff tab'ı: yan yana görünüm yok (yalnız unified); eski commit'te hunk'tan ⏎ dosyanın bugünkü halini açar;
  `--stat`'tan atlama yeniden adlandırılmış (`a => b`) dosyada çalışmaz. LazyVim `Space g h d`/`g h D` (gitsigns
  diffthis) kod penceresini yerinde böler (tam ekran değil, ama tab olarak da görünmez) — istenirse
  `gitdiff.open({ path = <buffer> })`'a bağlanabilir. `lazy-lock.json`'daki diffview satırı bir sonraki `:Lazy clean`'de
  düşer. stylua kurulu değil, biçim elle.
- Font: seçicide önizleme yok; fonta özel `font-feature` (ligatür) yazılmıyor; Berkeley Mono "Berkeley Mono" aile
  adıyla aranır (farklı adla kurulursa görünmez); ilk osascript'te macOS Otomasyon izni sorabilir; ekranda canlı
  doğrulanmadı.
- LESS: program başlık koymazsa yedek başlık `👻`; Cmd+Shift+P komut paletinde Ghostty'nin şaka ve güncelleme
  girişleri duruyor. Soğuk `lessed` açılışı çağıranın ortamını uygulamaya taşır. About ve sekmeli başlık görsel
  doğrulanmadı. Sonraki adımlar: notarization + DMG. İlk derlemede macOS bir kez "Dock tile extension" bildirimi
  gösterdi (System Settings → Login Items & Extensions'tan kaldırılabilir).
- İlk açılış: treesitter parser başına mesajları hâlâ görünür; LazyVim hiç inemezse önce "No specs found" hata seli +
  "Press ENTER", LESS mesajı sonra gelir; lazy.nvim clone'u sırasında yalnız "Getting ready…". Ghostty/LESS.app'te
  gözle bakılmadı (pty ekran metni doğrulandı).
- Önizleme tab'ı: bufferline'da tab'a geçmek onu normal yapmaz; `:e`, Snacks dışı açma yolları çevirmez; neo-tree
  `<Right>`/`l` doğrudan normal tab açar; Git panelinde tek tık eşlemesi yok (çift tık da önizleme). Split'te ve
  neo-tree buffers/git_status kaynaklarında denenmedi; italik Ghostty'de gözle görülmedi.
- Sürüm hattı hiç gerçek CI'da çalışmadı (yalnız actionlint): `xcode-27` preview imajı, universal derleme, imza/
  notarization (`--deep`, `shortcut:` notu), Linux apt paket adları (`libgtk4-layer-shell-dev`), blueprint kurulumu ve
  GTK derlemesi doğrulanmadı. Linux'ta About/menüler hâlâ "Ghostty", `super+` kısayolları masaüstüyle çakışabilir,
  zsh gerekir, `:Font` reload ve `lessed` yok. CI'da stylua yok (biçim doğrulanmamış kod CI'ı kırabilirdi).
- Şerit/koruma: yeni uzun mesaj Run/Git History'yi gizler (tek araç kuralı). Kod penceresi koruması mapping içinden
  `<Cmd>q<CR>`, `:bd`/`:only` ve Lua `nvim_win_close` yollarını kapsamaz. `restore` içinde hata olursa `winfixwidth` o turda
  kapalı kalabilir. Sağ sütuna %50 üst sınır yok. Gerçek temiz ilk açılış bu değişiklikten sonra çalıştırılmadı.
- Node.js Homebrew'dan değil `~/.local/bin` altından geliyor; Mason'ın npm tabanlı araçları için Node gerekli.
