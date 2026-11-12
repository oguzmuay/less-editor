# LESS-rs: kalan planlar (oturum başına bir dosya)

Kaynak: `../README.md` (devir notu, 2026-11-11), `../A1..A4.md`, `../../WAVES.md`.
Kullanım: yeni oturumda "`less-rs/docs/handoff/plans/<dosya>` planını uygula" de. Model, oturumu açarken seçilir.

| Sıra | Plan | Önerilen model | Ön koşul | Paralel mi |
|---|---|---|---|---|
| 1 | [A1 core-input](A1-core-input.md) — less-core modüllerini bağla, `editor.rs`, less-input (Vim/Modeless/ex) | **Opus** | — | A2, A3 ile paralel |
| 1 | [A2 lang](A2-lang.md) — tooltip'i bitir, snapshot + artımlı testler, clippy | **Opus** (Sonnet yeter) | — | A1, A3 ile paralel |
| 1 | [A3 run](A3-run.md) — DAP/shutdown/variables testleri, clippy | **Opus** | — | A1, A2 ile paralel |
| 2 | [B0 host](B0-host.md) — less-app panel ev sahibi + kod görünümü | **Opus** | A1, A2 | tek başına |
| 3 | [B1 git-ui](B1-git-ui.md) | **Sonnet** | B0 | B1–B5 paralel |
| 3 | [B2 run-ui](B2-run-ui.md) | **Opus** | B0 (+A3) | B1–B5 paralel |
| 3 | [B3 ai-ui](B3-ai-ui.md) | **Opus** | B0 | B1–B5 paralel |
| 3 | [B4 db-ui](B4-db-ui.md) | **Sonnet** | B0 | B1–B5 paralel |
| 3 | [B5 lang-ui](B5-lang-ui.md) | **Opus** | B0 | B1–B5 paralel |
| 4 | [C1 mcp](C1-mcp.md) | **Opus** | Dalga B | C2 ile paralel |
| 4 | [C2 extras](C2-extras.md) | **Sonnet** | Dalga B | C1 ile paralel |
| 5 | [C3 integrate](C3-integrate.md) | **Opus** | C1, C2 | tek başına |

Bitmiş (plan gerekmez): A4 term-ai (Opus), A5 git (Sonnet), A6 db (Sonnet), A7 shell (Sonnet).

Model önerileri `WAVES.md` tablosundan; A2 için Sonnet notu benim eklememdir (kalan iş test + küçük tamamlama).
