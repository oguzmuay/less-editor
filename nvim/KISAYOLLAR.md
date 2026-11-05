# Neovim Workbench — Kısayollar

Ghostty içinde `nvim` ile çalışır. Leader = `Space`.

## Paneller
Her tuş: kapalıysa açar, açıksa odaklar, zaten odaktaysa kapatır.

| Tuş | Leader | Panel |
|---|---|---|
| Cmd+1 | `Space 1` | Dosyalar (neo-tree, sol) |
| Cmd+2 | `Space 2` | Kod (son kullanılan editör penceresi) |
| Cmd+3 | `Space 3` | Claude Code (sağ) |
| Cmd+4 | `Space 4` | Terminal (sağ, Claude açıksa onun altında) |
| Cmd+5 | `Space 5` | Veritabanı (dadbod-ui, sol) |
| Cmd+6 | `Space 6` | Spring Boot uygulamaları (alt şerit) |
| Cmd+7 | `Space 7` | Git geçmişi (alt şerit) |

Yerleşim: sağ sütun (Claude + terminal) her zaman tam boydur. Alt şerit (Spring, loglar, Git) ekranın altına,
dosya panelinin altına da uzanır ama sağ sütunun altına girmez. Birden fazla alt panel yan yana dizilir.

## Hareket (ok tuşları)
| Tuş | İş |
|---|---|
| Oklar | Her modda imleç hareketi; satır sonunda Sağ / satır başında Sol bir sonraki/önceki satıra geçer |
| Cmd+Sol | Satırın ilk dolu karakteri, tekrar basınca en başı |
| Cmd+Sağ | Satır sonu |
| Cmd+Yukarı / Cmd+Aşağı | Dosya başı / sonu |
| Option+Sol / Option+Sağ | Kelime geri / ileri |
| Dosya ağacında Sağ / Sol | Klasörü aç (dosyayı aç) / klasörü kapat |

Terminal ve Claude panelinde Cmd+Sol/Sağ satır başı/sonu, Option+Sol/Sağ kelime atlama olarak shell'e iletilir.

## Editör
| Tuş | İş |
|---|---|
| Cmd+S | Kaydet |
| Cmd+F | Dosya bul |
| Cmd+E | Son açılan dosyalar |
| Cmd+Shift+F | Projede ara |
| Cmd+W | Buffer'ı kapat |
| Cmd+/ | Yorum satırı aç/kapat |
| Ctrl+Q (terminalde) | Terminalden normal moda çık (kaydırma/kopyalama için) |
| Ctrl+Tab | Ghostty sekmeleri arasında geçiş |

## Mouse
- Tıkla: satıra/panele git. Sekmeleri tıklayarak değiştir, üzerine gelince × ile kapat.
- Dosya ağacı: çift tıkla aç.
- Ctrl+tık veya Option+tık: tanıma git.
- Sağ tık: Go to definition, Find References, Rename, Code Action, Claude'a dosya ekle/seçimi gönder.

## Claude Code (`Space a`)
| Tuş | İş |
|---|---|
| `Space ac` | Claude'u aç/kapat |
| `Space ab` | Mevcut dosyayı bağlama ekle |
| `Space as` | Seçimi gönder (visual) / ağaçtaki dosyayı ekle |
| `Space aa` / `Space ad` | Claude'un önerdiği diff'i kabul et / reddet |
| `Space ar` / `Space aC` | Önceki oturuma devam et (resume / continue) |

## Spring Boot paneli (Cmd+6)
`@SpringBootApplication` sınıflarını tarar; `mvnw`/`gradlew` varsa onunla, yoksa `mvn`/`gradle` ile çalıştırır.

| Tuş | İş |
|---|---|
| Enter / çift tık | Başlat / durdur |
| `r` | Yeniden başlat |
| `l` | Logları şeritte panelin sağında aç |
| `p` | Profil seç (`-Dspring-boot.run.profiles=`) |
| `R` | Yeniden tara |
| `Space j` | Panel açmadan hızlıca başlat/durdur |

Durum: ○ durdu · ◐ başlıyor · ● çalışıyor (port gösterilir) · ✗ hata ile çıktı

## Git geçmişi (Cmd+7)
| Tuş | İş |
|---|---|
| Enter / çift tık | Commit'in değişikliklerini diffview ile yeni sekmede aç (`q` ile kapat) |
| `f` | Repo geçmişi ↔ son açtığın dosyanın geçmişi |
| `a` | Mevcut dal ↔ tüm dallar |
| `y` | Commit hash'ini kopyala |
| `R` | Yenile (commit/checkout sonrası zaten otomatik yenilenir) |

## Veritabanı (Cmd+5)
- `A`: bağlantı ekle (ör. `mysql://user:pass@localhost:3306/db`, `postgresql://...`).
- Tabloya Enter: hazır sorgular. Sorgu buffer'ında `Space S` ile çalıştır.
- Proje bazlı bağlantılar için proje kökünde `.lazy.lua` içine `vim.g.dbs = { dev = "..." }` yaz (gitignore'a ekle).

## Java / TS
- `gd` tanım, `gr` referanslar, `K` doküman, `Space ca` code action, `Space cr` rename
- `Space d…` debug (DAP), `Space t…` testler (neotest), `Space cf` format
