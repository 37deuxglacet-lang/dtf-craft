# Modifications made for DTF Craft

DTF Craft is derived from PhotoCraft (https://github.com/storytold/photocraft), commit
`ed7df56` (version 0.8.2+). As required by section 4(b) of the Apache License 2.0, this file
lists every file changed from that base. Modified Rust source files also carry a notice at the top.

All changes: Copyright (c) 2026 Stéphane Glacet, under the same terms as the original
(MIT OR Apache-2.0, at your option).

## Modified (85)

- `Cargo.lock`
- `LICENSE-MIT`
- `NOTICE`
- `README.md`
- `apps/photocraft-cli/build.rs`
- `apps/photocraft/build.rs`
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
- `crates/algo/Cargo.toml`
- `crates/algo/src/lib.rs`
- `crates/algo/src/quantize.rs`
- `crates/engine/src/commands.rs`
- `crates/engine/src/filters.rs`
- `crates/engine/src/image_cmds.rs`
- `crates/engine/src/jobs.rs`
- `crates/engine/src/lib.rs`
- `crates/ui-egui/src/brand.rs`
- `crates/ui-egui/src/canvas.rs`
- `crates/ui-egui/src/credits_tests.rs`
- `crates/ui-egui/src/dialogs.rs`
- `crates/ui-egui/src/dock.rs`
- `crates/ui-egui/src/dock_tests.rs`
- `crates/ui-egui/src/file_ui.rs`
- `crates/ui-egui/src/filter_dialog.rs`
- `crates/ui-egui/src/filter_preview_worker.rs`
- `crates/ui-egui/src/gpu_status.rs`
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
- `crates/ui-egui/src/i18n/live_tests.rs`
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
- `crates/ui-egui/src/kys_import.rs`
- `crates/ui-egui/src/kys_import_tests.rs`
- `crates/ui-egui/src/lib.rs`
- `crates/ui-egui/src/links.rs`
- `crates/ui-egui/src/menu_catalog.rs`
- `crates/ui-egui/src/menus.rs`
- `crates/ui-egui/src/native_menu.rs`
- `crates/ui-egui/src/panel_docking.rs`
- `crates/ui-egui/src/panels.rs`
- `crates/ui-egui/src/prefs_ui.rs`
- `crates/ui-egui/src/screen_picker.rs`
- `crates/ui-egui/src/shortcuts.rs`
- `crates/ui-egui/src/state.rs`
- `crates/ui-egui/src/theme.rs`
- `crates/ui-egui/src/titlebar.rs`
- `crates/ui-egui/src/view_cmds.rs`
- `crates/ui-egui/src/workspace_ui.rs`
- `ATTRIBUTION.md`
- `apps/photocraft/Cargo.toml`
- `apps/photocraft/src/services.rs`
- `crates/ui-egui/src/icon_data.rs`
- `crates/ui-egui/src/widgets.rs`
- `apps/photocraft-cli/src/lib.rs`
- `apps/photocraft-cli/tests/cli.rs`
- `crates/engine/src/build_info.rs`

## Added (31)

- `.github/workflows/dtf-craft-windows.yml`
- `DTF-CRAFT.md`
- `README-PhotoCraft.md`
- `assets/app-icon/dtfcraft-logo.png`
- `assets/app-icon/dtfcraft-master.png`
- `crates/algo/models/LICENSE`
- `crates/algo/models/README.md`
- `crates/algo/models/realesr-general-x4v3.dtfsr`
- `crates/algo/models/test/ref_in.f32`
- `crates/algo/models/test/ref_out.f32`
- `crates/algo/src/dtf.rs`
- `crates/algo/src/gang_sheet.rs`
- `crates/algo/src/upscale.rs`
- `crates/algo/src/vectorize.rs`
- `crates/engine/src/dtf_cmds.rs`
- `crates/engine/src/gang_sheet_cmds.rs`
- `crates/ui-egui/src/dtf_help.rs`
- `crates/ui-egui/src/dtf_panel.rs`
- `crates/ui-egui/src/dtf_upscale_gpu_tests.rs`
- `crates/ui-egui/src/gang_sheet_ui.rs`
- `crates/engine/src/dtf_vector_cmds.rs`
- `crates/ui-egui/src/vectorizer_ui.rs`
- `assets/icons/external-link.svg`
- `assets/icons/graduation-cap.svg`
- `assets/icons/wrench.svg`
- `crates/algo/src/dtf_color.rs`
- `crates/engine/src/dtf_color_cmds.rs`
- `crates/ui-egui/src/color_analyzer_ui.rs`
- `crates/ui-egui/src/color_sheet.rs`
- `crates/ui-egui/src/home_image.rs`
- `crates/ui-egui/src/promo.rs`

## Removed (7)

- `.github/workflows/ci.yml`
- `.github/workflows/docs.yml`
- `.github/workflows/freebsd.yml`
- `.github/workflows/packaging-lint.yml`
- `.github/workflows/perf-nightly.yml`
- `.github/workflows/release.yml`
- `.github/workflows/windows-arm64.yml`

Notes: `LICENSE-MIT` and `NOTICE` carry the DTF Craft copyright lines; `README.md` is the DTF Craft
README (the original is kept as `README-PhotoCraft.md`); the Windows workflow publishes no binary in
public releases.
