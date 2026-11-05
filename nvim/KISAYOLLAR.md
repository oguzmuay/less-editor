# Kısayollar — Neovim Workbench

> Editör arayüzü İngilizcedir; bu doküman Türkçedir. Tırnak içindeki etiketler ekranda göründüğü gibidir.

`Space` = leader tuşu. `Space`'e basıp beklersen kullanabileceğin tüm kısayollar ekranın altında listelenir.
**Kalın** olanlar bu kuruluma özel eklenenler; diğerleri LazyVim varsayılanı.

---

## Ekran düzeni

```
┌ Git/Files/DB ┬───── Kod (tablar üstte) ─────┬─ Claude Code ─┐
│ (Cmd+0)      │          (Cmd+2)             │   (Cmd+3)     │
│ (Cmd+1)      │                              │               │
│ (Cmd+5)      │                              ├───────────────┤
├──────────────┴──────────────────────────────┤  Terminal     │
│ Alt şerit: Run (Cmd+6) YA DA Git History (7)│   (Cmd+4)     │
└─────────────────────────────────────────────┴───────────────┘
```

- Sağ sütun her zaman tam boy; sırası hep **üstte Claude, altta terminal** (hangisi önce açılırsa açılsın).
- Alt şerit açıkken her zaman sol panellerin (Git / Files / Database) altına uzanır (sol panel sonradan açılsa da şeridin üstüne çıkmaz); sağ sütunun altına giremez.
- Sol paneller birbirinin yerine geçmez, yan yana açılabilir (Git en solda).
- Uzun mesajlar (ve `:Noice` geçmişi) da alt şeritte açılır, `q` ile kapanır; Run / Git History ile aynı yeri paylaşır.
- Ortadaki kod penceresi `:q` / `:wq` / `Space w d` ile kapanmaz (`:wq` yine kaydeder). Tab kapatmak Cmd+W, çıkmak `:qa` / Cmd+Q.
- Alt şeritte aynı anda **ya Run ya Git History** paneli açık olur; biri açılınca diğeri kapanır.

## Paneller

Her tuş: panel kapalıysa açar · açık ama odakta değilse odaklar · zaten odaktaysa kapatıp koda döner.

