# Öneriler

Henüz uygulanmamış, kullanıcının onayını bekleyen öneriler. Biri uygulanınca buradan silinip `CLAUDE.md` karar
listesine ve `nvim/KISAYOLLAR.md`'ye eklenir.

## Sayfa içinde hızlı hareket

### 1. Yapışkan başlık (sticky lines)
IntelliJ'deki gibi, uzun bir metodun içindeyken pencerenin üstünde hangi sınıf ve metotta olduğun sabit durur;
o satıra tıklayınca metodun başına gidilir.
- Uygulama: LazyVim extra'sı `ui.treesitter-context` (nvim-treesitter-context), `lua/config/lazy.lua`'ya tek satır.
- Dikkat: kod pencerelerinde winbar artık problem sayacına ait; treesitter-context winbar'ın altında çizilir, test et.

### 2. Yapı paneli (Outline / Structure)
IntelliJ "Structure" gibi, dosyanın sınıf / metot / alan ağacını gösteren panel; tıklayınca o sembole gidilir,
imleç hareket ettikçe panelde ilgili sembol vurgulanır.
- Uygulama: LazyVim extra'sı `editor.outline` (outline.nvim) ya da `editor.aerial`.
- Önerilen kısayol: **Cmd+8** + `:Outline` komutu; workbench panel sistemine (aç / odakla / kapat) dahil edilmeli.
- Yerleşim: sol panel olarak (Files / DB gibi) ya da sağ sütunda — karar verilmeli.

### 3. Mac / IntelliJ tarzı hata gezinme
Ok tuşu kullanıcısı için `]e` / `[e` yerine:
- **F2 / Shift+F2**: sonraki / önceki hata (yoksa uyarı) — IntelliJ ile aynı.
- Sağ kenar göstergesi ajanı da aynısını önerdi; bağlanacak çağrı `require("problems").jump(vim.api.nvim_get_current_win(), 1)`
  (Shift+F2 için `-1`) — sayaçtaki ↑↓ ile aynı davranış. Option+Aşağı/Yukarı alternatif olarak bu işe de verilebilir
  (4. öneriyle çakışır, birini seç).
- Ghostty'de F tuşları doğrudan Neovim'e gider; ek Ghostty ayarı gerekmez (macOS'ta "F tuşlarını standart
  fonksiyon tuşu olarak kullan" açık değilse Fn+F2).

### 4. Metotlar arası atlama
- **Option+Yukarı / Option+Aşağı**: önceki / sonraki metodun (fonksiyonun) başı.
- Uygulama: treesitter textobjects (`]f` / `[f` LazyVim'de mevcut) bu tuşlara bağlanır; Ghostty'de
  `alt+arrow_up/down` için CSI ayarı gerekebilir.
- Dikkat: LazyVim'de Option+J/K satır taşıma ile karışmamalı.

## Hâlihazırda kullanılabilenler (hatırlatma)

| Ne için | Tuş |
|---|---|
| Ekranda gördüğün herhangi bir yere atla | `s` + 2 harf + etiket |
| Dosyadaki sembollerden seçip git | `Space s s` |
| Sonraki / önceki hata · uyarı · hepsi | `]e` `[e` · `]w` `[w` · `]d` `[d` |
| Sonraki / önceki git değişikliği | `]h` / `[h` |
| İmleçteki kelimenin diğer kullanımları | `]]` / `[[` |
| Geri / ileri konum | `Ctrl+O` / `Ctrl+I` |
| Eşleşen parantez | `%` |
| Yarım sayfa aşağı / yukarı | `Ctrl+D` / `Ctrl+U` |
| Dosya başı / sonu | Cmd+Yukarı / Cmd+Aşağı |
| Satıra git | `:120` + Enter |

## Diğer bekleyen öneriler

- **Workspace'te Claude Code:** Claude workspace'in symlink klasöründe başlıyor; Grep/Glob symlink izlemezse arama
  eksik kalabilir. Öneri: claudecode'a `terminal.cwd_provider` (ilk kök) + diğer kökler için `--add-dir`.
- **PostgreSQL istemcisi:** `psql` kurulu değil; Postgres kullanılacaksa `brew install libpq` (PATH'e zaten ekli).
- **Spring yapılandırma tamamlama:** `application.yml/properties` için `spring-boot.nvim`.
- **Shift+Ok ile seçim** (Mac tarzı metin seçimi).
