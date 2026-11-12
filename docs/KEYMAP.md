# LESS-rs: Taşınacak Kısayollar

**Kural:** Yalnız LESS'e özel eklenen ve olmazsa kullanımı engelleyen kısayollar taşınır. Neovim / LazyVim'den gelenler
(`gd`, `gr`, `K`, `]d`, `Space c a`, `s`+2 harf, `Space s …` vb.) ayrıca taşınmaz: Vim modunda motorun doğal parçası olanlar
çalışır, gerisi için Cmd+K arama ve komut paleti yeter. Kaynak: `nvim/KISAYOLLAR.md` (kalın olanlar).

Normal modda (modeless) aynı Cmd kısayolları geçerlidir; Vim'e özgü tuşlar yoktur.

## Taşınır: paneller

| Tuş | İş |
|---|---|
| Cmd+1 / 2 / 3 / 4 / 5 / 6 / 7 / 0 | Files / Kod / AI / Terminal / Database / Run / Git History / Git (aç, odakla, odaktaysa kapat) |
| Cmd+Shift+0 | Panel boyutlarını sıfırla (`:UIReset`) |
| Cmd+N | Notlar (Files panelinde: yeni dosya) |
| Cmd+Shift+N | Yeni pencere |
| Cmd+K | Kısayol ara |
| Cmd+Shift+P | Komut paleti |
| `Space j` (Vim) / Cmd+J *(Cmd+J öneri, onaylanmadı; mevcut LESS'te yok)* | Run: öğe seç, sonra eylem |

## Taşınır: tab ve dosya

| Tuş | İş |
|---|---|
| Cmd+Shift+Sol / Sağ, Cmd+Option+Sol / Sağ | Tab gezinme (AI/Terminal/Run log/Notlar içinde o panelin tabları) |
| Cmd+T | Bağlama göre yeni tab (AI: tür seçici, Terminal: shell, başka yerde yeni boş dosya) |
| Cmd+W | Tab kapat |
| Cmd+Q | Yalnız bu pencereyi kapat |
| Cmd+P | Tab pinle: pinli tab'lar tab çubuğunun başına taşınır, pini kalkan pinli grubun arkasına döner (#30). Markdown dosyasında önizleme aç/kapa (#83; Esc metne döner, bağlantılar tıklanır), HTML dosyasında varsayılan tarayıcıda aç |
| Cmd+S | Kaydet |
| Cmd+E | Son dosyalar |
| Cmd+F | Dosya içi arama çubuğu (Vim ve modeless; `Aa` / `ab` / `.*` düğmeleri, Enter sonraki, Shift+Enter önceki, Esc kapat). LESS-nvim'deki "normal modda dosya araması" (#79) LESS-rs'te kullanıcı isteğiyle değişti |
| Cmd+O | Dosya adıyla ara (eski Cmd+F) |
| Cmd+Shift+F | Projede / tüm workspace köklerinde metin ara: `Aa` / `ab` / `.*` düğmeleri, sağda eşleşmenin önizlemesi, Enter dosyayı eşleşmede açar (rg varsa onunla) |
| Cmd+/ | Yorum aç/kapa |
| Ctrl+Space | Tamamlama listesini elle aç (Vim insert + modeless; yazarken ~100 ms sonra ve sunucunun tetikleyici karakterinde (`.`) kendiliğinden açılır). Liste açıkken ↑↓ / Ctrl+N / Ctrl+P seç, Enter / Tab kabul, Esc kapat (Vim'de Esc insert'ten de çıkar), fareyle tık kabul. `(` / `,` imza yardımı. macOS'ta Ctrl+Space giriş kaynağı kısayoluysa sistem ayarından kapatılmalı |

## Taşınır: hareket

| Tuş | İş |
|---|---|
| Cmd+Sol (ilk dolu karakter, tekrar sütun 1) / Cmd+Sağ | Satır başı / sonu |
| Cmd+Yukarı / Aşağı | Dosya başı / sonu |
| Option+Sol / Sağ | Kelime atla |
| Ok tuşları, satır sonunda sağ/sol satır değiştirir | Vim modunda `hjkl` de çalışır |

## Taşınır: debug ve Run

| Tuş | İş |
|---|---|
| Cmd+F8 | Breakpoint aç/kapa (oluğa tık da) |
| F9 / F8 / F7 / Shift+F8 | Devam / step over / into / out |
| F2 | Variables'ta değer düzenle |
| Cmd+Enter | DB konsolunda sorguyu çalıştır; kod editöründe imleçteki sembolün kullanım yerleri (tekse oraya gider, çoksa liste) |
| Çift tık / Cmd+D | Kod editöründe kelimeyi seçer (Vim modunda `viw`); seçim varken Cmd+D sonraki eşleşmeyi de seçer (çoklu imleç, sona gelince baştan) |
| Cmd+D | DB veri tab'ında satır kopyala |
| Panel içi tuşlar (Run: `d r s m e l p R q`, Git: `s u S U X o d m R q`, DB: `a e d R q`, Git History: `f a y g Tab c`) | Panel eylemleri; aynen korunur |

## Taşınır: mouse

Ctrl/Option+tık tanıma git, sağ tık menüsü (References, Rename, Code Action, Send to AI), oluğa tık breakpoint,
tab çubuğu tıklamaları (sol: geç, orta/✕: kapat, sağ: menü — Pin/Unpin, Close, Close Others, Close to the Left/Right,
Close All; toplu kapatma kaydedilmemiş ve pinli tab'ları atlar, Close All pinlileri de kapatır), Files'ta tek tık önizleme
tab'ı / çift tık normal tab (#38), mini haritaya tık / sürükle kaydırır, problem çubuğu tıklama/sürükleme,
satır sonu butonları (Run, Merge), kenar sürükleyerek boyutlandırma, durum satırındaki dil sunucusu göstergesine tık → `:options`.

## Taşınır: Vim modu özel eklemeleri

| Tuş | İş |
|---|---|
| Option+J / Option+K | Satırı taşı |
| `Space D` | DB sorgusunu çalıştır |
| `Space d u` / `Space d r` | Variables / REPL |
| `Space g a` | Blame aç/kapa |
| `:` / `/` / `?` | Pencerenin üst ortasında yüzen komut satırı (`Cmdline` / `Search`, noice gibi, #87); `:`'da öneri listesi: Tab / Shift+Tab ya da ↑↓ seç, Enter çalıştır, Esc iptal, liste kapalıyken ↑↓ geçmiş |
| `:` komutları | `:Files :Code :AI :Terminal :Datasource :Run :Git :GitHistory :UIReset :GitBlame :Notes :Workspace :Theme :Font :CursorTrail :Shortcuts :Dashboard :R :Minimap :Preview :options` (`:options` = Options tab'ı: Language Servers — durum, Install / Update / Uninstall) |

## Taşınmaz (bilerek)

LazyVim leader ağacı (`Space s…`, `Space c…`, `Space b…`, `Space t…`, `Space x…`, `Space u…` büyük kısmı), flash `s`, which-key,
`Space l` (lazy), `Space n` (noice geçmişi), mason, grug-far. Karşılıkları gerekirse komut paletinden ve ayarlardan gelir.
Vim motorunun doğal tuşları (`dd`, `yy`, `ciw`, `.`, `q`, `/`, `n`, `gg`, `G`, `Ctrl+O/I` …) Vim modunda zaten vardır.

## Açık nokta

Cmd+K ve `Space` leader haritasının Vim modunda nasıl yapılandırılabilir olacağı (TOML'da `[keymap]`) Faz 1'de
`less-input` ile birlikte netleşir.
