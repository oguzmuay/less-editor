# LESS

LESS editörünün (Ghostty fork'u + Neovim yapılandırması) **tamamen Rust ile yeniden yazımı**. Amaç: `../nvim/` altındaki
tüm özellikleri (bkz. [`../FEATURES.md`](../FEATURES.md)) tek bir yerel uygulamada, Neovim ve Lua'ya bağımlı olmadan sunmak.

> Durum: egui/eframe 0.37 üzerinde çalışan proje editörü ve panel ev sahibi. Vim / Normal düzenleme, Files, Git,
> Git History, Run/debug, AI/Terminal ve Database arayüzleri vardır. Yol haritası ve bilinçli sınırlar:
> [`docs/PLAN.md`](docs/PLAN.md), [`docs/handoff/`](docs/handoff/).
>
> - **Karşılama:** son projeler / workspace'ler, yeni proje veya workspace, Open Folder ve Customize.
> - **Proje:** düzenlenebilir kod tab'ları, dosya/proje arama, paneller ve Cmd kısayolları. Ayrıntı:
>   [`docs/KEYMAP.md`](docs/KEYMAP.md).
> - **Notes:** Cmd+N ortada tab'lı Markdown notları açar; Files odaktayken yeni dosya oluşturur. Notlar
>   `.less/notes/` altında otomatik kaydedilir. Cmd+Shift+N yeni LESS penceresi açar.
> - **Oturum:** ilk pencere cwd projesini ya da son projeyi/workspace'i açar; sonraki argümansız pencereler karşılamada
>   başlar. Açık paneller, boyutlar, dosya tab'ları, imleç ve pinler proje başına geri yüklenir. `--dashboard`,
>   `LESS_RS_NO_RESTORE=1` veya `LESS_RS_AUTORESTORE=0` otomatik açmayı kapatır.
> - **Görünüm:** `:Theme` ve `:Font` canlı önizlemeli seçici açar; Enter kaydeder, Esc geri alır. Font yalnız LESS-rs'in
>   kendi egui arayüzüne uygulanır; Ghostty ya da başka uygulama kontrol edilmez.
> - **File menüsü:** Open Folder ve Open Recent seçimi yeni pencerede açar. Bu, eframe içindeki uygulama menüsüdür;
>   native macOS menü çubuğu henüz yoktur.
> - **Kayıtlar:** `~/.config/less-rs/config.toml`, `~/.local/state/less-rs/`, `~/.local/share/less-rs/`. LESS-nvim'in
>   kayıtları yalnız okunur; onlara yazılmaz. Testler `LESS_RS_CONFIG_DIR` / `LESS_RS_STATE_DIR` / `LESS_RS_DATA_DIR`
>   ve izole geçici klasörler kullanır.
>
> Varlıklar: `crates/less-app/assets/icon.png`, `Phosphor-Regular.ttf` (MIT, `Phosphor-LICENSE.txt`). Varsayılan font
> JetBrains Mono; seçili font sistemde yoksa egui varsayılanı kullanılır ve seçici girdiyi "not installed" gösterir.

## Ana kararlar

| Konu | Karar |
|---|---|
| Arayüz | Yerel GPU'lu GUI (TUI değil), egui/eframe 0.37. |
| Modlar | **Vim modu (varsayılan, Rust'ta kendi motorumuz; gerçek nvim gömülmez)** ve **Normal mod** (modeless, Cmd kısayollarıyla). Ayardan ya da çalışırken değişir. |
| Veritabanı sürücüleri | Çekirdekte yok. Kullanıcı bir bağlantı türü seçince **istenirse indirilir** (ayrı süreç, sağlama toplamı doğrulamalı). |
| Kısayollar | LESS'e özel eklenenler aynen taşınır; Neovim/LazyVim'den gelenler yalnız Vim modunun doğal davranışı olarak kalır. Bkz. [`docs/KEYMAP.md`](docs/KEYMAP.md). |
| Arayüz dili | İngilizce (editördeki gibi); dokümanlar Türkçe. |
| Lisans | MIT |

## Yapı

```
less-rs/
  Cargo.toml            workspace
  crates/
    less-core/          metin modeli (buffer, undo, seçim, çoklu imleç)
    less-input/         mod motoru (Vim / Normal) + tuş haritası
    less-workbench/     panel yöneticisi, yerleşim, boyut sınırları, düzen kaydı
    less-lang/          LSP, tree-sitter, tooltip, problem verisi
    less-git/           Git paneli, geçmiş grafiği, blame, diff, merge
    less-run/           Run paneli, süreçler, DAP, debug değişkenleri
    less-db/            Database paneli çekirdeği + sürücü protokolü / indirme
    less-ai/            AI sekmeleri, oturum izleme, .less/ai, LESS MCP
    less-workspace/     workspace, projeler, oturum, son proje
    less-theme/         tema + font
    less-app/           ikili (`less-rs`)
  docs/
    PLAN.md             mimari + fazlar + açık sorular
    KEYMAP.md           taşınacak kısayollar
```

## Derleme ve kullanım

Gereksinimler: güncel Rust toolchain; paketleme için macOS'un `sips`, `iconutil`, `codesign` ve `hdiutil` araçları.

```sh
cd less-rs
cargo run -p less-app                 # geliştirme
cargo run -p less-app -- /yol/proje   # belirli klasör
cargo run -p less-app -- --workspace ad
make check                            # workspace cargo check
make test                             # workspace testleri
make clippy                           # bütün hedefler, uyarılar hata
make app                              # build/LESS.app
make dist                             # build/dist/*.zip ve *.dmg
make install                          # /Applications/LESS.app (PREFIX değiştirilebilir)
```

`make app` / `make dist` release ikilisini derler, uygulama paketini kurar ve **ad-hoc imzalar**. Developer ID imzası ve
notarization yoktur; macOS ilk açılışta onay isteyebilir. Betikler dış servise yükleme veya yayınlama yapmaz. Mevcut
Ghostty tabanlı `app/` uygulamasını değiştirmez. `CARGO_TARGET_DIR` verildiğinde paketleme aynı hedefin `release/less-rs`
ikilisini kullanır.
