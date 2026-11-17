# Modifications made for DTF Craft

DTF Craft is derived from PhotoCraft (https://github.com/storytold/photocraft), version 0.23.1
(commit `45066c4`; first based on commit `ed7df56`, 0.8.2+). As required by section 4(b) of the Apache License 2.0, this file
lists every file changed from that base (`PHOTOCRAFT-BASE.toml` records it for updates, see
`scripts/dtf-sync-photocraft.sh`). Modified Rust source files also carry a notice at the top.

All changes: Copyright (c) 2026 Stéphane Glacet, under the same terms as the original
(MIT OR Apache-2.0, at your option).

## Modified (97)

The DTF code lives in the added files; the PhotoCraft files below only hook it in.

| File | DTF change |
|---|---|
| `ATTRIBUTION.md` | DTF Craft's three Lucide icons counted in the icon row |
| `Cargo.lock` | dependencies of the DTF modules |
| `LICENSE-MIT` | DTF Craft copyright line |
| `NOTICE` | DTF Craft notice and base |
| `README.md` | DTF Craft README (PhotoCraft's kept as README-PhotoCraft.md) |
| `apps/photocraft-cli/build.rs` | Windows metadata of dtfcraft-cli |
| `apps/photocraft-cli/src/args.rs` | `--version`: dtfcraft-cli, DTF Craft's version, then the PhotoCraft base |
| `apps/photocraft-cli/tests/it/cli.rs` | test of that version line |
| `apps/photocraft/Cargo.toml` | HTTPS client for the start-screen illustration |
| `apps/photocraft/build.rs` | Windows metadata and icon of DTF Craft.exe |
| `apps/photocraft/src/app_dirs.rs` | settings and recovery folders `DTF Craft` (kept apart from PhotoCraft's) |
| `apps/photocraft/src/app_dirs/tests.rs` | tests of those folders |
| `apps/photocraft/src/launch_args.rs` | `--version` and usage: DTF Craft, its version, then the base |
| `apps/photocraft/src/main.rs` | DTF Craft window title and startup (`dtf_panel::dtf_craft_startup`); update module unused |
| `apps/photocraft/src/services.rs` | HTTPS GET service; no PhotoCraft update check or self-update |
| `apps/photocraft/tests/it/command_line_usage.rs` | tests of the version and usage lines |
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
| `crates/algo/src/lib.rs` | declares the DTF modules and DTF filters |
| `crates/algo/src/quantize.rs` | `spot` made `pub(crate)` (DTF Halftone) |
| `crates/engine/src/build_info.rs` | DTF Craft's version (the subscription follows it), then the base's |
| `crates/engine/src/cmd.rs` | registers the DTF commands |
| `crates/engine/src/doc_state.rs` | `save_hint` (AI Upscale copy) |
| `crates/engine/src/filters.rs` | DTF filter params |
| `crates/engine/src/jobs.rs` | AI Upscale as a background job |
| `crates/engine/src/lib.rs` | declares the DTF command modules |
| `crates/engine/src/shell_commands.rs` | Help › About DTF Craft |
| `crates/io/src/svg.rs` | declares `svg/dtf.rs` (gang sheet SVG helpers) |
| `crates/ui-egui/build.rs` | adds `src/i18n/dtf/<locale>.tsv` to each craft-i18n catalog (strings PhotoCraft also has under the `dtf` context) |
| `crates/ui-egui/examples/snapshot.rs` | `--dtf-craft` flag for DTF captures |
| `crates/ui-egui/src/app.rs` | initial state of the DTF modules |
| `crates/ui-egui/src/app/commands.rs` | cut-out's Toggle Last State |
| `crates/ui-egui/src/app/files.rs` | Save As starts from the save hint |
| `crates/ui-egui/src/app/frame.rs` | DTF windows each frame, drops onto the Gang Sheet |
| `crates/ui-egui/src/canvas/filter_preview.rs` | previews driven by the DTF panel |
| `crates/ui-egui/src/canvas/start_screen.rs` | hands the start screen to `dtf_home.rs` |
| `crates/ui-egui/src/canvas/tabs.rs` | `paint_dots` made `pub(crate)` |
| `crates/ui-egui/src/canvas/tool_input.rs` | cut-out clicks (holes, background eyedropper) |
| `crates/ui-egui/src/dialogs.rs` | About: DTF Craft, then its base; DTF dialogs in the DTF wording; DTF › Color Range… opened by its command, OK refused without a sample |
| `crates/ui-egui/src/dialogs/color_range_ui.rs` | DTF › Color Range…: the same dialog, OK erases the range on a copy (`dtf.colorRange`), no sample erases nothing, its help sheet |
| `crates/ui-egui/src/dialogs/credits/ui_tests.rs` | About test with the DTF Craft name |
| `crates/ui-egui/src/dialogs/file_ui/print.rs` | rebranded label |
| `crates/ui-egui/src/dialogs/filter_dialog.rs` | DTF dialogs: param spec extras (cm, text defaults, descriptions), DTF Halftone tabs and lock, help sheet and tooltips, cut-out preview |
| `crates/ui-egui/src/dialogs/filter_dialog/tests.rs` | test of the text defaults |
| `crates/ui-egui/src/dialogs/filter_preview_worker.rs` | previews driven by the DTF panel |
| `crates/ui-egui/src/dialogs/screen_picker.rs` | rebranded window title |
| `crates/ui-egui/src/dock.rs` | DTF tools dock group |
| `crates/ui-egui/src/dock/tests.rs` | dock tests with the DTF group |
| `crates/ui-egui/src/gpu_status.rs` | rebranded label |
| `crates/ui-egui/src/i18n/live_tests.rs` | menu test with the DTF Craft name |
| `crates/ui-egui/src/i18n.rs` | DTF windows read the `dtf` rows first (`dtf_i18n.rs`) |
| `crates/ui-egui/src/icon_data.rs` | DTF icons |
| `crates/ui-egui/src/input/shortcuts.rs` | DTF full-window modules keep the keyboard |
| `crates/ui-egui/src/lib.rs` | declares the DTF modules and their state |
| `crates/ui-egui/src/links.rs` | DTF Craft links, Stéphane Glacet's sites |
| `crates/ui-egui/src/menu_catalog.rs` | DTF menu, DTF workspace |
| `crates/ui-egui/src/menus.rs` | DTF menu, modal DTF windows |
| `crates/ui-egui/src/menus/invoke.rs` | DTF menu commands, DTF workspace |
| `crates/ui-egui/src/menus/items.rs` | training and quote requests in the menus |
| `crates/ui-egui/src/menus/menu_keys/harness_tests.rs` | keyboard tests with the DTF menu between Filter and View |
| `crates/ui-egui/src/menus/menu_nav/tests.rs` | tests with About DTF Craft |
| `crates/ui-egui/src/menus/native_menu.rs` | macOS app menu name |
| `crates/ui-egui/src/menus/native_menu/tests.rs` | tests of that menu |
| `crates/ui-egui/src/menus/state.rs` | DTF commands enabled and checked |
| `crates/ui-egui/src/menus/tests.rs` | tests with About DTF Craft |
| `crates/ui-egui/src/menus/view_cmds/dialogs.rs` | AI Upscale and Print Conformity dialogs |
| `crates/ui-egui/src/menus/view_cmds/dispatch.rs` | AI Upscale and Print Conformity dialogs |
| `crates/ui-egui/src/menus/workspace_ui.rs` | DTF workspace first in the switcher |
| `crates/ui-egui/src/panels.rs` | window title tests |
| `crates/ui-egui/src/panels/brand.rs` | product name, version, links, `rebrand` |
| `crates/ui-egui/src/panels/dock.rs` | DTF dock group (empty: the tools open in windows) |
| `crates/ui-egui/src/panels/panel_docking.rs` | DTF dock group |
| `crates/ui-egui/src/panels/title_bar.rs` | DTF Craft title |
| `crates/ui-egui/src/prefs/kys_import.rs` | rebranded message |
| `crates/ui-egui/src/prefs/kys_import/tests.rs` | test of that message |
| `crates/ui-egui/src/prefs/prefs_ui/fields.rs` | rebranded label |
| `crates/ui-egui/src/prefs/prefs_ui/shortcuts.rs` | rebranded message |
| `crates/ui-egui/src/services.rs` | HTTPS GET service |
| `crates/ui-egui/src/state.rs` | DTF panel state |
| `crates/ui-egui/src/state/color_panel.rs` | DTF dock tab |
| `crates/ui-egui/src/theme.rs` | light theme of the Gang Sheet window |
| `crates/ui-egui/src/widgets.rs` | exports the typed-only field |
| `crates/ui-egui/src/widgets/fields.rs` | typed-only number fields (Gang Sheet) |
| `crates/ui-egui/src/widgets/tests.rs` | test of the typed-only field |
| `crates/ui-egui/src/widgets/units.rs` | typed-only number fields (Gang Sheet) |

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
- `crates/ui-egui/src/dtf_help.rs`
- `crates/ui-egui/src/dtf_i18n.rs`
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
- `crates/ui-egui/src/i18n/dtf/*.tsv` (the DTF strings of each language, added to the craft-i18n catalogs at build)

## Removed (11)

- `.github/dependabot.yml`
- `.github/workflows/ci.yml`
- `.github/workflows/docs.yml`
- `.github/workflows/freebsd.yml`
- `.github/workflows/house-rules.yml`
- `.github/workflows/nix.yml`
- `.github/workflows/packaging-lint.yml`
- `.github/workflows/perf-nightly.yml`
- `.github/workflows/release.yml`
- `.github/workflows/sonarcloud.yml`
- `.github/workflows/windows-arm64.yml`

`crates/vectorcraft-geom/src` and `crates/vectorcraft-trace/src` are VectorCraft's `crates/geom/src` and
`crates/trace/src` (https://github.com/storytold/vectorcraft, commit `a8ec515`), Copyright (c) 2026
ArtCraft Team and the VectorCraft contributors, MIT OR Apache-2.0, unchanged; only their
`Cargo.toml` and `README.md` are DTF Craft's.

Notes: `LICENSE-MIT` and `NOTICE` carry the DTF Craft copyright lines; `README.md` is the DTF Craft
README (the original is kept as `README-PhotoCraft.md`); the Windows workflow publishes no binary in
public releases.
