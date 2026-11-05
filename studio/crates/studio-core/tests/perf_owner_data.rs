//! Page-load timings on a COPY of real data. Ignored by default; never run on the live database
//! (opening it migrates). Usage:
//!   SAYA_PERF_DIR=<copy with studio.sqlite3 + pass-states/saya-runs> \
//!   cargo test --release -p studio-core --test perf_owner_data -- --ignored --nocapture
use std::time::Instant;
use studio_core::{gallery::Filters, Database};

fn time<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let value = f();
    eprintln!("{:>8.1} ms  {label}", start.elapsed().as_secs_f64() * 1000.0);
    value
}

#[test]
#[ignore = "needs SAYA_PERF_DIR, a copy of real data"]
fn page_load_timings() {
    let dir = std::path::PathBuf::from(std::env::var("SAYA_PERF_DIR").expect("SAYA_PERF_DIR"));
    let db = time("open (migrations)", || Database::open(&dir.join("studio.sqlite3")).unwrap());
    time("preferences", || db.preferences().unwrap());
    time("saya_runs (cold)", || db.saya_runs().unwrap().len());
    time("saya_runs (warm)", || db.saya_runs().unwrap().len());
    time("casting_history", || db.casting_history().unwrap().len());
    time("casting_history (again)", || db.casting_history().unwrap().len());
    time("casting_previews", || db.casting_previews().unwrap());
    time("casting_choices", || db.casting_choices().unwrap().len());
    let filters = Filters::default();
    time("gallery_window(48)", || db.gallery_window(&filters, 48).unwrap().items.len());
    time("gallery_window(48) (again)", || db.gallery_window(&filters, 48).unwrap().items.len());
    time("gallery(page)", || db.gallery(&filters).unwrap().items.len());
    time("facets", || db.facets().unwrap());
    time("library_sources", || db.library_sources().unwrap().len());
    time("characters", || db.characters().unwrap().len());
    time("character_cards", || db.character_cards().unwrap().len());
    time("character_cards (again)", || db.character_cards().unwrap().len());
    time("jobs", || db.jobs().unwrap().len());
    time("workflows", || db.workflows().unwrap().len());
    // Engine reads (read-only files), when SAYA_PERF_ENGINE points at the draw engine.
    if let Some(engine) = std::env::var_os("SAYA_PERF_ENGINE").map(std::path::PathBuf::from) {
        time("casting_persos (engine registry)", || db.available_characters(&engine).unwrap().len());
        time("actions_lister (engine bank)", || {
            studio_core::actions_bank::list_actions(&engine.join(studio_core::actions_bank::ACTIONS_DATA)).unwrap()
        });
    }
}
