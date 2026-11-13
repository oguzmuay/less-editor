# Paylaşılan crate'ler submodule oldu — handoff (2026-11-13)

Durum: **less-core, less-input, less-theme, less-ai kendi public repolarına taşındı ve `crates/` altına git submodule olarak geri eklendi.**

## Yapılanlar

- **Taşınan repolar:** `github.com/oguzmuay/less-core`, `less-input`, `less-theme`, `less-ai`. Cargo.toml'ları artık workspace
  alanlarını miras almaz (sürüm 0.1.0, edition 2024, MIT, kendi lint'leri).
- **Yan yana kuralı:** less-input, less-core'a `../less-core` ile bağlıdır; ikisi her tüketicide kardeş klasör olmalıdır.
- **Neden:** CEF tabanlı yeni tarayıcı projesi (less-browser: klavye odaklı + gömülü AI ajanı) bu crate'leri paylaşır;
  orada da less-core ve less-input submodule'dür.
- **less-input:** `PageEngine` (`page.rs`: j/k, d/u, Ctrl-d/u, gg/G, sayılar → `PageAction`) ve isteğe bağlı `egui` özelliği
  (`less_input::egui::{translate, to_less_key}`). Editörün egui→KeyEvent çevirisi buraya taşındı; `less-app/src/keys.rs`
  `translate`'i re-export eder, `less-app` `features = ["egui"]` açar.
- **Cargo.toml:** `[profile.dev] debug = "line-tables-only"` ve `[profile.dev.package."*"] debug = false` (target/debug 25G idi).
- **CI:** `ci.yml` ve `release.yml` `submodules: recursive` ile checkout eder.

## Çalışma düzeni

- Klonlama: `git clone --recursive` ya da `git submodule update --init`.
- Paylaşılan crate'i değiştirmek: submodule içinde commit + push, sonra burada yükselen submodule işaretçisini commit et.
- Planlı: bu klasör kullanıcı tarafından `/Users/oguzugurdogan/Documents/projects/less/less-editor` altına, less-browser'ın yanına taşınacak.

## Bilinen sorun

- less-lang testi `lsp::tests::spring_boot_relays_java_data_and_classpath_and_merges_hover` kararsız (4 koşuda 2 kez düştü); bu değişiklikle ilgisiz.
