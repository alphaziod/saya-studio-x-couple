//! Bounded real-GPU proof that stopping a Saya run stops every phase ComfyUI owns:
//! the executing prompt is interrupted and the queued one is deleted from the queue.
//! Ignored by default. Isolated temp DB/results; SFW still-life prompt, fresh seeds.
//! Run: `cargo test -p studio-core --test real_gpu_run_stop -- --ignored --nocapture`
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use studio_core::{comfy::Comfy, Database};

const CKPT: &str = "Illustrious/no tags/ilustmix_v9.safetensors";

fn graph(seed: u64, prefix: &str) -> Value {
    json!({
        "1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":CKPT}},
        "2":{"class_type":"CLIPTextEncode","inputs":{"text":"still life, red apple on a wooden table, soft daylight","clip":["1",1]}},
        "3":{"class_type":"CLIPTextEncode","inputs":{"text":"blurry, lowres","clip":["1",1]}},
        "4":{"class_type":"EmptyLatentImage","inputs":{"width":1024,"height":1024,"batch_size":1}},
        "5":{"class_type":"KSampler","inputs":{"seed":seed,"steps":40,"cfg":5.0,"sampler_name":"euler","scheduler":"normal","denoise":1.0,"model":["1",0],"positive":["2",0],"negative":["3",0],"latent_image":["4",0]}},
        "6":{"class_type":"VAEDecode","inputs":{"samples":["5",0],"vae":["1",2]}},
        "7":{"class_type":"SaveImage","inputs":{"images":["6",0],"filename_prefix":prefix}}
    })
}

fn fresh_seed() -> u64 {
    u64::from_le_bytes(uuid::Uuid::new_v4().as_bytes()[..8].try_into().unwrap()) % 1_000_000_000_000
}

fn job_state(db: &Database, job: &str) -> String {
    db.jobs()
        .unwrap()
        .into_iter()
        .find(|j| j["id"] == job)
        .and_then(|j| j["state"].as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn prompt_id(db: &Database, job: &str) -> String {
    db.jobs()
        .unwrap()
        .into_iter()
        .find(|j| j["id"] == job)
        .and_then(|j| j["promptId"].as_str().map(str::to_owned))
        .unwrap_or_else(|| panic!("promptId missing for {job}"))
}

fn in_queue(queue: &Value, list: &str, prompt: &str) -> bool {
    queue[list]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item[1] == prompt))
}

#[test]
#[ignore = "real GPU ComfyUI; two SFW prompts, both stopped before completion"]
fn stopping_a_run_interrupts_the_running_phase_and_dequeues_the_next() {
    let comfy = Comfy::loopback(8188).unwrap();
    let info = comfy.get("/object_info").expect("ComfyUI must be running on 8188");
    let idle = comfy.get("/queue").unwrap();
    assert!(
        idle["queue_running"].as_array().is_some_and(Vec::is_empty)
            && idle["queue_pending"].as_array().is_some_and(Vec::is_empty),
        "ComfyUI queue must be empty: this test never touches other jobs"
    );
    let dir = std::env::temp_dir().join(format!("saya-gpu-stop-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let results = dir.join("results");
    let mut db = Database::open(&dir.join("studio.sqlite3")).unwrap();

    let mut jobs = Vec::new();
    for prefix in ["saya-gpu-stop-running", "saya-gpu-stop-queued"] {
        let workflow = db
            .import_workflow(prefix, &graph(fresh_seed(), prefix).to_string())
            .unwrap();
        let prepared = db.prepare_job(&workflow, &[], &info).unwrap();
        db.submit_job(&prepared.job_id, &prepared.snapshot_hash, true, true, &comfy)
            .unwrap();
        jobs.push(prepared.job_id);
    }
    let (running, queued) = (jobs[0].clone(), jobs[1].clone());
    let (running_prompt, queued_prompt) = (prompt_id(&db, &running), prompt_id(&db, &queued));

    // Wait until the first prompt executes while the second still waits in the queue.
    let start = Instant::now();
    loop {
        let queue = comfy.get("/queue").unwrap();
        if in_queue(&queue, "queue_running", &running_prompt)
            && in_queue(&queue, "queue_pending", &queued_prompt)
        {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(120), "queue never reached running+pending: {queue}");
        std::thread::sleep(Duration::from_millis(200));
    }

    // A two-phase run owning both prompts, written where `saya_runs` reads manifests.
    let run = uuid::Uuid::new_v4().to_string();
    let manifest = json!({"id":run,"state":"running","autoAdvance":true,
        "phases":{"1":{"job":running,"state":"submitted"},"2":{"job":queued,"state":"submitted"}}});
    let runs = db.pass_state_root().unwrap().join("saya-runs");
    std::fs::create_dir_all(&runs).unwrap();
    std::fs::write(runs.join(format!("{run}.json")), manifest.to_string()).unwrap();

    let stopped = db.saya_stop(&run, &comfy, &results).unwrap();
    assert_eq!(stopped["state"], "stopped");
    assert_eq!(stopped["stopPending"], json!(false), "both controls sent: {stopped}");

    let start = Instant::now();
    loop {
        db.sync_jobs(&comfy, &results).unwrap();
        let (a, b) = (job_state(&db, &running), job_state(&db, &queued));
        if a == "interrupted" && b == "cancelled" {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(60), "running={a} queued={b}");
        std::thread::sleep(Duration::from_millis(300));
    }
    let queue = comfy.get("/queue").unwrap();
    assert!(!in_queue(&queue, "queue_pending", &queued_prompt), "queued prompt left: {queue}");
    assert!(!in_queue(&queue, "queue_running", &running_prompt), "running prompt left: {queue}");
    eprintln!("GPU stop proof OK: running={running_prompt} interrupted, queued={queued_prompt} deleted");
    let _ = std::fs::remove_dir_all(&dir);
}
