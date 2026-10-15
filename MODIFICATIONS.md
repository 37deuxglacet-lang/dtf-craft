# Modifications made for DTF Craft

DTF Craft is derived from PhotoCraft (https://github.com/storytold/photocraft), commit
`ed7df56` (version 0.8.2+). As required by section 4(b) of the Apache License 2.0, this file
lists every file changed from that base. Modified Rust source files also carry a notice at the top.

All changes: Copyright (c) 2026 Stéphane Glacet, under the same terms as the original
(MIT OR Apache-2.0, at your option).

## Modified (46)

- `apps/photocraft/src/app_dirs.rs`
- `apps/photocraft/src/main.rs`
- `assets/app-icon/README.md`
- `assets/app-icon/hicolor/128x128/apps/ai.storyteller.photocraft.png`
- `assets/app-icon/hicolor/16x16/apps/ai.storyteller.photocraft.png`
- `assets/app-icon/hicolor/24x24/apps/ai.storyteller.photocraft.png`
- `assets/app-icon/hicolor/256x256/apps/ai.storyteller.photocraft.png`
- `assets/app-icon/hicolor/32x32/apps/ai.storyteller.photocraft.png`
- `assets/app-icon/hicolor/48x48/apps/ai.storyteller.photocraft.png`
- `assets/app-icon/hicolor/512x512/apps/ai.storyteller.photocraft.png`
- `assets/app-icon/hicolor/64x64/apps/ai.storyteller.photocraft.png`
- `assets/app-icon/hicolor/scalable/apps/ai.storyteller.photocraft.svg`
- `assets/app-icon/photocraft-1024.png`
- `assets/app-icon/photocraft.icns`
- `assets/app-icon/photocraft.ico`
- `crates/algo/src/lib.rs`
- `crates/algo/src/quantize.rs`
- `crates/engine/src/commands.rs`
- `crates/engine/src/filters.rs`
- `crates/engine/src/image_cmds.rs`
- `crates/engine/src/lib.rs`
- `crates/ui-egui/src/filter_dialog.rs`
- `crates/ui-egui/src/i18n/cs.tsv`
- `crates/ui-egui/src/i18n/de.tsv`
- `crates/ui-egui/src/i18n/el.tsv`
- `crates/ui-egui/src/i18n/es.tsv`
- `crates/ui-egui/src/i18n/fr.tsv`
- `crates/ui-egui/src/i18n/hu.tsv`
- `crates/ui-egui/src/i18n/id.tsv`
- `crates/ui-egui/src/i18n/it.tsv`
- `crates/ui-egui/src/i18n/ja.tsv`
- `crates/ui-egui/src/i18n/ko.tsv`
- `crates/ui-egui/src/i18n/nl.tsv`
- `crates/ui-egui/src/i18n/pl.tsv`
- `crates/ui-egui/src/i18n/pt-br.tsv`
- `crates/ui-egui/src/i18n/ru.tsv`
- `crates/ui-egui/src/i18n/sr.tsv`
- `crates/ui-egui/src/i18n/tr.tsv`
- `crates/ui-egui/src/i18n/uk.tsv`
- `crates/ui-egui/src/i18n/vi.tsv`
- `crates/ui-egui/src/i18n/zh-hans.tsv`
- `crates/ui-egui/src/i18n/zh-hant.tsv`
- `crates/ui-egui/src/menu_catalog.rs`
- `crates/ui-egui/src/menus.rs`
- `crates/ui-egui/src/native_menu.rs`
- `crates/ui-egui/src/view_cmds.rs`

## Added (12)

- `.github/workflows/dtf-craft-windows.yml`
- `DTF-CRAFT.md`
- `assets/app-icon/dtfcraft-logo.png`
- `assets/app-icon/dtfcraft-master.png`
- `crates/algo/models/LICENSE`
- `crates/algo/models/README.md`
- `crates/algo/models/realesr-general-x4v3.dtfsr`
- `crates/algo/models/test/ref_in.f32`
- `crates/algo/models/test/ref_out.f32`
- `crates/algo/src/dtf.rs`
- `crates/algo/src/upscale.rs`
- `crates/engine/src/dtf_cmds.rs`

## Removed (7)

- `.github/workflows/ci.yml`
- `.github/workflows/docs.yml`
- `.github/workflows/freebsd.yml`
- `.github/workflows/packaging-lint.yml`
- `.github/workflows/perf-nightly.yml`
- `.github/workflows/release.yml`
- `.github/workflows/windows-arm64.yml`

Added after this list was generated: `MODIFICATIONS.md`; `LICENSE-MIT` and `NOTICE` (DTF Craft
copyright lines); `README.md` (replaced by the DTF Craft README; the original is kept as
`README-PhotoCraft.md`); `.github/workflows/dtf-craft-windows.yml` (no binary in public releases).
