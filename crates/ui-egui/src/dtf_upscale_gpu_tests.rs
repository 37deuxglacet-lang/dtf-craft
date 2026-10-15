// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! AI Upscale makes a copy and enlarges it as a background job: the progress dialog shows and the
//! copy's canvas shows its pixels once the job lands. Renders the real app offscreen with the GPU
//! canvas; skips without a GPU adapter.

use crate::PhotocraftApp;
use serde_json::json;

type Harness = egui_kittest::Harness<'static, PhotocraftApp>;

fn harness() -> Option<Harness> {
    harness_with(None)
}

/// With `jpeg`, the document is that file opened (a Background without alpha, as a JPEG).
fn harness_with(jpeg: Option<String>) -> Option<Harness> {
    let built = std::panic::catch_unwind(move || {
        egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_pixels_per_point(1.0).with_max_steps(64).wgpu().build_eframe(|cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            let services = crate::Services {
                import: Some(Box::new(|name, bytes, depth| {
                    photocraft_io::import_with_svg_group_depth(name, bytes, depth).map(|r| (r.document, r.warnings)).map_err(|e| e.to_string())
                })),
                ..Default::default()
            };
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
            if let Some(rs) = cc.wgpu_render_state.as_ref() {
                app.set_wgpu(rs.clone());
            }
            if let Some(path) = &jpeg {
                app.open_file(path, &std::fs::read(path).unwrap()).unwrap();
            } else {
                app.run("file.new", json!({"width": 96, "height": 64, "resolution": 72, "background": "white"})).unwrap();
                app.run("select.rect", json!({"x": 0, "y": 0, "width": 48, "height": 64})).unwrap();
                app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
                app.run("select.deselect", json!({})).unwrap();
            }
            app.background_jobs = true;
            app.sync_views();
            app
        })
    });
    built.ok()
}

/// Share of the canvas area drawn in the picture's red.
fn red_share(h: &mut Harness) -> u32 {
    let r = h.state().last_canvas_rect;
    let img = h.render().expect("render");
    let (mut red, mut all) = (0u32, 0u32);
    for y in (r.top() as u32..r.bottom() as u32).step_by(3) {
        for x in (r.left() as u32..r.right() as u32).step_by(3) {
            if x >= img.width() || y >= img.height() {
                continue;
            }
            let p = img.get_pixel(x, y).0;
            all += 1;
            if p[0] > 200 && p[1] < 60 && p[2] < 60 {
                red += 1;
            }
        }
    }
    let _ = all;
    red
}

#[test]
fn the_upscaled_copy_shows_its_pixels_and_a_progress_dialog() {
    let Some(mut h) = harness() else { return };
    h.run_steps(8);
    let gpu = h.state().perf.gpu;
    let before = red_share(&mut h);
    h.state_mut().run("dtf.upscale", json!({"scale": "3"})).unwrap();
    assert_eq!(h.state().session.documents().len(), 2);
    let copy = h.state().session.active().unwrap().doc.id;
    assert!(h.state().session.job_on(copy).is_some(), "the enlargement runs as a job on the copy");
    assert!(h.state().session.active_job().is_some(), "the copy is active, so its progress dialog shows");
    let t0 = std::time::Instant::now();
    while h.state().session.job_on(copy).is_some() && t0.elapsed().as_secs() < 120 {
        h.run_steps(2);
    }
    h.run_steps(10);
    let st = h.state().session.active().unwrap();
    assert_eq!((st.doc.id, st.doc.size.width), (copy, 288), "the copy is enlarged ×3 and active");
    if let Some(dir) = std::env::var_os("DTF_VISUALS") {
        let _ = h.render().map(|img| img.save(std::path::Path::new(&dir).join("upscale.png")));
    }
    let share = red_share(&mut h);
    assert!(share > before * 4, "the enlarged copy shows its pixels (gpu: {gpu}): {share} red samples, original {before}");
}