| Tuş | Leader | Panel |
|---|---|---|
| **Cmd+1** | `Space 1` | Dosyalar (neo-tree) |
| **Cmd+2** | `Space 2` | Kod — panelden geçince en baştaki **pinli** tab, pin yoksa son kullanılan editör penceresi |
| **Cmd+3** | `Space 3` | Claude Code |
| **Cmd+4** | `Space 4` | Terminal |
| **Cmd+5** | `Space 5` | Veritabanı (dadbod-ui) |
| **Cmd+6** | `Space 6` | Run (Spring Boot + Node script'leri) |
| **Cmd+7** | `Space 7` | Git History (commit geçmişi, alt şerit) |
| **Cmd+0** | `Space 0` | Git (commit edilmemiş dosyalar, sol) — stage / unstage |
| **`Space j`** | | Panel açmadan Run öğesi seç, sonra eylem (Run / Stop / Debug / Rerun…) |

## Panel komutları (`:` ile)

| Komut | Panel | Davranış |
|---|---|---|
| **`:Files`** | Dosya gezgini | Aç / kapat |
| **`:UIReset`** / **Cmd+Shift+0** | Tüm paneller | Genişlik/yükseklikleri varsayılana döndürür (AI sütunu kodu kapatınca) |
| **`:Code`** | Kod penceresi | Koda odaklanır (toggle değil) |
| **`:Claude`** | Claude Code | Aç / kapat |
| **`:Terminal`** | Terminal | Aç / kapat |
| **`:Datasource`** | Veritabanı | Aç / kapat |
| **`:Run`** | Run paneli (alt şerit) | Aç / kapat; alt şeritteki diğer aracı gizler |
| **`:Git`** | Git paneli (sol, commit edilmemiş dosyalar) | Aç / kapat |
| **`:GitHistory`** | Git History (alt şerit) | Aç / kapat; alt şeritteki diğer aracı gizler |

Komutlar saf toggle'dır: panel açıksa odakta olmasa bile kapanır (Cmd+0..7 ise önce odaklar).
Tab ile tamamlanır (ör. `:Gi` + Tab → `Git`, `GitBlame`).

## Tablar (açık dosyalar)

| Tuş | İş |
|---|---|
| **Cmd+Shift+Sol / Sağ** (veya Cmd+Option+Sol / Sağ) | Önceki / sonraki tab. Claude panelindeyken Claude tabları, Terminal panelindeyken terminal tabları arasında; diğer panellerden basılırsa önce koda geçer |
| **Cmd+T** | Claude panelinde yeni Claude oturumu · Terminal panelinde yeni shell · başka yerde yeni boş dosya |
| Mouse ile tab'a tıkla | O dosyaya geç; üzerine gelince çıkan × ile kapat |
| **Cmd+W** | Tab'ı (buffer) kapat |
| **Cmd+P** (veya `Space b p`) | Tab'ı pinle / pin'i kaldır (📌, başa geçer; oturumla kalır). Panelden basılırsa son kod tab'ına uygulanır |
| `Space b P` | Sabitlenmemiş tüm tabları kapat |
| `Space b o` | Diğer tüm tabları kapat |
| `Space b l` / `Space b r` | Soldaki / sağdaki tabları kapat |
| `[B` / `]B` | Tab'ı sola / sağa taşı |
| `Space b j` | Harfle tab seç |
| `` Space ` `` | Bir önceki tab'a dön |

## Hareket (ok tuşları)

| Tuş | İş |
|---|---|
| Oklar | İmleç; satır sonunda Sağ → alt satır, satır başında Sol → üst satır |
| **Cmd+Sol** | Satırın ilk dolu karakteri; tekrar basınca sütun 1 |
| **Cmd+Sağ** | Satır sonu |
| **Cmd+Yukarı / Aşağı** | Dosya başı / sonu |
| **Option+Sol / Sağ** | Kelime geri / ileri |
| Home / End | Satır başı / sonu |
| Page Up / Page Down | Sayfa yukarı / aşağı |
| `Ctrl+O` / `Ctrl+I` | Önceki / sonraki konuma geri dön (tanıma gittikten sonra geri gelmek için) |
| `s` + 2 harf | Ekrandaki herhangi bir yere atla (flash) |

Terminal ve Claude panelinde Cmd+Sol/Sağ shell'e satır başı/sonu, Option+Sol/Sağ kelime atlama olarak gider.

## Workspaces (farklı yerlerdeki projeler tek projeymiş gibi)

| Ne | Nasıl |
|---|---|
| Workspace listesi / aç | Açılış ekranında **`w`** · **`:Workspace`** · **`Space f w`** (açık olan ● ile işaretli; listenin sonunda "＋ Create new workspace…" ve "⚙ Manage workspaces…") |
| Yönetim menüsü | **`Space f W`** · `:Workspace manage [ad]` → Open / Add folder… / Remove folder… / Rename… / Delete… |
| Yeni workspace | `:Workspace new [ad]` (ad + ilk klasör sorulur, sonra yönetim menüsü açılır) |
| Klasör ekle | `:Workspace add [yol]` (Tab ile tamamlanır; yol yoksa: ağaçta seçili klasör / çalışma klasörü / son projeler / yol yaz) |
| Klasör çıkar · adlandır · sil | `:Workspace remove` · `:Workspace rename [ad]` · `:Workspace delete [ad]` (diskteki klasörlere dokunulmaz) |
| Doğrudan aç | `:Workspace open <ad>` (Tab ile tamamlanır) |
| Workspace'ten çık | `:Workspace close` |

Workspace açıkken: Cmd+1'de tüm klasörler üst düğüm olarak görünür, Cmd+F / Cmd+Shift+F hepsinde arar,
Cmd+6 hepsinin Spring/Node öğelerini listeler, Cmd+7'de `g` ile repo seçilir, Cmd+0 tüm repoları alt alta gösterir. Her workspace'in kendi oturumu vardır.
Listelerde mouse ile çift tık seçer.

## Dosya ve arama

| Tuş | İş |
|---|---|
| **Cmd+F** | Dosya adıyla ara (tüm repo) |
| **Cmd+Shift+F** | Dosya içeriğinde ara (tüm repo) |
| **Cmd+E** | Son açılan dosyalar |
| `Space Space` | Dosya ara (sadece bulunduğun alt proje) |
| `Space /` | İçerikte ara (sadece bulunduğun alt proje) |
| `Space ,` | Açık tablar arasında ara |
| `Space f p` | Projeler arası geçiş |
| `Space s w` | İmlecin altındaki kelimeyi projede ara |
| `Space s b` | Bu dosyanın satırlarında ara |
| `Space s r` | Projede bul-değiştir (grug-far) |
| `Space s k` | Tüm kısayolları ara |
| `/` metin, sonra `n` / `N` | Dosya içinde ara, sonraki / önceki |

Arama listelerinde: oklarla gez, Enter ile aç, Esc ile kapat.

## Düzenleme

| Tuş | İş |
|---|---|
| **Cmd+S** (veya `Ctrl+S`) | Kaydet |
| **Cmd+/** | Satırı / seçimi yorum yap |
| `i` / `a` / `o` | Yazma moduna geç (imleçten önce / sonra / alt satıra) |
| `Esc` | Normal moda dön |
| `u` / `Ctrl+R` | Geri al / yinele |
| `v` + oklar | Karakter seç · `V` satır seç |
| `y` / `d` / `p` | Seçimi kopyala / kes / yapıştır (sistem panosu ile ortak) |
| `yy` / `dd` | Satırı kopyala / sil |
| `Option+J` / `Option+K` | Satırı (veya seçimi) aşağı / yukarı taşı |
| `gco` / `gcO` | Alta / üste yorum satırı ekle |
| `Space c f` | Dosyayı formatla |

## Kod (LSP — Java, TS)

| Tuş | İş |
|---|---|
| `gd` / **Ctrl+tık** / **Option+tık** | Tanıma git |
| `gr` | Referanslar |
| `gI` | Implementasyonlar |
| `gy` | Tip tanımı |
| `K` | Dokümantasyon (hover) |
| `Ctrl+K` (yazarken) | Parametre ipucu |
| `Space c a` | Code action (import ekle, düzelt...) |
| `Space c r` | Yeniden adlandır (rename) |
| `Space c R` | Dosyayı yeniden adlandır |
| `Space c o` | Java: import'ları düzenle |
| `Space c x v` / `Space c x c` | Java: değişken / sabit çıkar |
| `Space c g s` | Java: üst sınıf metoduna git |
| `]d` / `[d` | Sonraki / önceki uyarı-hata |
| `]e` / `[e` | Sonraki / önceki hata |
| `Space c d` | Satırdaki hatayı göster |
| `Space x x` | Tüm hatalar listesi |
| `Space s s` | Dosyadaki semboller |
| `Space c l` | LSP durumu · `Space c m` Mason |
| **Sağ tık** | Find References, Rename, Code Action, Claude'a gönder |

### Problem göstergesi (sağ kenar + sağ üst sayaç)

Sadece kod pencerelerinde. Sağ kenarda yarı saydam çubuk görünen alanı gösterir; işaretler: `▬` hata/uyarı/info/hint
(renkli), `▕` / `▁` git eklenen-değişen / silinen satır, `━` arama eşleşmesi.

| Mouse / tuş | İş |
|---|---|
| **Sol tık işarete** | O satıra git (aynı yerde birden çok işaret varsa art arda tıkla) |
| **Çubuğu sürükle** | Sayfayı kaydır |
| **Sağ tık işarete** | İşaret bilgisi + satır listesi |
| Sayaçta **✖ sayısına** tık | Sonraki hata (⚠ / info / hint sayıları da kendi türüne gider) |
| Sayaçta **↓ / ↑** | Sonraki / önceki problem (önce hatalar, yoksa uyarılar; sonda başa sarar) |
| **Sağ tık sayaca** | Dosyanın problem listesi (Trouble) |
| `Space u M` | Sağ kenar işaretlerini aç / kapat |

Sayaç biçimi: ` 3   1  ↑  ↓`; problem yoksa yeşil `✓`.

Java: bir projeyi ilk açışta jdtls Maven import + indeksleme yapar (30–90 sn). Sorun olursa
`rm -rf ~/.cache/nvim/jdtls` ile cache'i silip Neovim'i yeniden aç.

## Claude Code

| Tuş | İş |
|---|---|
| **Cmd+3** | Claude panelini aç / odakla / kapat |
| `Space a c` | Claude'u aç/kapat |
| `Space a b` | Mevcut dosyayı bağlama ekle |
| `Space a s` | Seçimi gönder (visual) · dosya ağacında dosyayı ekle |
| `Space a a` / `Space a d` | Önerilen diff'i kabul et / reddet |
| `Space a r` / `Space a C` | Eski oturumu seçip sürdür (resume) / son oturuma devam (continue) — **yeni bir tab'da** açılır |
| Sağ tık → Send to Claude | Seçimi gönder (sadece seçili Claude tab'ına gider) |
| **Cmd+T** (Claude panelindeyken) · `:ClaudeNew` | Yeni Claude oturumu tab'ı |
| **Cmd+Shift+Sol / Sağ** | Summary (özet) ve Claude tabları arasında gez |
| **`:R <ad>`** | Tab'ı yeniden adlandır (Claude'un kendi `/rename` komutu da çalışır) |

**Tab adı** = Claude oturumunun adı (`/rename` ile verdiğin ad → Claude'un otomatik başlığı → "Claude N").
**Durum simgeleri:** ● working · ◐ waiting for approval/input · ○ idle (sıra sende) · ◌ starting.

**Özet tab'ı (`≡ Summary`):** Birden fazla oturum varken en solda durur. Her oturumun durumu, harcanan token (alt ajanlar dahil),
bağlam boyutu, son yaptığı iş (son araç / istek / yanıt) 2 saniyede bir güncellenir. Enter / çift tık → o oturumun tab'ına geç.

## Terminal

| Tuş | İş |
|---|---|
| **Cmd+4** | Terminal panelini aç / odakla / kapat |
| **Cmd+T** (Terminal panelindeyken) · `:TerminalNew` | Yeni shell tab'ı |
| **Cmd+Shift+Sol / Sağ** | Terminal tabları arasında gez |
| **`:R <ad>`** · `:TerminalRename <ad>` | Tab'ı yeniden adlandır (boş ad → otomatik ada dön; argümansız → adı sorar) |
| `:TermTabClose` | Tab'ı kapat (süreci sonlandırır; Claude'da onay sorar) |
| **Ctrl+Q** | Terminalden normal moda çık (kaydırma / kopyalama için); `i` ile geri dön |
| Tıklama | Terminale tıklayınca doğrudan yazmaya hazır olur |

**Tab adı:** shell'in pencere başlığı (boştayken klasör, komut çalışırken komut), yoksa çalışan süreç / klasör adı.

**Tab çubuğu (Claude ve Terminal panelinin üstü, mouse ile):** sol tık → tab'a geç · orta tık veya **✕** → kapat ·
sağ tık → yeniden adlandır · **+** → yeni tab. Süreç bitince tab kendiliğinden kalkar; tab kalmazsa panel kapanır.
Tab kapatmak için Cmd+W değil ✕ / `:TermTabClose` kullan.

## Dosya ağacı (Cmd+1)

Önizleme tab'ı, düzenleyince, kaydedince (`:w`), Cmd+P ile pinleyince ya da aynı dosyayı Cmd+F / `gd` ile açınca
normal tab olur. Düzenlenmiş dosya asla kendiliğinden kapanmaz.

| Tuş | İş |
|---|---|
| **Enter** / **tek tık** | Dosyayı **önizleme** tab'ında aç (italik; tek önizleme, sonraki dosya aynı tab'da açılır). Enter koda geçer, tek tık ağaçta kalır |
| **Aynı dosyada tekrar Enter / çift tık** | Normal tab yap |
| **Sağ ok** | Normal tab olarak aç (klasörse genişlet) |
| **Sol ok** | Klasörü kapat |
| `a` | Yeni dosya/klasör (sonu `/` ise klasör) |
| `d` / `r` | Sil / yeniden adlandır |
| `c` / `m` | Kopyala / taşı |
| `y` / `x` / `p` | Panoya kopyala / kes / yapıştır |
| `H` | Gizli dosyaları göster/gizle |
| `/` | Ağaçta filtrele |
| `P` | Yüzen önizleme penceresi |
| `?` | Ağaç kısayolları yardımı |

## Run paneli (Cmd+6)

Repo kökündeki Spring Boot uygulamalarını ve tüm `package.json` script'lerini ağaç olarak listeler.

| Tuş | İş |
|---|---|
| **Space** | Grubu / paketi kapat-aç (altındakiler gizlenir / görünür) |
| **Sağ / Sol ok** | Aç / kapat; öğedeyken Sol → üst düğüme git |
| **Enter / çift tık** | Öğede: başlat / durdur · grupta: kapat-aç |
| `d` | Debug: duruyorsa debug modunda başlat · çalışıyorsa debug moduna geçir · debugger kopmuşsa yeniden bağla |
| `r` | Rerun: durdurup aynı modla yeniden başlat (durmuşsa son modla başlat) |
| `s` | Durdur |
| `m` / **sağ tık** | Eylem menüsü (Run, Stop, Debug, Rerun, Show log, Set profile…) |
| Satır sonu butonları (tek tık) | `` Run (çalışırken `` Stop) · `` Debug · `` Rerun · `` Log |
| `l` | Logu panelin sağında aç (başlatınca otomatik açılır) |
| `p` | Spring profili seç |
| `R` | Yeniden tara |
| `q` | Paneli kapat (çalışanlar arka planda devam eder) |
| `Space j` (her yerden) | Önce öğe, sonra eylem seç |

Butonlar ve sağ tık panel odakta olmasa da çalışır.

Durum ikonları:  durdu ·  başlıyor ·  çalışıyor ·  başarıyla bitti ·  hata ile bitti ·  turuncu = debug modunda.
Debug rozeti: ` jdwp :5006 · attached` (yeşil, birden çok süreçte `×N`) · ` detached` (gri) ·
`waiting for jdtls…` / `attaching…` (sarı). Grup satırında ` n` = debug'daki öğe sayısı.

Debug notları: Spring uygulaması debugger'ı beklemeden açılır, jdtls hazır olunca (ilk açılışta 30–90 sn) bağlanır;
breakpoint'ler o andan sonra çalışır. Node'da script'in açtığı tüm node süreçlerine otomatik bağlanılır.
Debug sırasında adım atma vb. için "Debug ve test" bölümündeki `Space d …` tuşları.
Port yakalanırsa `:8080` gibi gösterilir.

## Git

| Tuş | İş |
|---|---|
| **Cmd+7** | Git History paneli |
| Panelde Enter / çift tık | Commit'in değişikliklerini kod editöründe **tab** olarak aç (`f` modunda yalnız o dosya) |
| Panelde `f` | Repo geçmişi ↔ son açtığın dosyanın geçmişi |
| Panelde `a` | Mevcut dal ↔ tüm dallar |
| Panelde `y` | Commit hash'ini kopyala |
| Panelde `g` | Workspace'te birden çok repo varsa repo seç |
| Panelde `R` / `q` | Yenile / kapat |
| **`:GitBlame`** / **`Space g a`** / sağ tık → Annotate | Kodun soluna tarih (`2026-11-05` biçiminde) + yazar sütununu aç/kapat (IntelliJ "Annotate") |
| Blame sütununda Enter / çift tık | O satırın commit diff'ini tab olarak aç · `q` sütunu kapat |
| (blame kapalıyken) | Sınıf / metot / fonksiyon satırının sonunda ` yazar +N` görünür |
| `Space g g` | Lazygit (commit, push, branch... her şey) |
| `Space g s` | Git status |
| `Space g d` | Değişiklikler (hunk'lar) |
| `Space g b` | Satırın blame bilgisi |
| `]h` / `[h` | Sonraki / önceki değişiklik bloğu |
| `Space g h s` / `Space g h r` | Bloğu stage et / geri al |
| `Space g h p` | Bloğu önizle |

### Git paneli (Cmd+0)

Sadece commit edilmemiş dosyalar; her repo için başlıkta dal (ahead/behind) ve altında dolu olan bölümler:
**Conflicts / Staged / Changes / Untracked**. Hem stage'li hem değişmiş dosya iki bölümde de görünür.
Kaydetme, odak dönüşü ve dışarıdan (ör. Claude) dosya değişikliklerinde kendiliğinden yenilenir.

| Tuş / Mouse | İş |
|---|---|
| Enter / çift tık | Dosyayı önizleme tab'ında aç (tekrar Enter → normal tab; silinmişse diff) · bölüm başlığında aç-kapat |
| `o` / `d` | Dosyayı aç / diff'ini tab olarak aç |
| `s` / `u` | Stage / unstage (dosya, bölüm ya da repo başlığında hepsi) |
| `S` / `U` | Repodaki her şeyi stage / unstage |
| `X` | Değişikliği geri al (discard) / untracked dosyayı sil — **her zaman onay sorar** |
| `m` / **sağ tık** | Eylem menüsü (panel odakta olmasa da) |
| Sağ / Sol ok | Bölümü aç / kapat |
| `R` / `q` | Yenile / kapat |

Commit için: `Space g g` (Lazygit) ya da terminal.

### Diff tab'ı

Diff'ler ekranı kaplamaz; kod editöründe normal bir tab olur (paneller yerinde kalır).

| Tuş | İş |
|---|---|
| Üstteki dosya listesinde Enter / çift tık | O dosyanın değişikliğine in |
| Değişiklik satırında Enter / çift tık | Gerçek dosyayı o satırda aç (diff tab'ı kalır) |
| `q` / Cmd+W | Diff tab'ını kapat |
| `:e` | Diff'i yenile |
| Cmd+P, Cmd+Shift+Sol/Sağ, tab'a tık | Normal tab gibi |

Diff tab'ı salt okunurdur (Cmd+S uyarı verir).

## Veritabanı (Cmd+5)

| Tuş | İş |
|---|---|
| `A` | Bağlantı ekle (ör. `mysql://kullanici:sifre@localhost:3306/db`) |
| Enter / `o` | Aç / genişlet (tabloda hazır sorgular) |
| `R` | Yenile |
| `d` | Sil |
| `r` | Yeniden adlandır |
| Sorgu dosyasında `Space S` | Sorguyu (veya seçimi) çalıştır |
| Sorgu dosyasında `Space W` | Sorguyu kaydet |

Proje bazlı bağlantılar: repo kökünde `.lazy.lua` içine `vim.g.dbs = { dev = "..." }` (gitignore'a ekle).

## Debug ve test

| Tuş | İş |
|---|---|
| `Space d b` | Breakpoint koy/kaldır |
| `Space d c` | Debug başlat / devam et |
| `Space d O` / `Space d i` / `Space d o` | Step over / into / out |
| `Space d C` | İmlece kadar çalıştır |
| `Space d t` | Debug'ı bitir |
| `Space d e` | İfade değerlendir |
| `Space t r` | İmleçteki testi çalıştır |
| `Space t t` | Dosyadaki testleri çalıştır |
| `Space t T` | Tüm testler |
| `Space t s` | Test özeti paneli |
| `Space t o` | Test çıktısı |

## Pencereler

| Tuş | İş |
|---|---|
| Tıklama | Pencereye geç |
| Kenarı mouse ile sürükle | Boyutlandır |
| `Ctrl+Sol/Sağ/Yukarı/Aşağı` | Pencereyi daralt / genişlet / boy |
| `Space -` / `Space \|` | Pencereyi alta / sağa böl |
| `Space w d` | Pencereyi kapat |

## Oturum ve genel

| Tuş | İş |
|---|---|
| `Space q q` | Çık (açık dosyalar oturum olarak kaydedilir) |
| `Space q s` | Bu klasörde son oturumu geri yükle |
| `Space q h` / `:Dashboard` | Açılış ekranı (editörün üstünde yüzer, panellere dokunmaz) |
| `Space u C` | Renk teması seç |
| `Space l` | Eklenti yöneticisi ("Plugins") |
| `Space n` | Bildirim geçmişi |
| `:` | Komut satırı |

### Açılış

- `nvim` (argümansız): bir projenin içindeysen o proje (kökü) açılır; değilsen **en son bıraktığın proje ya da
  workspace** açılır. Bulunamazsa açılış ekranı gelir.
- Her proje / workspace açılışında **Files + editör + Claude + terminal** hazır gelir, odak editörde (pinli tab).
- Açılış ekranını görmek için: `nvim +Dashboard` ya da `NVIM_NO_RESTORE=1 nvim`.
- `nvim dosya.txt`, git commit editörü gibi açılışlar son projeyi değiştirmez.
- Kapatmak: `vim.g.autorestore = false` (otomatik açma), `vim.g.default_layout = false` ya da `{ "files" }` (paneller).

### Font

| Komut | İş |
|---|---|
| `:Font` | Font seçici (`(current)` = şu anki, `(not installed)` = kurulu değil) |
| `:Font <ad>` | Doğrudan seç, Tab ile tamamlanır (ör. `:Font fira code`, `:Font monaspace radon`) |

Varsayılan JetBrains Mono. Seçim kalıcıdır ve Ghostty kendiliğinden yeniden yüklenir (olmazsa Cmd+Shift+,).
Seçenekler: JetBrains Mono, Geist Mono, Berkeley Mono (ücretli — kendin kurarsan), Cascadia Code, Fira Code,
Maple Mono, IBM Plex Mono, Monaspace Neon / Argon / Xenon / Radon / Krypton.

## LESS uygulaması

Açılış ekranında LESS logosu ve kısayollar: `f` dosya bul · `n` yeni dosya · `p` projeler · `w` workspaces ·
`s` oturumu geri yükle · `r` son dosyalar · `g` metin ara · `c` config · `l` eklentiler · `q` çık.

İlk açılışta "Setting up LESS" ekranı eklentileri kurar (bir kez, ~1–2 dk); ardından dil araçları arka planda kurulur
("Installing language tools…" bildirimi, bitince "LESS is ready."). İnternet yoksa LESS ne yapacağını söyler;
bağlantıyı düzeltip yeniden aç.

| Komut | İş |
|---|---|
| `make app` / `make run` | `build/LESS.app`'i derle / aç |
| `make install` | `/Applications/LESS.app` + `~/.local/bin/lessed` kur |
| `lessed` / `lessed <klasör>` / `lessed <dosya>` | LESS'i aç (klasörde ya da dosyayla) |

LESS kendi ayarlarını `~/.config/less/` altında tutar; normal Ghostty ve nvim'i etkilemez. İlk açılışta eklentiler
yeniden iner (birkaç dakika). Kısayolların hepsi aynı.

## Ghostty (terminal uygulaması)

| Tuş | İş |
|---|---|
| **Cmd+Shift+T** / Cmd+N | Yeni Ghostty sekmesi / penceresi (Cmd+T artık Neovim'e gider) |
| Ctrl+Tab / Ctrl+Shift+Tab | Ghostty sekmeleri arası geçiş |
| Cmd+C / Cmd+V | Kopyala / yapıştır (mouse ile seçmek de kopyalar) |
| Cmd+Shift+, | Ghostty ayarlarını yeniden yükle |
| Cmd+Shift+P | Ghostty komut paleti |
