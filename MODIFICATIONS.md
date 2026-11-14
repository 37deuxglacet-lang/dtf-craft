# Modifications made for DTF Craft

DTF Craft is derived from PhotoCraft (https://github.com/storytold/photocraft), commit
`ed7df56` (version 0.8.2+). As required by section 4(b) of the Apache License 2.0, this file
lists every file changed from that base (`PHOTOCRAFT-BASE.toml` records it for updates, see
`scripts/dtf-sync-photocraft.sh`). Modified Rust source files also carry a notice at the top.

All changes: Copyright (c) 2026 Stéphane Glacet, under the same terms as the original
(MIT OR Apache-2.0, at your option).

## Modified (69)

The DTF code lives in the added files; the PhotoCraft files below only hook it in.

| File | DTF change |
|---|---|
| `ATTRIBUTION.md` | DTF Craft's icons excluded from the PhotoCraft icon row |
| `Cargo.lock` | dependencies of the DTF modules (regenerate after a PhotoCraft update) |
| `LICENSE-MIT` | DTF Craft copyright line |
| `NOTICE` | DTF Craft notice and base |
| `README.md` | DTF Craft README (PhotoCraft's kept as README-PhotoCraft.md) |
| `apps/photocraft-cli/build.rs` | Windows metadata of dtfcraft-cli |
| `apps/photocraft-cli/src/lib.rs` | `--version`: DTF Craft's version, then the PhotoCraft base |
| `apps/photocraft-cli/tests/cli.rs` | test of that version line |
| `apps/photocraft/Cargo.toml` | HTTPS client for the start-screen illustration |
| `apps/photocraft/build.rs` | Windows metadata and icon of DTF Craft.exe |
| `apps/photocraft/src/app_dirs.rs` | settings folder `DTF Craft` (kept apart from PhotoCraft's) |
| `apps/photocraft/src/main.rs` | DTF Craft name, version and startup (`dtf_panel::dtf_craft_startup`) |
| `apps/photocraft/src/services.rs` | HTTPS GET service (start-screen illustration) |
| `assets/app-icon/README.md` | DTF Craft icon |
| `assets/app-icon/hicolor/128x128/apps/ai.storyteller.photocraft.png` | DTF Craft icon |
| `assets/app-icon/hicolor/16x16/apps/ai.storyteller.photocraft.png` | DTF Craft icon |
| `assets/app-icon/hicolor/24x24/apps/ai.storyteller.photocraft.png` | DTF Craft icon |
| `assets/app-icon/hicolor/256x256/apps/ai.storyteller.photocraft.png` | DTF Craft icon |
| `assets/app-icon/hicolor/32x32/apps/ai.storyteller.photocraft.png` | DTF Craft icon |
| `assets/app-icon/hicolor/48x48/apps/ai.storyteller.photocraft.png` | DTF Craft icon |
| `assets/app-icon/hicolor/512x512/apps/ai.storyteller.photocraft.png` | DTF Craft icon |
| `assets/app-icon/hicolor/64x64/apps/ai.storyteller.photocraft.png` | DTF Craft icon |
| `assets/app-icon/hicolor/scalable/apps/ai.storyteller.photocraft.svg` | DTF Craft icon |
| `assets/app-icon/photocraft-1024.png` | DTF Craft icon |
| `assets/app-icon/photocraft.icns` | DTF Craft icon |
| `assets/app-icon/photocraft.ico` | DTF Craft icon |
| `crates/algo/Cargo.toml` | tracers of the Vectorisation module |
| `crates/algo/src/lib.rs` | declares the DTF modules |
| `crates/algo/src/quantize.rs` | `spot` made `pub(crate)` (DTF Halftone) |
| `crates/engine/src/build_info.rs` | DTF Craft's version (the subscription follows it), then the base's |
| `crates/engine/src/commands.rs` | registers the DTF commands |
| `crates/engine/src/filters.rs` | DTF filter params |
| `crates/engine/src/image_cmds.rs` | `resample_doc` made `pub(crate)` (Print Conformity) |
| `crates/engine/src/jobs.rs` | AI Upscale as a background job |
| `crates/engine/src/lib.rs` | declares the DTF command modules |
| `crates/io/src/svg.rs` | declares `svg/dtf.rs` (gang sheet SVG helpers) |
| `crates/ui-egui/build.rs` | appends `i18n/dtf/<code>.tsv` to each catalog |
| `crates/ui-egui/src/brand.rs` | product name, version, links, `rebrand` |
| `crates/ui-egui/src/canvas.rs` | DTF preview source, cut-out clicks (holes, eyedropper), hands the start screen to `dtf_home.rs`; `paint_dots` and `start_screen_drop_hint` made `pub(crate)` |
| `crates/ui-egui/src/credits_tests.rs` | About test with the DTF Craft name |
| `crates/ui-egui/src/dialogs.rs` | About: DTF Craft, then its base |
| `crates/ui-egui/src/dock.rs` | DTF tools dock group |
| `crates/ui-egui/src/dock_tests.rs` | dock tests with the DTF group |
| `crates/ui-egui/src/file_ui.rs` | rebranded label |
| `crates/ui-egui/src/filter_dialog.rs` | DTF dialogs: param spec extras (cm, text defaults, descriptions), DTF Halftone tabs and lock, help sheet and tooltips |
| `crates/ui-egui/src/filter_preview_worker.rs` | previews driven by the DTF panel |
| `crates/ui-egui/src/gpu_status.rs` | rebranded label |
| `crates/ui-egui/src/i18n/live_tests.rs` | menu test with the DTF Craft name |
| `crates/ui-egui/src/i18n/mod.rs` | catalog test reads `i18n/dtf/<code>.tsv` too |
| `crates/ui-egui/src/icon_data.rs` | DTF icons |
| `crates/ui-egui/src/kys_import.rs` | rebranded message |
| `crates/ui-egui/src/kys_import_tests.rs` | test of that message |
| `crates/ui-egui/src/lib.rs` | declares the DTF modules, their state and hooks (save hint, drops on the Gang Sheet, HTTPS service) |
| `crates/ui-egui/src/links.rs` | DTF Craft links, Stéphane Glacet's sites |
| `crates/ui-egui/src/menu_catalog.rs` | DTF menu |
| `crates/ui-egui/src/menus.rs` | DTF menu commands, modal DTF windows |
| `crates/ui-egui/src/native_menu.rs` | macOS app menu name |
| `crates/ui-egui/src/panel_docking.rs` | DTF dock group |
| `crates/ui-egui/src/panels.rs` | DTF workspace first in the list, DTF Craft window title |
| `crates/ui-egui/src/prefs_ui.rs` | rebranded labels |
| `crates/ui-egui/src/screen_picker.rs` | rebranded window title |
| `crates/ui-egui/src/shortcuts.rs` | Gang Sheet handles its own keys |
| `crates/ui-egui/src/state.rs` | DTF panel state |
| `crates/ui-egui/src/theme.rs` | light theme of the Gang Sheet window |
| `crates/ui-egui/src/titlebar.rs` | no Discord button in the title bar |
| `crates/ui-egui/src/view_cmds.rs` | DTF window commands |
| `crates/ui-egui/src/widgets.rs` | typed-only number fields (Gang Sheet) |
| `crates/ui-egui/src/workspace_ui.rs` | DTF workspace |
| `xtask/src/i18n_coverage.rs` | reads `i18n/dtf/<code>.tsv` too |

## Added (40)

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
- `crates/ui-egui/src/dtf_halftone_ui.rs`
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
- `crates/algo/src/vectorize_vc.rs`
- `crates/ui-egui/src/offers.rs`
- `crates/vectorcraft-geom/` (copied unchanged from VectorCraft, see below)
- `crates/vectorcraft-trace/` (copied unchanged from VectorCraft, see below)
- `PHOTOCRAFT-BASE.toml`
- `scripts/dtf-sync-photocraft.sh`
- `crates/io/src/svg/dtf.rs`
- `crates/ui-egui/src/dtf_home.rs`
- `crates/ui-egui/src/i18n/dtf/*.tsv` (the DTF strings of each catalog)

## Removed (7)

- `.github/workflows/ci.yml`
- `.github/workflows/docs.yml`
- `.github/workflows/freebsd.yml`
- `.github/workflows/packaging-lint.yml`
- `.github/workflows/perf-nightly.yml`
- `.github/workflows/release.yml`
- `.github/workflows/windows-arm64.yml`

`crates/vectorcraft-geom/src` and `crates/vectorcraft-trace/src` are VectorCraft's `crates/geom/src` and
`crates/trace/src` (https://github.com/storytold/vectorcraft, commit `a8ec515`), Copyright (c) 2026
ArtCraft Team and the VectorCraft contributors, MIT OR Apache-2.0, unchanged; only their
`Cargo.toml` and `README.md` are DTF Craft's.

Notes: `LICENSE-MIT` and `NOTICE` carry the DTF Craft copyright lines; `README.md` is the DTF Craft
README (the original is kept as `README-PhotoCraft.md`); the Windows workflow publishes no binary in
public releases.
