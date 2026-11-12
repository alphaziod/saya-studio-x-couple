//! Saya Couple review gate bridge. The custom node remains authoritative: this
//! only forwards an explicit human decision to its real validate/redo routes.
use crate::{comfy::Comfy, Database, Result};
use serde_json::{json, Value};

/// The Saya Couple workflow publishes six phase boundaries (phase 1 base,
/// phases 2-6 automatic). The bridge therefore tracks and advances every phase.
const SAYA_PHASES: u32 = 6;

impl Database {
    pub fn saya_review_decision(
        &self,
        job: &str,
        action: &str,
        client: &Comfy,
        confirmed: bool,
    ) -> Result<Value> {
        if !confirmed {
            return Err("Décision humaine explicite requise".into());
        }
        let outputs: String = self
            .connection
            .query_row(
                "SELECT outputs_json FROM generation_jobs WHERE id=?",
                [job],
                |r| r.get(0),
            )
            .map_err(|_| "Job Saya inconnu")?;
        let outputs: Value = serde_json::from_str(&outputs)?;
        let review = outputs
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|o| {
                o.get("sayaReview")
                    .and_then(|r| r.as_array())
                    .and_then(|a| a.first())
            })
            .ok_or("Review gate Saya absent pour ce job")?;
        let phase = review["phase"].as_u64().unwrap_or(1);
        let transaction = review["transaction_uuid"].as_str().unwrap_or_default();
        let root = review["checkpoint_root"]
            .as_str()
            .unwrap_or("image/checkpoints");
        let (path, payload) = match action {
            "validate" | "continue" => (
                "/saya/image-phases/validate",
                json!({"phase":phase,"detailer":"none","checkpoint_root":root,"transaction_uuid":transaction}),
            ),
            "redo" | "restart" => (
                "/saya/image-phases/redo",
                json!({"phase":phase,"checkpoint_root":root}),
            ),
            _ => return Err("Décision Saya inconnue".into()),
        };
        let response = client.post_json(path, payload)?;
        Ok(json!({"job":job,"action":action,"phase":phase,"response":response}))
    }
}

/// Shared-base providers (`saya_shared_base`) are authored once and reused by
/// every phase, but they keep their authoring phase tag. The phase-3 routing
/// rule keeps only phase-3 and HiDream-tagged nodes, which mutes those
/// providers and leaves the active phase-3 prompt encoders with an unresolved
/// `clip`/`model`. Re-tag the shared providers for this conversion only so the
/// converter keeps their original mode and the real wires are preserved.
///
/// The optional model hubs are untagged subgraph instances (`saya-model-hub-*`)
/// whose saved mode is not a phase signal: the editor re-saves them active when
/// the imported copy was fully activated (`image_restore::activate_workflow`),
/// muted when another phase was last edited. Tag routing cannot restore them,
/// so the converted phase would receive unresolved MODEL inputs from providers
/// owned by another phase. Gate every hub on this private copy for EVERY phase:
/// only the hub owned by the requested phase stays active; the source workflow
/// is never written. A hub owned by no listed phase keeps its saved mode.
fn prepare_phase_source(source: &str, phase: u32) -> Result<String> {
    let mut workflow: Value = serde_json::from_str(source)?;
    let mut shared = std::collections::HashSet::new();
    collect_shared_types(&workflow, &mut shared);
    retag_shared(&mut workflow, phase, &shared, false);
    activate_phase_hubs(&mut workflow, phase);
    Ok(workflow.to_string())
}

/// Keep only the nodes reachable from an output node (`output_node`
/// in object_info). The engine does the same closure before sending
/// (`comfy.minimal_prompt`, SAVE_NODE root): dead nodes of the UI graph —
/// for example the unwired PRECHARGE loaders of the embedded workflow — are
/// never submitted to ComfyUI. This reduces the payload; their mere presence in
/// the API proves neither their execution nor the cause of an OOM: ComfyUI also
/// starts from the outputs. If no output is identifiable, the graph is left intact.
pub fn prune_unreachable_nodes(api: &mut Value, info: &Value) {
    let Some(nodes) = api.as_object_mut() else {
        return;
    };
    let is_output_node = |class: &str| {
        info.get(class)
            .and_then(|c| c.get("output_node"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    let output_ids: Vec<String> = nodes
        .iter()
        .filter(|(_, node)| node["class_type"].as_str().is_some_and(|ct| is_output_node(ct)))
        .map(|(id, _)| id.clone())
        .collect();
    if output_ids.is_empty() {
        return;
    }
    let mut reached: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut stack: Vec<String> = output_ids;
    while let Some(id) = stack.pop() {
        if !reached.insert(id.clone()) {
            continue;
        }
        let Some(inputs) = nodes
            .get(&id)
            .and_then(|n| n.get("inputs"))
            .and_then(Value::as_object)
        else {
            continue;
        };
        for input_value in inputs.values() {
            if let Some(link) = input_value.as_array().filter(|l| l.len() >= 1) {
                if let Some(origin_id) = link[0].as_str() {
                    if nodes.contains_key(origin_id) && !reached.contains(origin_id) {
                        stack.push(origin_id.to_owned());
                    }
                }
            }
        }
    }
    nodes.retain(|id, _| reached.contains(id));
}

/// ComfyUI exposes COMBO outputs as an array of allowed strings (for example
/// the JPS sampler/scheduler settings node linked to `sampler_name` and
/// `scheduler`). `validate_graph` only understands scalar type names and would
/// reject those real links. Validation therefore gets a schema view where COMBO
/// outputs are wildcards; the links themselves are never rewritten.
fn schema_for_validation(info: &Value) -> Value {
    let mut info = info.clone();
    for (_, class) in info.as_object_mut().into_iter().flatten() {
        let Some(outputs) = class["output"].as_array_mut() else {
            continue;
        };
        for output in outputs {
            if output.is_array() {
                *output = json!("*");
            }
        }
    }
    info
}

/// Optional model hubs and the phase that owns each one. A hub owned by no
/// entry (for example `saya-model-hub-usdu`, wired to always-active providers)
/// keeps its saved mode in every phase.
const PHASE_HUBS: &[(&str, u32)] = &[
    ("saya-model-hub-detailers", 5),
    ("saya-model-hub-phase6", 6),
];

/// Gate every listed hub for the requested phase: the owner runs, the others
/// are muted exactly like the saved workflow keeps them outside their phase.
fn activate_phase_hubs(workflow: &mut Value, phase: u32) {
    if let Some(nodes) = workflow.get_mut("nodes").and_then(Value::as_array_mut) {
        for node in nodes {
            let Some(kind) = node["type"].as_str() else {
                continue;
            };
            if let Some((_, owner)) = PHASE_HUBS.iter().find(|(name, _)| *name == kind) {
                node["mode"] = json!(if *owner == phase { 0 } else { 2 });
            }
        }
    }
}

fn collect_shared_types(scope: &Value, out: &mut std::collections::HashSet<String>) {
    if let Some(nodes) = scope.get("nodes").and_then(Value::as_array) {
        for node in nodes {
            if node["properties"]["saya_shared_base"].as_bool() == Some(true) {
                if let Some(kind) = node["type"].as_str() {
                    out.insert(kind.to_owned());
                }
            }
        }
    }
    if let Some(definitions) = scope
        .pointer("/definitions/subgraphs")
        .and_then(Value::as_array)
    {
        for definition in definitions {
            collect_shared_types(definition, out);
        }
    }
}

fn retag_shared(
    scope: &mut Value,
    phase: u32,
    shared: &std::collections::HashSet<String>,
    forced: bool,
) {
    if let Some(nodes) = scope.get_mut("nodes").and_then(Value::as_array_mut) {
        for node in nodes {
            let is_shared =
                forced || node["properties"]["saya_shared_base"].as_bool() == Some(true);
            if is_shared && node["properties"]["saya_phase"].as_u64().is_some() {
                node["properties"]["saya_phase"] = json!(phase);
            }
        }
    }
    if let Some(definitions) = scope
        .pointer_mut("/definitions/subgraphs")
        .and_then(Value::as_array_mut)
    {
        for definition in definitions {
            let force = definition["id"]
                .as_str()
                .is_some_and(|id| shared.contains(id));
            retag_shared(definition, phase, shared, forced || force);
        }
    }
}

/// Build an executable phase API without submitting it. All numeric seed and
/// noise_seed inputs receive the same exact u64. The source UI is only read.
pub fn prepare_phase_api(
    source: &str,
    info: &Value,
    phase: u32,
    master_seed: Option<&str>,
    isolated_root: Option<&str>,
) -> Result<Value> {
    let projected = crate::saya_preset::phase_source(&serde_json::from_str(source)?, phase)?;
    let projected = projected.map(|v| v.to_string());
    let source = projected.as_deref().unwrap_or(source);
    let source = prepare_phase_source(source, phase)?;
    let mut converted = crate::workflow_ui_api::convert_ui_phase(&source, Some(info), phase)?;
    if converted["report"]["complete"] != true {
        return Err(format!(
            "Conversion phase {phase} incomplète: {}",
            converted["report"]["unresolved"]
        )
        .into());
    }
    let mut targets = Vec::new();
    if let Some(seed_text) = master_seed.filter(|s| !s.trim().is_empty()) {
        let seed: u64 = seed_text.parse().map_err(|_| "Master seed u64 invalide")?;
        targets = apply_master_seed(&mut converted["api"], seed)?;
    }
    if let Some(root) = isolated_root {
        for (_, node) in converted["api"].as_object_mut().into_iter().flatten() {
            for (input, value) in node["inputs"].as_object_mut().into_iter().flatten() {
                if input == "checkpoint_root" && value.is_string() {
                    *value = json!(root);
                } else if input == "filename_prefix" {
                    if let Some(prefix) = value.as_str() {
                        *value = json!(format!("{root}/{prefix}"));
                    }
                }
            }
        }
        apply_isolated_phase_root(&mut converted["api"], info, root);
    }
    if let Some(api) = converted.get_mut("api") {
        prune_unreachable_nodes(api, info);
    }
    crate::workflow::validate_graph(&converted["api"], &schema_for_validation(info))?;
    Ok(
        json!({"phase":phase,"api":converted["api"],"report":converted["report"],
        "seedTargets":targets,"seed":master_seed}),
    )
}

/// Writes the master seed on every LITERAL `seed` / `noise_seed` input of an API graph and
/// returns the inputs written. A linked seed is never touched: it follows its wire (the
/// `Seed (rgthree)` node in pass 1, the phase `Load` node's manifest seed in passes 2-6), so
/// one master seed reaches every sampler without rewiring. A non-numeric value under that name
/// (a JSON text widget) is not a seed and stays as it is.
fn apply_master_seed(api: &mut Value, seed: u64) -> Result<Vec<Value>> {
    let mut targets = Vec::new();
    for (node, value) in api.as_object_mut().ok_or("API absente")?.iter_mut() {
        for (input, current) in value["inputs"].as_object_mut().into_iter().flatten() {
            if (input == "seed" || input == "noise_seed") && current.is_number() {
                *current = json!(seed);
                targets.push(json!({"node":node,"input":input}));
            }
        }
    }
    Ok(targets)
}

/// Folders to search for split passes: `SAYA_PASSES_DIR`, the ComfyUI copy placed by the
/// preset, then the installed preset.
pub fn generation_preset_source(source: &str, source_path: Option<&str>) -> Result<String> {
    let doc: Value = serde_json::from_str(source)?;
    if doc["extra"].get("saya_split").is_none() || doc["extra"].get("saya_preset").is_some() {
        return Ok(source.to_owned());
    }
    let mut dirs = split_dirs();
    if let Some(parent) = source_path.and_then(|p| Path::new(p).parent()) {
        dirs.insert(0, parent.to_owned());
    }
    let dir = dirs
        .iter()
        .find(|d| d.join("ILUSTCOUPLE_00_MAITRE_CONFIG.json").is_file())
        .ok_or("Maître du preset introuvable")?;
    Ok(crate::saya_preset::assemble(&doc, dir)?.to_string())
}

fn split_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(d) = std::env::var_os("SAYA_PASSES_DIR") {
        dirs.push(PathBuf::from(d));
    }
    if let Ok(h) = crate::paths::home_dir() {
        dirs.push(Path::new(&h).join("AI/ComfyUI/user/default/workflows/ILUSTCOUPLE_PASSES"));
    }
    dirs.push(crate::paths::presets_dir().join("passes"));
    dirs.push(crate::paths::share_dir().join("presets/passes"));
    dirs.dedup();
    dirs
}

/// Split source (`extra.saya_split` = "pass 1"): returns the workflow of pass `phase`,
/// tuned by the master then by the working copy (same node ids, same type).
/// `None` for a full workflow, which keeps the historical path.
pub fn split_phase_source(source: &str, phase: u32) -> Result<Option<String>> {
    split_phase_source_in(source, phase, &split_dirs())
}

fn split_phase_source_in(source: &str, phase: u32, dirs: &[PathBuf]) -> Result<Option<String>> {
    let doc: Value = serde_json::from_str(source)?;
    if !doc["extra"]["saya_split"]
        .as_str()
        .is_some_and(|s| s.starts_with("passe"))
        || phase < 2
    {
        return Ok(None);
    }
    let prefix = format!("ILUSTCOUPLE_{phase:02}_PASSE_{phase}_");
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let found = entries.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".json"))
        });
        let Some(path) = found else { continue };
        let mut pass: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        // Same overlay semantics as the preset path (`saya_preset::phase_source`):
        // widgets of BOTH fields then definitions, master then working copy. The
        // engine splits a master ALREADY tuned by the shot (comfy.py fill_workflow
        // writes `widgets_values_named` on hub 810 AND 488 of its definition), and
        // the conversion reads `widgets_values_named` first (`workflow_ui_api::
        // widget_values`) — an overlay limited to the `widgets_values` array therefore left
        // passes 2-6 on the state ON DISK (Generation checkpoint instead of the
        // shot checkpoint, pass 1 prompts and LoRA lost).
        if let Ok(bytes) = std::fs::read(dir.join("ILUSTCOUPLE_00_MAITRE_CONFIG.json")) {
            let master: Value = serde_json::from_slice(&bytes)?;
            crate::saya_preset::overlay_widgets(&mut pass, &master);
            crate::saya_preset::overlay_definitions(&mut pass, &master);
        }
        crate::saya_preset::overlay_widgets(&mut pass, &doc);
        crate::saya_preset::overlay_definitions(&mut pass, &doc);
        return Ok(Some(pass.to_string()));
    }
    Err(format!("Passe {phase} introuvable ({prefix}*.json) dans {dirs:?}").into())
}

/// The automatic sequence's per-phase `SayaImagePhaseNLoad` / `Stop` nodes are
/// hidden fixed variants that hardcode the node's own
/// `DEFAULT_CHECKPOINT_ROOT` (`image/checkpoints`), so a phase 1 written into an
/// isolated root is invisible to phase 2. Re-point them at the isolated root by
/// using the configurable classes they derive from: only the
/// phase/detailer/root widget values differ, the wiring and outputs are the same.
fn apply_isolated_phase_root(api: &mut Value, info: &Value, root: &str) {
    for (_, node) in api.as_object_mut().into_iter().flatten() {
        let Some(class) = node["class_type"].as_str().map(str::to_owned) else {
            continue;
        };
        let Some((phase, generic)) = fixed_phase_node(&class) else {
            continue;
        };
        let Some(choice) = phase_choice(info, generic, phase) else {
            continue;
        };
        node["class_type"] = json!(generic);
        if let Some(inputs) = node["inputs"].as_object_mut() {
            inputs.insert("phase".into(), json!(choice));
            inputs.insert("detailer".into(), json!("none"));
            inputs.insert("checkpoint_root".into(), json!(root));
        }
    }
}

fn fixed_phase_node(class: &str) -> Option<(u32, &'static str)> {
    let rest = class.strip_prefix("SayaImagePhase")?;
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let (number, suffix) = rest.split_at(digits);
    let phase: u32 = number.parse().ok()?;
    let generic = match suffix {
        "Load" => "SayaImagePhaseCheckpointLoad",
        "Stop" => "SayaImagePhaseCheckpointStop",
        _ => return None,
    };
    (2..=6).contains(&phase).then_some((phase, generic))
}

/// Exact `phase` combo option advertised by the node, so validation passes.
fn phase_choice(info: &Value, class: &str, phase: u32) -> Option<String> {
    let spec = &info[class]["input"]["required"]["phase"];
    let options = spec[0]
        .as_array()
        .or_else(|| spec[1]["options"].as_array())?;
    let prefix = phase.to_string();
    options
        .iter()
        .filter_map(Value::as_str)
        .find(|option| option.split_whitespace().next() == Some(prefix.as_str()))
        .map(str::to_owned)
}

use crate::{gallery::digest, Result as CoreResult};
use rusqlite::OptionalExtension;
use std::path::{Path, PathBuf};

fn runs_root(db: &Database) -> CoreResult<PathBuf> {
    let root = db.pass_state_root()?.join("saya-runs");
    std::fs::create_dir_all(&root)?;
    Ok(root)
}
#[cfg(test)]
pub(crate) fn saya_runs_root_for_tests(db: &Database) -> PathBuf {
    runs_root(db).unwrap()
}
fn run_file(db: &Database, run: &str) -> CoreResult<PathBuf> {
    if run.len() < 8 || !run.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Run Saya invalide".into());
    }
    Ok(runs_root(db)?.join(format!("{run}.json")))
}
/// `pub(crate)`: the casting stop/pause/resume/save commands (casting.rs)
/// read and persist the SAME manifest, without duplicating its paths.
/// Run manifests listed by `saya_runs`, without their `source`, cached per file and keyed by
/// modification time and size. A casting keeps hundreds of runs whose frozen workflow weighs
/// megabytes each: every history, background tick and batch operation used to read and parse all
/// of them again (hundreds of MB, seconds each time). Only changed files are read now.
type RunStamp = (std::time::SystemTime, u64);
fn run_cache() -> &'static std::sync::Mutex<std::collections::HashMap<PathBuf, (RunStamp, Value)>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<PathBuf, (RunStamp, Value)>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

pub(crate) fn read_run(db: &Database, run: &str) -> CoreResult<Value> {
    let bytes = match std::fs::read(run_file(db, run)?) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err("Run introuvable (déjà supprimé ?)".into())
        }
        other => other?,
    };
    Ok(serde_json::from_slice(&bytes)?)
}
/// SQLite busy/locked errors: another connection held the write lock past the timeout.
fn is_transient_db_error(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("database is locked") || message.contains("database is busy")
}
/// Job ids referenced by the phases of a run manifest.
pub(crate) fn phase_jobs(run: &Value) -> Vec<String> {
    run["phases"]
        .as_object()
        .map(|p| {
            p.values()
                .filter_map(|v| v["job"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
/// Representative image of a trial: the final one if pass 6 succeeded, otherwise pass 1.
fn casting_representative(db: &Database, run: &Value) -> Option<String> {
    let phase =
        if run["phases"]["6"]["state"] == "succeeded" || run["phases"]["6"]["asset"].is_string() {
            "6"
        } else {
            "1"
        };
    let info = &run["phases"][phase];
    if let Some(asset) = info["asset"].as_str() {
        return Some(asset.to_owned());
    }
    let outputs: String = db
        .connection
        .query_row(
            "SELECT outputs_json FROM generation_jobs WHERE id=? AND state='succeeded'",
            [info["job"].as_str()?],
            |r| r.get(0),
        )
        .ok()?;
    let outputs = serde_json::from_str::<Value>(&outputs).ok()?;
    crate::comfy::representative_asset(&outputs).map(str::to_owned)
}
pub(crate) fn write_run(db: &Database, run: &str, value: &Value) -> CoreResult<()> {
    crate::fsutil::write_atomic(&run_file(db, run)?, value.to_string().as_bytes())
}

/// Remove abandoned `*.tmp` files (crash between write and rename). Never touches `.json`.
fn remove_stale_run_temps(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "tmp") {
            continue;
        }
        let old = entry
            .metadata()
            .ok()
            .filter(|m| m.is_file())
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > std::time::Duration::from_secs(3600));
        if old {
            let _ = std::fs::remove_file(&path);
        }
    }
}

impl Database {
    /// Read existing manifests for UI recovery, including when the engine is offline.
    pub fn saya_runs(&self) -> CoreResult<Vec<Value>> {
        // Listing never creates the folder: readers such as the gallery call it too.
        let root = self.pass_state_root()?.join("saya-runs");
        if !root.is_dir() {
            return Ok(Vec::new());
        }
        remove_stale_run_temps(&root);
        let mut cache = run_cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut present = std::collections::HashSet::new();
        let mut runs = Vec::new();
        for entry in std::fs::read_dir(&root)? {
            let entry = entry?;
            let path = entry.path();
            if !entry.file_type()?.is_file() || path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let metadata = entry.metadata()?;
            let stamp = (metadata.modified()?, metadata.len());
            present.insert(path.clone());
            if let Some((cached_stamp, value)) = cache.get(&path) {
                if *cached_stamp == stamp {
                    runs.push(value.clone());
                    continue;
                }
            }
            let parsed = std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| {
                    serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string())
                })
                .and_then(|v| {
                    if v.is_object() {
                        Ok(v)
                    } else {
                        Err("Manifeste Saya invalide".to_owned())
                    }
                });
            let mut value = match parsed {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("Manifeste Saya ignoré {}: {error}", path.display());
                    cache.remove(&path);
                    continue;
                }
            };
            if let Some(object) = value.as_object_mut() {
                object.remove("source");
            }
            cache.insert(path, (stamp, value.clone()));
            runs.push(value);
        }
        // Deleted runs leave the cache (only this database's folder is pruned).
        cache.retain(|path, _| !path.starts_with(&root) || present.contains(path));
        drop(cache);
        runs.sort_by(|a, b| b["createdAt"].as_str().cmp(&a["createdAt"].as_str()));
        Ok(runs)
    }

    /// Casting -> gallery: marks the representative image (final if pass 6
    /// succeeded, otherwise pass 1) as kept. No file copy: validating
    /// the same image twice never creates a duplicate.
    /// Keeps trials in the gallery and queues their six-pass finish (owner rule: the finish runs
    /// after the whole casting, see `casting_start_queued_finish`).
    pub fn casting_publish(
        &mut self,
        runs: &[String],
        client: &Comfy,
        results: &Path,
    ) -> CoreResult<Value> {
        let published = self.casting_mark_published(runs)?;
        let mut queued = 0;
        for id in runs {
            if self.casting_keep_for_finish(id, client, results)? {
                queued += 1;
            }
        }
        Ok(json!({"published": published, "finishQueued": queued}))
    }

    fn casting_mark_published(&self, runs: &[String]) -> CoreResult<usize> {
        let mut published = 0;
        for id in runs {
            let mut run = read_run(self, id)?;
            if !run["casting"].is_object() {
                return Err("Ce résultat ne vient pas du casting".into());
            }
            let asset = casting_representative(self, &run).ok_or("Attendre une image terminée")?;
            self.asset_path(&asset)?;
            self.connection
                .execute("UPDATE assets SET review='accepted' WHERE id=?", [&asset])?;
            if run["casting"]["published"] != true {
                run["casting"]["published"] = json!(true);
                write_run(self, id, &run)?;
                published += 1;
            }
        }
        Ok(published)
    }

    /// Deletes a casting batch. Trials validated in the gallery or saved
    /// as a character only leave casting; the others lose their
    /// manifest and the Saya images that no other result uses. An asset
    /// row still cited by a job's provenance stays, removed from the catalog.
    pub fn casting_delete_batch(&self, id: &str) -> CoreResult<Value> {
        let source = read_run(self, id)?;
        let batch = source["casting"]["catalogue"].clone();
        if !batch.is_string() {
            return Err("Lot absent".into());
        }
        let runs = self.saya_runs()?;
        let in_batch = |r: &Value| r["casting"]["catalogue"] == batch;
        let busy = |r: &Value| {
            r["phases"].as_object().is_some_and(|p| {
                p.values().any(|v| {
                    matches!(
                        v["state"].as_str(),
                        Some(
                            "waiting"
                                | "running"
                                | "submitted"
                                | "submitting"
                                | "queued"
                                | "submission_unknown"
                        )
                    )
                })
            })
        };
        if runs
            .iter()
            .any(|r| in_batch(r) && r["state"] == "running" && busy(r))
        {
            return Err("Annuler le lot avant de le supprimer".into());
        }
        let chosen: std::collections::HashSet<String> = self
            .casting_choices()?
            .iter()
            .filter_map(|c| c["run"].as_str().map(str::to_owned))
            .collect();
        let kept = |r: &Value| {
            r["casting"]["published"] == true
                || r["casting"]["decision"] == "oui"
                || r["id"].as_str().is_some_and(|i| chosen.contains(i))
        };
        let phase_assets = |r: &Value| -> Vec<String> {
            r["phases"]
                .as_object()
                .map(|p| {
                    p.values()
                        .filter_map(|v| v["asset"].as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        // Assets still used outside the deleted trials.
        let mut used = std::collections::HashSet::new();
        for run in runs.iter().filter(|r| !in_batch(r) || kept(r)) {
            used.extend(phase_assets(run));
        }
        let data = self
            .pass_state_root()?
            .parent()
            .ok_or("Base sans dossier")?
            .canonicalize()?;
        let mut owned = vec![data.join("results"), data.join("casting")];
        if let Ok(Some(root)) = self.capture_root() {
            owned.push(root);
        }
        let (mut removed_runs, mut archived_runs, mut removed_files) = (0, 0, 0);
        for run in runs.iter().filter(|r| in_batch(r)) {
            let run_id = run["id"].as_str().ok_or("Run absent")?;
            if kept(run) {
                let mut value = read_run(self, run_id)?;
                value["casting"]["archived"] = json!(true);
                write_run(self, run_id, &value)?;
                archived_runs += 1;
                continue;
            }
            for asset in phase_assets(run) {
                if used.contains(&asset) {
                    continue;
                }
                removed_files += self.remove_unused_asset(&asset, &owned)?;
            }
            std::fs::remove_file(run_file(self, run_id)?)?;
            removed_runs += 1;
        }
        // The catalog folder disappears when no manifest cites it anymore.
        if let Some(catalogue) = batch.as_str().map(Path::new) {
            let still = self
                .saya_runs()?
                .iter()
                .any(|r| r["casting"]["catalogue"] == batch);
            if let (false, Some(folder)) = (still, catalogue.parent()) {
                if folder
                    .canonicalize()
                    .is_ok_and(|f| f.starts_with(data.join("casting")) && f != data.join("casting"))
                {
                    std::fs::remove_dir_all(folder)?;
                }
            }
        }
        Ok(
            json!({"removedRuns": removed_runs, "archivedRuns": archived_runs, "removedFiles": removed_files}),
        )
    }

    /// Removes an asset of a replaced or deleted trial. Only files inside Studio's own
    /// folders (`owned`) are touched: a delivery deduplicated onto an image of the
    /// user's library shares its asset, and that image must stay listed and on disk.
    /// Spared: favourites, accepted images and anything a character, a reference or the
    /// A/B lab uses. Every erased file is appended to the removal journal.
    pub(crate) fn remove_unused_asset(&self, asset: &str, owned: &[PathBuf]) -> CoreResult<usize> {
        let Some(path): Option<String> = self
            .connection
            .query_row(
                "SELECT absolute_path FROM assets WHERE id=?",
                [asset],
                |r| r.get(0),
            )
            .optional()?
        else {
            return Ok(0);
        };
        let file = PathBuf::from(&path);
        let in_owned_root = match file.canonicalize() {
            Ok(real) => owned
                .iter()
                .any(|root| root.canonicalize().is_ok_and(|root| real.starts_with(root))),
            // Already gone: its catalog rows are cleaned only if it lived in Studio's folders.
            Err(_) => owned.iter().any(|root| {
                file.starts_with(root)
                    || root.canonicalize().is_ok_and(|root| file.starts_with(root))
            }),
        };
        if !in_owned_root {
            return Ok(0);
        }
        let protected: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM assets a WHERE a.absolute_path=?1 AND (a.review='accepted' OR a.favorite=1
               OR EXISTS(SELECT 1 FROM asset_characters c WHERE c.asset_id=a.id)
               OR EXISTS(SELECT 1 FROM character_references c WHERE c.asset_id=a.id)
               OR EXISTS(SELECT 1 FROM ab_cell_images c WHERE c.asset_id=a.id)))",
            [&path], |r| r.get(0))?;
        if protected {
            return Ok(0);
        }
        let ids: Vec<String> = {
            let mut q = self
                .connection
                .prepare("SELECT id FROM assets WHERE absolute_path=?")?;
            let v = q
                .query_map([&path], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?;
            v
        };
        for id in &ids {
            let cited: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM job_outputs WHERE asset_id=?1) OR EXISTS(SELECT 1 FROM asset_models WHERE asset_id=?1) OR EXISTS(SELECT 1 FROM pass_artifacts WHERE asset_id=?1)",
                [id], |r| r.get(0))?;
            if cited {
                self.connection.execute(
                    "UPDATE assets SET catalog_removed=1,availability='missing' WHERE id=?",
                    [id],
                )?;
            } else {
                self.connection
                    .execute("DELETE FROM assets WHERE id=?", [id])?;
            }
        }
        // Re-checked through the canonical path right before erasing (no symlink escape).
        let inside = owned
            .iter()
            .any(|root| crate::fsutil::ensure_within(root, &file).is_ok());
        if inside && file.is_file() {
            std::fs::remove_file(&file)?;
            self.journal_removed_file(&file);
            return Ok(1);
        }
        Ok(0)
    }

    /// Appends one erased image to `<pass states>/preview-cleanup/removed-assets.log`
    /// (`<unix seconds>\t<path>`). A journal failure is logged, never fatal: the file is
    /// already gone and the caller must still finish its catalog update.
    fn journal_removed_file(&self, file: &Path) {
        use std::io::Write;
        let outcome = self.pass_state_root().and_then(|root| {
            let directory = root.join("preview-cleanup");
            std::fs::create_dir_all(&directory)?;
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let mut journal = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(directory.join("removed-assets.log"))?;
            writeln!(journal, "{stamp}\t{}", file.display())?;
            Ok(())
        });
        if let Err(error) = outcome {
            eprintln!("saya: removal journal failed for {}: {error}", file.display());
        }
    }

    pub fn casting_history(&self) -> CoreResult<Vec<Value>> {
        let choices = self.casting_choices()?;
        let mut query = self
            .connection
            .prepare("SELECT id,state,outputs_json FROM generation_jobs")?;
        let jobs = query
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    (r.get::<_, String>(1)?, r.get::<_, String>(2)?),
                ))
            })?
            .collect::<std::result::Result<std::collections::HashMap<_, _>, _>>()?;
        let mut rows = Vec::new();
        for mut run in self.saya_runs()? {
            if run["casting"]["archived"] == true {
                continue;
            }
            if let Some(phases) = run["phases"].as_object_mut() {
                for phase in phases.values_mut() {
                    if let Some((state, outputs)) =
                        phase["job"].as_str().and_then(|id| jobs.get(id))
                    {
                        if state != "draft" {
                            phase["state"] = json!(state);
                        }
                        let outputs: Value = serde_json::from_str(outputs)?;
                        if let Some(asset) = crate::comfy::representative_asset(&outputs) {
                            phase["asset"] = json!(asset);
                        }
                    }
                }
            }
            let kept = choices.iter().find(|c| c["run"] == run["id"]);
            let metadata = &run["casting"];
            if !metadata.is_object() && kept.is_none() {
                continue;
            }
            let shot = metadata
                .get("shot")
                .cloned()
                .unwrap_or_else(|| kept.cloned().unwrap_or(Value::Null));
            let decision = if kept.is_some() {
                "oui"
            } else {
                metadata["decision"].as_str().unwrap_or("")
            };
            let phase = &run["phases"]["1"];
            let final_asset = (2..=6).rev().find_map(|p| {
                run["phases"][p.to_string()]["asset"]
                    .as_str()
                    .map(str::to_owned)
            });
            let sheet_name = match run["id"].as_str() {
                Some(id) => self.casting_sheet_name(id)?,
                None => None,
            };
            let name = sheet_name.map_or_else(|| shot["name"].clone(), Value::String);
            rows.push(json!({"run":run["id"],"stem":shot["stem"],"name":name,"famille":shot["famille"],"model":shot["model"],
                "seed":run["masterSeed"],"graine":metadata.get("graine").or_else(|| kept.and_then(|k|k.get("graine"))),
                "catalogue":metadata.get("catalogue").or_else(|| kept.and_then(|k|k.get("catalogue"))),
                "state":phase["state"],"asset":phase["asset"],"decision":decision,"shot":shot,
                "createdAt":run["createdAt"],"runState":run["state"],"stopPending":run["stopPending"],"phases":run["phases"],"finalAsset":final_asset,"error":run["autoError"],"published":metadata["published"],"finishQueued":metadata["finishQueued"],
                "step":run["casting"]["step"],"released":run["casting"]["released"]}));
        }
        Ok(rows)
    }

    /// Called under saya_operations, including when no generation page is mounted.
    pub fn casting_tick(&mut self, client: &Comfy, results: &Path) -> CoreResult<()> {
        let runs = self.saya_runs()?;
        let mut synchronized = false;
        for run in &runs {
            if !run["casting"].is_object() {
                continue;
            }
            let active = run["phases"].as_object().is_some_and(|phases| {
                phases.values().any(|p| {
                    matches!(
                        p["state"].as_str(),
                        Some(
                            "submitted"
                                | "submitting"
                                | "queued"
                                | "running"
                                | "submission_unknown"
                        )
                    )
                })
            });
            if active {
                if !synchronized {
                    self.sync_jobs(client, results)?;
                    synchronized = true;
                }
                self.saya_status_synced(run["id"].as_str().ok_or("Run absent")?, client, results)?;
            }
        }
        let mut runs = self.saya_runs()?;
        runs.sort_by_key(|r| r["casting"]["index"].as_u64().unwrap_or(0));
        let mut started = std::collections::HashSet::new();
        for run in &runs {
            if run["state"] != "running" || run["phases"]["1"]["state"] != "waiting" {
                continue;
            }
            let Some(batch) = run["casting"]["catalogue"].as_str() else {
                continue;
            };
            // Step-by-step batches only submit the trials the user released.
            if run["casting"]["step"] == true && run["casting"]["released"] != true {
                continue;
            }
            if started.contains(batch) {
                continue;
            }
            if !casting_batch_ready(&runs, batch) {
                continue;
            }
            let id = run["id"].as_str().ok_or("Run absent")?;
            let job = run["phases"]["1"]["job"].as_str().ok_or("Job absent")?;
            let hash = run["phases"]["1"]["snapshotHash"]
                .as_str()
                .ok_or("Snapshot absent")?;
            // `saya_runs` removes `source` (UI response only): rewriting this clone
            // persisted a manifest without source, and the next casting decision
            // (`saya_continue`) failed with "Source Saya absente". The manifest
            // written ALWAYS starts again from a full re-read of the disk.
            let mut value = read_run(self, id)?;
            value["phases"]["1"]["state"] = json!("submitting");
            write_run(self, id, &value)?;
            // Never retry a POST automatically after an uncertain acknowledgement.
            if let Err(error) = self.submit_job(job, hash, true, true, client) {
                value["state"] = json!("paused");
                value["autoError"] = json!(error.to_string());
                write_run(self, id, &value)?;
                return Err(error);
            }
            value["phases"]["1"]["state"] = json!("submitted");
            write_run(self, id, &value)?;
            started.insert(batch.to_owned());
        }
        self.casting_start_queued_finish(client, results)?;
        Ok(())
    }

    pub fn casting_batch_prepared(&self, ids: &[String]) -> CoreResult<()> {
        for id in ids {
            let mut run = read_run(self, id)?;
            run["casting"]["prepared"] = json!(true);
            write_run(self, id, &run)?;
        }
        Ok(())
    }

    /// Step-by-step mode ("une par une"): the batch is drawn as usual but only its first
    /// trial is released; `casting_next` releases the following ones, one per call.
    pub fn casting_batch_step(&self, ids: &[String]) -> CoreResult<()> {
        for (position, id) in ids.iter().enumerate() {
            let mut run = read_run(self, id)?;
            run["casting"]["step"] = json!(true);
            run["casting"]["released"] = json!(position == 0);
            write_run(self, id, &run)?;
        }
        Ok(())
    }

    /// Releases the next waiting trial of a step-by-step batch (lowest index first).
    /// Returns its run id; an exhausted batch is an error the UI answers with a new batch.
    pub fn casting_next(&self, run: &str) -> CoreResult<String> {
        let current = read_run(self, run)?;
        let batch = current["casting"]["catalogue"]
            .as_str()
            .ok_or("Run hors casting")?
            .to_owned();
        let mut waiting: Vec<Value> = self
            .saya_runs()?
            .into_iter()
            .filter(|r| {
                r["casting"]["catalogue"].as_str() == Some(batch.as_str())
                    && r["casting"]["step"] == true
                    && r["casting"]["released"] != true
                    && r["state"] == "running"
                    && r["phases"]["1"]["state"] == "waiting"
            })
            .collect();
        waiting.sort_by_key(|r| r["casting"]["index"].as_u64().unwrap_or(u64::MAX));
        let next = waiting
            .first()
            .and_then(|r| r["id"].as_str())
            .ok_or("Lot pas-à-pas épuisé")?
            .to_owned();
        let mut value = read_run(self, &next)?;
        value["casting"]["released"] = json!(true);
        write_run(self, &next, &value)?;
        Ok(next)
    }

    pub fn casting_batch_runs(&self, id: &str) -> CoreResult<Vec<String>> {
        let current = read_run(self, id)?;
        if !current["casting"].is_object() {
            return Err("Run hors casting".into());
        }
        let batch = current["casting"]["catalogue"].as_str();
        Ok(self
            .saya_runs()?
            .iter()
            .filter(|r| {
                r["id"] == id || (batch.is_some() && r["casting"]["catalogue"].as_str() == batch)
            })
            .filter(|r| {
                r["state"] == "running"
                    || r["state"] == "paused"
                    || (r["state"] == "stopped"
                        && r["phases"].as_object().is_some_and(|phases| {
                            phases.values().any(|p| {
                                matches!(
                                    p["state"].as_str(),
                                    Some(
                                        "submitted"
                                            | "submitting"
                                            | "queued"
                                            | "running"
                                            | "submission_unknown"
                                    )
                                )
                            })
                        }))
            })
            .filter_map(|r| r["id"].as_str().map(str::to_owned))
            .collect())
    }

    pub fn casting_decide(
        &mut self,
        run: &str,
        keep: bool,
        client: &Comfy,
        results: &Path,
    ) -> CoreResult<Value> {
        let mut value = read_run(self, run)?;
        if value["phases"]["1"]["state"] != "succeeded" {
            return Err("Attendre la fin de la passe 1".into());
        }
        // Run paused (casting.rs casting_pause): deciding would launch pass 2 —
        // the pause forbids any NEW queueing, so we enforce it.
        if value["state"] == "paused" {
            return Err("Run casting en pause : le reprendre avant de décider".into());
        }
        // Duo trials stop at pass 1: passes 2-6 have never been validated with two people here.
        if keep && value["casting"]["shot"]["duo"] == json!(true) {
            return Err("Un duo s'arrête à la passe 1 : pas de 6 passes".into());
        }
        let metadata = value["casting"].clone();
        if !metadata.is_object() {
            return Err("Ancien casting sans catalogue figé : choix conservé, relancer un nouvel essai pour le finaliser".into());
        }
        let prior = metadata["decision"].as_str().unwrap_or("");
        if !prior.is_empty() && (prior == "oui") != keep {
            return Err("Le choix de cet essai est déjà enregistré".into());
        }
        if keep {
            self.casting_validate(
                run,
                metadata["graine"]
                    .as_str()
                    .ok_or("Graine casting absente")?,
                metadata["catalogue"]
                    .as_str()
                    .ok_or("Catalogue casting absent")?,
                &metadata["shot"],
            )?;
        }
        value["casting"]["decision"] = json!(if keep { "oui" } else { "non" });
        if !keep {
            value["state"] = json!("stopped");
            value["autoAdvance"] = json!(false);
        }
        // Owner rule (2026-11-04): a kept trial's six passes never run between pass-1 trials (an
        // SDXL pass 1 and a HiDream pass 3 sharing the GPU ran out of memory). The finish is
        // queued; `casting_start_queued_finish` starts it once every pass-1 trial is done.
        if keep && only_pass_one(&value) && value["casting"]["finishQueued"] != true {
            value["casting"]["finishQueued"] = json!(true);
            value["casting"]["finishQueuedAt"] = json!(chrono_now());
            value["state"] = json!("running");
        }
        write_run(self, run, &value)?;
        if keep {
            if let Err(error) = self.casting_start_queued_finish(client, results) {
                // The finish stays queued: the background runner retries.
                eprintln!("saya: casting_decide: queued finish not started yet: {error}");
            }
            return read_run(self, run);
        }
        Ok(value)
    }

    /// Keeping a trial (gallery or character sheet) also wants its six passes: queue the finish
    /// of an undecided trial whose pass 1 succeeded. A rejected trial is left as it is.
    pub fn casting_keep_for_finish(
        &mut self,
        run: &str,
        client: &Comfy,
        results: &Path,
    ) -> CoreResult<bool> {
        let value = read_run(self, run)?;
        // Already decided, past pass 1 (finished or finishing) or paused: kept as it is.
        if !value["casting"].is_object()
            || value["phases"]["1"]["state"] != "succeeded"
            || value["state"] == "paused"
            || !only_pass_one(&value)
            || !value["casting"]["decision"].as_str().unwrap_or_default().is_empty()
        {
            return Ok(false);
        }
        self.casting_decide(run, true, client, results)?;
        Ok(true)
    }

    /// Starts the oldest queued casting finish when no pass-1 trial is waiting or in flight and
    /// no other finish is running: one six-pass chain at a time, after the whole casting. Trials
    /// held by step-by-step mode until the user releases them do not block. A finish stopped on
    /// a failed pass does not block the others (it waits for « Reprendre »).
    pub fn casting_start_queued_finish(
        &mut self,
        client: &Comfy,
        results: &Path,
    ) -> CoreResult<Option<Value>> {
        let runs = self.saya_runs()?;
        let casting: Vec<&Value> = runs.iter().filter(|r| r["casting"].is_object()).collect();
        if casting.iter().any(|r| pass_one_pending(r) || finish_running(r)) {
            return Ok(None);
        }
        let mut queued: Vec<&&Value> = casting
            .iter()
            .filter(|r| {
                r["casting"]["finishQueued"] == true
                    && r["state"] == "running"
                    && r["phases"]["1"]["state"] == "succeeded"
                    && only_pass_one(r)
            })
            .collect();
        queued.sort_by_key(|r| {
            r["casting"]["finishQueuedAt"]
                .as_str()
                .or(r["createdAt"].as_str())
                .unwrap_or_default()
                .to_owned()
        });
        let Some(id) = queued.first().and_then(|r| r["id"].as_str()).map(str::to_owned) else {
            return Ok(None);
        };
        let started = self.saya_continue(&id, client, results)?;
        let mut value = read_run(self, &id)?;
        value["casting"]["finishQueued"] = json!(false);
        write_run(self, &id, &value)?;
        Ok(Some(started))
    }

    /// Create a persistent Saya run and submit phase 1. The source is read-only.
    pub fn saya_start(
        &mut self,
        source_path: &str,
        master_seed: Option<&str>,
        isolated_root: &str,
        client: &Comfy,
        results: &Path,
    ) -> CoreResult<Value> {
        if !Path::new(source_path).is_absolute() {
            return Err("Chemin de workflow Saya absolu requis".into());
        }
        let source = std::fs::read_to_string(source_path)?;
        self.saya_start_source(
            &source,
            source_path,
            master_seed,
            isolated_root,
            client,
            results,
        )
    }

    pub fn saya_start_source(
        &mut self,
        source: &str,
        source_path: &str,
        master_seed: Option<&str>,
        isolated_root: &str,
        client: &Comfy,
        results: &Path,
    ) -> CoreResult<Value> {
        if source.len() > 32 * 1024 * 1024 {
            return Err("Workflow Saya trop volumineux".into());
        }
        let info = client.get("/object_info")?;
        self.saya_start_source_with_info(
            source,
            source_path,
            master_seed,
            isolated_root,
            client,
            results,
            &info,
        )
    }

    /// A batch shares a single server catalogue; each job still gets its own validated snapshot.
    pub fn saya_start_source_with_info(
        &mut self,
        source: &str,
        source_path: &str,
        master_seed: Option<&str>,
        isolated_root: &str,
        client: &Comfy,
        results: &Path,
        info: &Value,
    ) -> CoreResult<Value> {
        if source.len() > 32 * 1024 * 1024 {
            return Err("Workflow Saya trop volumineux".into());
        }
        let run = uuid::Uuid::new_v4().to_string();
        let api = prepare_phase_api(&source, &info, 1, master_seed, Some(isolated_root))?;
        let job = if source_path.starts_with("temporary-") {
            self.prepare_temporary_image(
                &api["api"],
                &schema_for_validation(&info),
                Some(&serde_json::from_str(source)?),
            )?
        } else {
            let wf = self.import_internal_workflow("Saya Couple · phase 1", &api["api"].to_string())?;
            self.prepare_job(&wf, &[], &schema_for_validation(&info))?
        };
        let casting = serde_json::from_str::<Value>(source)?["extra"]["saya_casting"].clone();
        let mut value = json!({"id":run,"sourcePath":source_path,"sourceSha256":digest(source.as_bytes()),
            "source":source,"masterSeed":master_seed,"isolatedRoot":isolated_root,"state":"running",
            "phases":{"1":{"job":job.job_id,"state":"submitting"}},"createdAt":chrono_now(),"casting":casting});
        // Casting batches keep prepared jobs local until their turn. No GPU queue flooding.
        if value["casting"]["deferred"] == true {
            value["phases"]["1"]["state"] = json!("waiting");
            value["phases"]["1"]["snapshotHash"] = json!(job.snapshot_hash);
            write_run(self, &run, &value)?;
            return Ok(value);
        }
        // Persist ownership before POST so a lost response can be recovered.
        write_run(self, &run, &value)?;
        self.submit_job(&job.job_id, &job.snapshot_hash, true, true, client)?;
        value["phases"]["1"]["state"] = json!("submitted");
        write_run(self, &run, &value)?;
        // The run is already persisted: a sync failure is reported, not fatal.
        if let Err(error) = self.sync_jobs(client, results) {
            eprintln!("saya: saya_start_source_with_info: sync_jobs failed for run {run}: {error}");
        }
        Ok(value)
    }

    pub fn saya_status(&mut self, run: &str, client: &Comfy, results: &Path) -> CoreResult<Value> {
        self.sync_jobs(client, results)?;
        self.saya_status_synced(run, client, results)
    }

    pub(crate) fn saya_status_synced(
        &mut self,
        run: &str,
        client: &Comfy,
        results: &Path,
    ) -> CoreResult<Value> {
        let mut value = read_run(self, run)?;
        let jobs = self.jobs_by_ids(&phase_jobs(&value))?;
        let mut all_done = true;
        for phase in 1..=SAYA_PHASES {
            let key = phase.to_string();
            let Some(job_id) = value["phases"][&key]["job"].as_str().map(str::to_owned) else {
                all_done = false;
                continue;
            };
            let Some(job) = jobs.iter().find(|j| j["id"] == job_id) else {
                all_done = false;
                continue;
            };
            if !(matches!(
                value["phases"][&key]["state"].as_str(),
                Some("waiting" | "cancelled")
            ) && job["state"] == "draft")
            {
                value["phases"][&key]["state"] = job["state"].clone();
            }
            if let Some(asset) = crate::comfy::representative_asset(&job["outputs"]) {
                value["phases"][&key]["asset"] = json!(asset);
                if let Ok(sha) = self.connection.query_row(
                    "SELECT content_sha256 FROM assets WHERE id=?",
                    [asset],
                    |r| r.get::<_, String>(0),
                ) {
                    value["phases"][&key]["sha256"] = json!(sha);
                }
            }
            if let Some(review) = job["outputs"].as_array().and_then(|o| {
                o.iter()
                    .find(|x| x["sayaReview"].is_array())
                    .map(|x| x["sayaReview"].clone())
            }) {
                value["phases"][&key]["review"] = review;
            }
            match job["state"].as_str() {
                Some("succeeded") => {}
                Some(state @ ("failed" | "cancelled" | "interrupted")) => {
                    all_done = false;
                    value["autoAdvance"] = json!(false);
                    // The cause reaches the interface: a failed pass used to show "Erreur"
                    // only, its message (an out-of-memory, a missing model) staying in the
                    // database.
                    if state == "failed" {
                        if let Some(reason) = crate::comfy::failure_summary(&job["error"]) {
                            if value["autoError"].is_null() {
                                value["autoError"] = json!(format!("Passe {phase} : {reason}"));
                            }
                            value["phases"][&key]["error"] = json!(reason);
                        }
                    }
                }
                _ => all_done = false,
            }
        }
        if all_done {
            value["state"] = json!("completed");
        } else if value["state"] == "running" {
            value["state"] = json!("running");
        }
        write_run(self, run, &value)?;
        // Lineage rules (crate::lineage): a validated phase 1 or 6 replaces older images of
        // this run. Never fatal for the status poll.
        if let Err(error) = self.compact_lineage(run) {
            eprintln!("saya: compact_lineage failed for run {run}: {error}");
        }
        if value["autoAdvance"] == true && next_ready_phase(&value).is_some_and(|p| p > 2) {
            match self.saya_continue(run, client, results) {
                Ok(advanced) => return Ok(advanced),
                Err(error) => {
                    // Keep any durable job ownership written before the failed POST.
                    value = read_run(self, run)?;
                    let message = error.to_string();
                    // A busy/locked database is transient (another request was writing):
                    // keep chaining, the next status poll retries. Anything else stops the
                    // chain and is shown to the user.
                    if !is_transient_db_error(&message) {
                        value["autoAdvance"] = json!(false);
                        value["autoError"] = json!(message);
                        write_run(self, run, &value)?;
                    }
                }
            }
        }
        Ok(value)
    }

    pub fn saya_continue(
        &mut self,
        run: &str,
        client: &Comfy,
        results: &Path,
    ) -> CoreResult<Value> {
        let mut value = read_run(self, run)?;
        let phase = next_ready_phase(&value)
            .ok_or("Aucune passe prête : attendre la fin, ou redémarrer après une erreur")?;
        // Validate phase 1 exactly once.
        let phase1 = value["phases"]["1"].clone();
        if phase1["state"] == "succeeded"
            && phase1["review"].is_array()
            && value["reviewValidated"] != true
        {
            let job = phase1["job"].as_str().ok_or("Phase 1 sans job")?;
            self.saya_review_decision(job, "validate", client, true)?;
            value["reviewValidated"] = json!(true);
            write_run(self, run, &value)?;
        }
        self.submit_phase(run, value, phase, client, results)
    }

    /// Prepares phase `phase` of a run from its frozen source and master seed, records the
    /// job in the manifest BEFORE the POST (a lost response stays recoverable), then submits.
    /// Phase 1 is built from the whole source, like `saya_start`; later phases from their
    /// pass file when the preset is split.
    fn submit_phase(
        &mut self,
        run: &str,
        mut value: Value,
        phase: u32,
        client: &Comfy,
        results: &Path,
    ) -> CoreResult<Value> {
        let info = client.get("/object_info")?;
        let source = value["source"].as_str().ok_or("Source Saya absente")?;
        let seed = value["masterSeed"].as_str();
        let root = value["isolatedRoot"]
            .as_str()
            .ok_or("Racine Saya absente")?;
        let split = if phase > 1 {
            split_phase_source(source, phase)?
        } else {
            None
        };
        let api = prepare_phase_api(
            split.as_deref().unwrap_or(source),
            &info,
            phase,
            seed,
            Some(root),
        )?;
        let job = if value["sourcePath"]
            .as_str()
            .is_some_and(|path| path.starts_with("temporary-"))
        {
            self.prepare_temporary_image(
                &api["api"],
                &schema_for_validation(&info),
                Some(&serde_json::from_str(source)?),
            )?
        } else {
            let wf = self.import_internal_workflow(
                &format!("Saya Couple · phase {phase}"),
                &api["api"].to_string(),
            )?;
            self.prepare_job(&wf, &[], &schema_for_validation(&info))?
        };
        value["phases"][phase.to_string()] = json!({"job":job.job_id,"state":"submitting"});
        value["autoAdvance"] = json!(true);
        value["autoError"] = Value::Null;
        write_run(self, run, &value)?;
        self.submit_job(&job.job_id, &job.snapshot_hash, true, true, client)?;
        value["phases"][phase.to_string()]["state"] = json!("submitted");
        write_run(self, run, &value)?;
        // The run is already persisted: a sync failure is reported, not fatal.
        if let Err(error) = self.sync_jobs(client, results) {
            eprintln!("saya: submit_phase: sync_jobs failed for run {run}: {error}");
        }
        Ok(value)
    }

    /// Crash recovery: resubmits the first unfinished phase of a run with the SAME master
    /// seed after ComfyUI crashed or restarted, a pass failed or the user stopped. Finished
    /// phases are kept: the resubmitted phase reads the previous phase's state, which
    /// ComfyUI saved on disk under the run's isolated root.
    ///
    /// A phase still queued or running is never duplicated. A prompt left
    /// `submission_unknown` by a ComfyUI restart is first confirmed gone (targeted queue
    /// delete, then absence from queue and history) before it is replaced. A casting
    /// trial's phase 1 belongs to the casting queue (`casting_resume_batch`).
    pub fn saya_resume(&mut self, run: &str, client: &Comfy, results: &Path) -> CoreResult<Value> {
        // Unlike a status poll, the engine must answer: deciding "lost" needs fresh evidence.
        self.sync_jobs(client, results)?;
        let mut value = read_run(self, run)?;
        let jobs = self.jobs_by_ids(&phase_jobs(&value))?;
        let job_state = |job: &str| {
            jobs.iter()
                .find(|j| j["id"] == job)
                .and_then(|j| j["state"].as_str().map(str::to_owned))
        };
        let mut target = None;
        for phase in 1..=SAYA_PHASES {
            let key = phase.to_string();
            let Some(job) = value["phases"][&key]["job"].as_str().map(str::to_owned) else {
                // No attempt yet: a waiting later phase is what `saya_continue` submits.
                target = (phase > 1).then_some((phase, None));
                break;
            };
            let state = job_state(&job).unwrap_or_else(|| {
                value["phases"][&key]["state"].as_str().unwrap_or_default().to_owned()
            });
            match state.as_str() {
                "succeeded" => continue,
                "queued" | "running" | "submitting" => {
                    return Err("Une passe est encore en cours : rien à reprendre".into())
                }
                "submission_unknown" => {
                    self.abandon_lost_job(&job, client)?;
                    target = Some((phase, Some(job)));
                }
                "failed" | "interrupted" | "cancelled" | "draft" => {
                    target = Some((phase, Some(job)))
                }
                other => return Err(format!("Passe {phase} dans un état non repris : {other}").into()),
            }
            break;
        }
        let (phase, previous) = target.ok_or("Toutes les passes sont terminées : rien à reprendre")?;
        if phase == 1 && value["casting"].is_object() {
            return Err("Essai de casting : utiliser « Reprendre » sur le lot".into());
        }
        if let Some(job) = previous {
            let mut retired = value["retiredJobs"].as_array().cloned().unwrap_or_default();
            if !retired.iter().any(|v| v.as_str() == Some(job.as_str())) {
                retired.push(json!(job));
            }
            value["retiredJobs"] = json!(retired);
            let mut resumed = value["resumedPhases"].as_array().cloned().unwrap_or_default();
            resumed.push(json!({"phase":phase,"previousJob":job,"at":chrono_now()}));
            value["resumedPhases"] = json!(resumed);
        }
        if phase == 1 {
            value["reviewValidated"] = json!(false);
        }
        value["state"] = json!("running");
        value["stopPending"] = json!(false);
        self.submit_phase(run, value, phase, client, results)
    }

    /// Confirms that a `submission_unknown` job is not executing anymore and records it as
    /// cancelled. With a prompt id the targeted queue delete proves its absence; without
    /// one, the successful sync that just ran already searched the queue and the recent
    /// history by client id and found nothing.
    fn abandon_lost_job(&self, job: &str, client: &Comfy) -> CoreResult<()> {
        let prompt: Option<String> = self.connection.query_row(
            "SELECT comfy_prompt_id FROM generation_jobs WHERE id=?",
            [job],
            |r| r.get(0),
        )?;
        // Without a prompt id, fresh evidence decides; found after all, it is cancelled below.
        if prompt.is_none() && !self.settle_lost_submission(job, client)? {
            return Ok(());
        }
        let outcome = self.control_job(job, "cancel_queue", true, client)?;
        if outcome["confirmed"] != true {
            return Err(format!(
                "Passe perdue non confirmée : {}",
                outcome["message"].as_str().unwrap_or("état moteur incertain")
            )
            .into());
        }
        Ok(())
    }

    /// Settles a `submission_unknown` job that has no prompt id (the answer to its POST was
    /// lost) from a fresh look at the engine's queue and recent history. Absent: the job is
    /// recorded cancelled and `false` is returned. Found once: its prompt id is attached and
    /// `true` is returned, so the usual targeted control applies. Several matches stay
    /// uncertain and are an error.
    fn settle_lost_submission(&self, job: &str, client: &Comfy) -> CoreResult<bool> {
        let queue = client.get("/queue")?;
        crate::comfy::queue_contains(&queue, "queue_running", "")?;
        crate::comfy::queue_contains(&queue, "queue_pending", "")?;
        let history = client.get("/history?max_items=200")?;
        match self.engine_candidates(job, &queue, &history)?.len() {
            0 => {
                self.connection.execute(
                    "UPDATE generation_jobs SET state='cancelled' WHERE id=? AND state='submission_unknown' AND comfy_prompt_id IS NULL",
                    [job],
                )?;
                self.job_event(
                    job,
                    "lost_submission_abandoned",
                    &json!({"evidence":"absent_from_queue_and_history"}),
                )?;
                Ok(false)
            }
            1 => Ok(self.recover_prompt(job, &queue, &history)?.is_some()),
            _ => Err("Plusieurs prompts du moteur correspondent à ce job : état non confirmé".into()),
        }
    }

    /// Sends the matching control for every phase job ComfyUI may still execute: a queued
    /// or uncertain prompt is deleted from the queue, the running one is interrupted
    /// (`/interrupt` only reaches the executing prompt). Job states are read from the
    /// database, so callers synchronize first. A refused control is reported with
    /// `sent: false`, never fatal: the caller records the pending stop.
    pub(crate) fn stop_phase_jobs(&self, run: &Value, client: &Comfy) -> CoreResult<Vec<Value>> {
        let jobs = self.jobs_by_ids(&phase_jobs(run))?;
        let mut controls = Vec::new();
        for phase in 1..=SAYA_PHASES {
            let Some(job) = run["phases"][phase.to_string()]["job"].as_str() else {
                continue;
            };
            let Some(record) = jobs.iter().find(|j| j["id"] == job) else {
                continue;
            };
            // A lost submission without prompt id cannot be targeted: `control_job` refused
            // it, the stop stayed pending for ever and the batch could not be resumed.
            if record["state"] == "submission_unknown" && record["promptId"].is_null() {
                match self.settle_lost_submission(job, client) {
                    Ok(true) => {}
                    Ok(false) => {
                        controls.push(json!({"job":job,"phase":phase,"action":"abandon_lost","sent":true}));
                        continue;
                    }
                    Err(error) => {
                        controls.push(json!({"job":job,"phase":phase,"action":"abandon_lost","sent":false,"error":error.to_string()}));
                        continue;
                    }
                }
            }
            let action = match record["state"].as_str() {
                Some("queued" | "submission_unknown") => "cancel_queue",
                Some("running") => "interrupt",
                _ => continue,
            };
            controls.push(match self.control_job(job, action, true, client) {
                Ok(response) => json!({"job":job,"phase":phase,"action":action,"sent":true,"response":response}),
                Err(error) => json!({"job":job,"phase":phase,"action":action,"sent":false,"error":error.to_string()}),
            });
        }
        Ok(controls)
    }

    /// True while a phase job may still be executed by ComfyUI (database state).
    fn has_engine_job(&self, run: &Value) -> CoreResult<bool> {
        Ok(self.jobs_by_ids(&phase_jobs(run))?.iter().any(|job| {
            matches!(
                job["state"].as_str(),
                Some("submitting" | "queued" | "running" | "submission_unknown")
            )
        }))
    }

    pub fn saya_restart(&mut self, run: &str, client: &Comfy, results: &Path) -> CoreResult<Value> {
        if let Err(error) = self.sync_jobs(client, results) {
            eprintln!("saya: saya_restart: sync_jobs failed for run {run}: {error}");
        }
        let mut value = read_run(self, run)?;
        // A restart submits a new phase 1: an attempt still owned by ComfyUI would keep
        // running beside it (two GPU jobs, orphaned outputs). Stop it first.
        if self.has_engine_job(&value)? {
            return Err("Une passe est encore en cours : l’arrêter avant de relancer".into());
        }
        let phase1 = value["phases"]["1"].clone();
        if let Some(job) = phase1["job"].as_str() {
            if let Err(error) = self.saya_review_decision(job, "redo", client, true) {
                eprintln!("saya: saya_restart: redo of job {job} (run {run}) failed: {error}");
            }
        }
        let info = client.get("/object_info")?;
        // Restart = another draw: a new master seed (same range as the interface), the old one
        // stays in the run history. Replaying the same seed brings nothing (Saya 2026-10-22).
        let new_seed = (u128::from_le_bytes(*uuid::Uuid::new_v4().as_bytes())
            % 1_125_899_906_842_624)
            .to_string();
        if let Some(old) = value["masterSeed"].as_str().map(str::to_owned) {
            let mut history = value["previousSeeds"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            history.push(json!(old));
            value["previousSeeds"] = json!(history);
        }
        value["masterSeed"] = json!(new_seed);
        let source = value["source"].as_str().ok_or("Source Saya absente")?;
        let root = value["isolatedRoot"]
            .as_str()
            .ok_or("Racine Saya absente")?;
        let api = prepare_phase_api(source, &info, 1, Some(&new_seed), Some(root))?;
        let job = if value["sourcePath"]
            .as_str()
            .is_some_and(|path| path.starts_with("temporary-"))
        {
            self.prepare_temporary_image(
                &api["api"],
                &schema_for_validation(&info),
                Some(&serde_json::from_str(source)?),
            )?
        } else {
            let wf = self.import_internal_workflow("Saya Couple · phase 1", &api["api"].to_string())?;
            self.prepare_job(&wf, &[], &schema_for_validation(&info))?
        };
        // Superseded attempt (same lineage): every job is retired so the gallery hides it,
        // except its final, listed in `retiredFinals`, which stays visible until the new
        // attempt validates phase 6 (crate::lineage rules 1-3).
        let mut retired = value["retiredJobs"].as_array().cloned().unwrap_or_default();
        let finished = value["phases"]["6"]["state"] == "succeeded"
            && value["phases"]["6"]["asset"].is_string();
        let previous: Vec<(String, String)> = value["phases"]
            .as_object()
            .map(|phases| {
                phases
                    .iter()
                    .filter_map(|(phase, info)| {
                        info["job"].as_str().map(|job| (phase.clone(), job.to_owned()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut finals = value["retiredFinals"].as_array().cloned().unwrap_or_default();
        for (phase, job_id) in previous {
            if !retired.iter().any(|v| v.as_str() == Some(job_id.as_str())) {
                retired.push(json!(job_id));
            }
            if phase == "6" && finished && !finals.iter().any(|v| v.as_str() == Some(job_id.as_str())) {
                finals.push(json!(job_id));
            }
        }
        value["retiredJobs"] = json!(retired);
        value["retiredFinals"] = json!(finals);
        value["phases"] = json!({"1":{"job":job.job_id,"state":"submitting"}});
        value["reviewValidated"] = json!(false);
        value["autoAdvance"] = json!(false);
        value["autoError"] = Value::Null;
        value["state"] = json!("running");
        write_run(self, run, &value)?;
        self.submit_job(&job.job_id, &job.snapshot_hash, true, true, client)?;
        value["phases"]["1"]["state"] = json!("submitted");
        write_run(self, run, &value)?;
        // The run is already persisted: a sync failure is reported, not fatal.
        if let Err(error) = self.sync_jobs(client, results) {
            eprintln!("saya: saya_restart: sync_jobs failed for run {run}: {error}");
        }
        Ok(value)
    }

    pub fn saya_stop(&mut self, run: &str, client: &Comfy, results: &Path) -> CoreResult<Value> {
        // The manifest can lag behind ComfyUI: decide the controls from synced job states.
        if let Err(error) = self.sync_jobs(client, results) {
            eprintln!("saya: saya_stop: sync_jobs failed for run {run}: {error}");
        }
        let mut value = read_run(self, run)?;
        let controls = self.stop_phase_jobs(&value, client)?;
        for control in controls.iter().filter(|c| c["sent"] == false) {
            eprintln!("saya: saya_stop: control failed for run {run}: {control}");
        }
        value["state"] = json!("stopped");
        value["autoAdvance"] = json!(false);
        value["stopPending"] = json!(controls.iter().any(|c| c["sent"] == false));
        write_run(self, run, &value)?;
        // The run is already persisted: a sync failure is reported, not fatal.
        if let Err(error) = self.sync_jobs(client, results) {
            eprintln!("saya: saya_stop: sync_jobs failed for run {run}: {error}");
        }
        Ok(value)
    }
}
/// The run has no phase beyond pass 1 yet (a finished run may record only phases 1 and 6).
fn only_pass_one(run: &Value) -> bool {
    run["phases"].as_object().is_some_and(|phases| phases.keys().all(|k| k == "1"))
}

/// A casting trial whose pass 1 still has to run or is running. A step-by-step trial not yet
/// released waits for the user, not for the GPU.
fn pass_one_pending(run: &Value) -> bool {
    if run["state"] != "running" {
        return false;
    }
    let held = run["casting"]["step"] == true && run["casting"]["released"] != true;
    match run["phases"]["1"]["state"].as_str().unwrap_or_default() {
        "waiting" => !held,
        "submitting" | "submitted" | "queued" | "running" | "submission_unknown" => true,
        _ => false,
    }
}

/// A casting six-pass finish in progress: started, not completed, and not stopped on a failed
/// pass (that one waits for « Reprendre » and must not hold the queue).
fn finish_running(run: &Value) -> bool {
    run["state"] == "running"
        && !only_pass_one(run)
        && run["phases"]["6"]["state"] != "succeeded"
        && !run["phases"].as_object().is_some_and(|phases| {
            phases.values().any(|p| {
                matches!(
                    p["state"].as_str(),
                    Some("failed" | "interrupted" | "cancelled")
                )
            })
        })
}

/// The next trial of a batch may be submitted: no trial of the batch is paused, still being
/// prepared, or has its pass 1 owned by ComfyUI. Only pass 1 matters: a kept trial's later
/// pass that failed (a HiDream pass 3 out of memory) or a trial stopped, rejected or failed on
/// its own must not freeze the rest of the batch forever; those wait for « Reprendre ».
fn casting_batch_ready(runs: &[Value], batch: &str) -> bool {
    !runs
        .iter()
        .filter(|r| r["casting"]["catalogue"] == batch)
        .any(|r| {
            r["state"] == "paused"
                || (r["casting"]["deferred"] == true && r["casting"]["prepared"] != true)
                || matches!(
                    r["phases"]["1"]["state"].as_str(),
                    Some("submitted" | "submitting" | "queued" | "running" | "submission_unknown")
                )
        })
}

#[cfg(test)]
mod casting_batch_queue_tests {
    use super::*;
    fn row(state: &str, phase: &str) -> Value {
        json!({"state":state,"casting":{"catalogue":"batch","deferred":true,"prepared":true},"phases":{"1":{"state":phase}}})
    }
    #[test]
    fn batch_waits_for_running_unknown_and_paused_jobs() {
        for phase in [
            "submitted",
            "submitting",
            "queued",
            "running",
            "submission_unknown",
        ] {
            assert!(!casting_batch_ready(
                &[row("running", phase), row("running", "waiting")],
                "batch"
            ));
        }
        assert!(!casting_batch_ready(
            &[row("paused", "succeeded"), row("running", "waiting")],
            "batch"
        ));
        assert!(casting_batch_ready(
            &[row("running", "succeeded"), row("running", "waiting")],
            "batch"
        ));
    }

    /// Regression (owner, 2026-11-04): a HiDream pass 3 ran out of memory on a kept trial and
    /// the batch stayed at 9/20 forever. A failed, interrupted, stopped or rejected trial no
    /// longer holds the remaining ones.
    #[test]
    fn a_failed_or_stopped_trial_never_freezes_the_batch() {
        let mut finish_failed = row("running", "succeeded");
        finish_failed["phases"]["3"] = json!({"state":"failed"});
        for blocker in [
            finish_failed,
            row("running", "failed"),
            row("running", "interrupted"),
            row("stopped", "cancelled"),
            row("stopped", "succeeded"),
        ] {
            assert!(
                casting_batch_ready(&[blocker.clone(), row("running", "waiting")], "batch"),
                "{blocker}"
            );
        }
    }
    #[test]
    fn incomplete_preparation_and_other_batches_are_separate() {
        let mut incomplete = row("running", "waiting");
        incomplete["casting"]["prepared"] = json!(false);
        assert!(!casting_batch_ready(&[incomplete], "batch"));
        let mut other = row("paused", "running");
        other["casting"]["catalogue"] = json!("other");
        assert!(casting_batch_ready(
            &[other, row("running", "waiting")],
            "batch"
        ));
    }
}

// A recorded job is never re-submitted, even after a lost acknowledgement.
fn next_ready_phase(value: &Value) -> Option<u32> {
    if value["state"] != "running" {
        return None;
    }
    for phase in 1..=SAYA_PHASES {
        let current = &value["phases"][phase.to_string()];
        if current["state"] == "succeeded" {
            continue;
        }
        return (phase > 1
            && current.get("job").is_none()
            && (current["state"].is_null() || current["state"] == "waiting"))
            .then_some(phase);
    }
    None
}

fn chrono_now() -> String {
    format!("{:?}", std::time::SystemTime::now())
}

#[cfg(test)]
mod split_tests {
    use super::split_phase_source_in;
    use serde_json::json;

    #[test]
    fn phase_comes_from_its_pass_file_with_master_then_workspace_settings() {
        let dir = std::env::temp_dir().join(format!("saya-split-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let node = |id: u64, kind: &str, w: &str| json!({"id":id,"type":kind,"widgets_values":[w]});
        std::fs::write(dir.join("ILUSTCOUPLE_02_PASSE_2_Hires_Fix.json"), json!({"nodes":[node(810,"hub","fichier"),node(5,"vae","fichier"),node(9,"x","fichier")]}).to_string()).unwrap();
        std::fs::write(
            dir.join("ILUSTCOUPLE_00_MAITRE_CONFIG.json"),
            json!({"nodes":[node(810,"hub","maître"),node(5,"vae","maître")]}).to_string(),
        )
        .unwrap();
        let work = json!({"extra":{"saya_split":"passe 1"},"nodes":[node(810,"hub","travail"),node(9,"autre-type","travail")]}).to_string();
        let out: serde_json::Value = serde_json::from_str(
            &split_phase_source_in(&work, 2, &[dir.clone()])
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let w = |i: usize| {
            out["nodes"][i]["widgets_values"][0]
                .as_str()
                .unwrap()
                .to_owned()
        };
        assert_eq!(
            (w(0), w(1), w(2)),
            ("travail".into(), "maître".into(), "fichier".into())
        );
        assert!(
            split_phase_source_in(&json!({"nodes":[]}).to_string(), 2, &[dir.clone()])
                .unwrap()
                .is_none()
        );
        assert!(split_phase_source_in(&work, 3, &[dir]).is_err());
    }

    /// Finishing of a casting shot on a split source: each projected pass (2-6)
    /// carries the SHOT CHECKPOINT, not the disk/master one. Fixture faithful to the real thing:
    /// hub 810 (instance) and 488/489 of its definition carry
    /// `widgets_values_named`, and casting fill (`casting::fill_source`,
    /// like comfy.py fill_workflow) writes ONLY the named field — the
    /// `widgets_values` array stays the disk one. Conversion reads the named field
    /// first (`workflow_ui_api::widget_values`): without an overlay of both fields the
    /// pass falls back to the Generation checkpoint.
    #[test]
    fn each_projected_pass_carries_the_shot_checkpoint() {
        let dir = std::env::temp_dir().join(format!("saya-split-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        const GENERATION: &str = "Illustrious/Sayapacito-Alpha (config Generation)";
        const SHOT: &str = "Noobai/MKLAN POISONED NOOB AI (shot)";
        const FIXED_CHECKPOINT: &str = "SDXL/Mklan-AnimeTest6SDXL (sampler 2 fixe)";
        let hub = |ckpt: &str| {
            json!({"id": 810, "type": "hub-test", "mode": 0,
                "widgets_values": [ckpt], "widgets_values_named": {"ckpt_name": ckpt}})
        };
        let definition = |ckpt488: &str| {
            json!({"id": "hub-test", "nodes": [
                {"id": 488, "type": "CheckpointLoaderSimple", "mode": 0,
                 "widgets_values": [ckpt488], "widgets_values_named": {"ckpt_name": ckpt488}},
                {"id": 489, "type": "CheckpointLoaderSimple", "mode": 0,
                 "widgets_values": [FIXED_CHECKPOINT], "widgets_values_named": {"ckpt_name": FIXED_CHECKPOINT}}
            ]})
        };
        for phase in 2..=6 {
            let pass_doc = json!({
                "nodes": [hub(GENERATION)],
                "definitions": {"subgraphs": [definition(GENERATION)]}
            });
            std::fs::write(
                dir.join(format!(
                    "ILUSTCOUPLE_{phase:02}_PASSE_{phase}_Hires_Fix.json"
                )),
                pass_doc.to_string(),
            )
            .unwrap();
        }
        std::fs::write(
            dir.join("ILUSTCOUPLE_00_MAITRE_CONFIG.json"),
            json!({
                "nodes": [hub(GENERATION)],
                "definitions": {"subgraphs": [definition(GENERATION)]}
            })
            .to_string(),
        )
        .unwrap();
        // Working copy filled by casting: named field = shot, array frozen
        // on the old checkpoint (fill_source does not rewrite the array).
        let work = json!({
            "extra": {"saya_split": "passe 1"},
            "nodes": [{"id": 810, "type": "hub-test", "mode": 0,
                "widgets_values": [GENERATION], "widgets_values_named": {"ckpt_name": SHOT}}],
            "definitions": {"subgraphs": [definition(SHOT)]}
        })
        .to_string();
        for phase in 2..=6 {
            let out: serde_json::Value = serde_json::from_str(
                &split_phase_source_in(&work, phase, &[dir.clone()])
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
            let projected_hub = &out["nodes"][0];
            assert_eq!(
                projected_hub["widgets_values_named"]["ckpt_name"],
                json!(SHOT),
                "phase {phase} : le hub 810 doit porter le checkpoint du shot"
            );
            let projected_def = &out["definitions"]["subgraphs"][0];
            assert_eq!(
                projected_def["id"],
                json!("hub-test"),
                "phase {phase} : la définition du hub est conservée"
            );
            let n488 = projected_def["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["id"] == 488)
                .unwrap();
            assert_eq!(
                n488["widgets_values_named"]["ckpt_name"],
                json!(SHOT),
                "phase {phase} : le 488 de la définition porte le checkpoint du shot"
            );
            let n489 = projected_def["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["id"] == 489)
                .unwrap();
            assert_eq!(
                n489["widgets_values_named"]["ckpt_name"],
                json!(FIXED_CHECKPOINT),
                "phase {phase} : le sampler 2 (489) reste au checkpoint fixe"
            );
        }
    }

    /// Real conversion of the 6 passes (engine required):
    /// SAYA_OBJECT_INFO=object_info.json SAYA_PASSES_DIR=presets/passes cargo test -- --ignored real_passes
    #[test]
    #[ignore]
    fn real_passes_convert_to_api() {
        let info: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::env::var("SAYA_OBJECT_INFO").unwrap()).unwrap(),
        )
        .unwrap();
        let dir = std::path::PathBuf::from(std::env::var("SAYA_PASSES_DIR").unwrap());
        let first = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.to_string_lossy().contains("_01_PASSE_1_"))
            .unwrap();
        let source = std::fs::read_to_string(first).unwrap();
        for phase in 1..=6 {
            let split = split_phase_source_in(&source, phase, &[dir.clone()]).unwrap();
            let api = super::prepare_phase_api(
                split.as_deref().unwrap_or(&source),
                &info,
                phase,
                Some("12345"),
                Some("saya-tests/split"),
            )
            .unwrap();
            let full = std::fs::read_to_string(std::env::var("SAYA_FULL").unwrap()).unwrap();
            let reference = super::prepare_phase_api(
                &full,
                &info,
                phase,
                Some("12345"),
                Some("saya-tests/split"),
            )
            .unwrap();
            let strip = |v: &serde_json::Value| {
                let mut v = v.clone();
                for (_, n) in v.as_object_mut().unwrap() {
                    n.as_object_mut().unwrap().remove("_meta");
                }
                v
            };
            let (a, b) = (strip(&api["api"]), strip(&reference["api"]));
            for k in a
                .as_object()
                .unwrap()
                .keys()
                .chain(b.as_object().unwrap().keys())
            {
                if a.get(k) != b.get(k) {
                    eprintln!(
                        "phase {phase} diff {k}:\n  passe  {}\n  complet {}",
                        a.get(k).map_or("-".into(), |v| v.to_string()),
                        b.get(k).map_or("-".into(), |v| v.to_string())
                    );
                }
            }
            eprintln!(
                "phase {phase} : {} nodes API, identique au workflow complet",
                api["api"].as_object().unwrap().len()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> Value {
        let options = json!([["1 — BASE GENERATION", "2 — HIRES / USDU"]]);
        json!({
            "SayaImagePhaseCheckpointLoad": {"input": {"required": {"phase": options}}},
            "SayaImagePhaseCheckpointStop": {"input": {"required": {"phase": options}}}
        })
    }

    /// Seed invariants of a Saya pass, on the shapes of the real preset: the master seed
    /// lands exactly (u64 above 2^53 included) on every literal seed, whatever it held (a
    /// stale number, rgthree's -1 "random" sentinel, a detailer left on "randomize"); wired
    /// seeds keep their wire, so passes 2-6 still read the seed of the validated checkpoint.
    #[test]
    fn master_seed_reaches_every_literal_seed_and_no_wire() {
        const SEED: u64 = 18_446_744_073_709_551_557; // not representable as an f64
        let mut api = json!({
            "100032": {"class_type": "Seed (rgthree)", "inputs": {"seed": 463900182678381u64}},
            "100033:100021": {"class_type": "ClownsharKSampler_Beta",
                "inputs": {"seed": ["100032", 0], "steps": 30, "cfg": 4.5}},
            "100034:100022": {"class_type": "ClownsharKSampler_Beta", "inputs": {"seed": -1}},
            "9:29": {"class_type": "SayaDuoSegsDetail", "inputs": {"seed": 473090418616687u64}},
            "12": {"class_type": "SamplerCustom", "inputs": {"noise_seed": 5, "cfg": 1.0}},
            "100055:100088": {"class_type": "ClownsharKSampler_Beta",
                "inputs": {"seed": ["100055:10150", 3]}},
            "100060": {"class_type": "SayaImagePhaseCheckpointStop",
                "inputs": {"seed": "[]", "models_json": "[]"}},
            "7": {"class_type": "EmptyLatentImage", "inputs": {"width": 832, "batch_size": 1}}
        });
        let before = api.clone();
        let targets = apply_master_seed(&mut api, SEED).unwrap();
        let written: Vec<String> = targets
            .iter()
            .map(|t| format!("{}.{}", t["node"].as_str().unwrap(), t["input"].as_str().unwrap()))
            .collect();
        assert_eq!(
            written,
            ["100032.seed", "100034:100022.seed", "12.noise_seed", "9:29.seed"]
        );
        for target in &targets {
            let value = &api[target["node"].as_str().unwrap()]["inputs"][target["input"].as_str().unwrap()];
            assert_eq!(value.as_u64(), Some(SEED), "exact u64 on {target}");
        }
        // Everything else is byte-identical: wires, text widgets, sampling parameters.
        for (node, input) in [
            ("100033:100021", "seed"),
            ("100033:100021", "steps"),
            ("100033:100021", "cfg"),
            ("100055:100088", "seed"),
            ("100060", "seed"),
            ("100060", "models_json"),
            ("12", "cfg"),
            ("7", "width"),
        ] {
            assert_eq!(api[node]["inputs"][input], before[node]["inputs"][input], "{node}.{input}");
        }
        // The exact value survives serialization to the engine.
        assert!(api.to_string().contains("18446744073709551557"));
        assert!(apply_master_seed(&mut json!([]), SEED).is_err());
    }

    #[test]
    fn isolated_root_repoints_fixed_phase_nodes_only() {
        let mut api = json!({
            "100050:10123": {"class_type": "SayaImagePhase2Load", "inputs": {}},
            "100051": {"class_type": "SayaImagePhase2Stop", "inputs": {"image": ["100050:10123", 0]}},
            "100054": {"class_type": "SayaImageGenerationReview", "inputs": {"checkpoint_root": "image/checkpoints"}},
            "generic": {"class_type": "SayaImagePhaseCheckpointLoad", "inputs": {"phase": "2 — HIRES / USDU"}}
        });
        apply_isolated_phase_root(&mut api, &info(), "saya-tests/isolated");
        assert_eq!(
            api["100050:10123"]["class_type"],
            "SayaImagePhaseCheckpointLoad"
        );
        assert_eq!(api["100050:10123"]["inputs"]["phase"], "2 — HIRES / USDU");
        assert_eq!(api["100050:10123"]["inputs"]["detailer"], "none");
        assert_eq!(
            api["100050:10123"]["inputs"]["checkpoint_root"],
            "saya-tests/isolated"
        );
        assert_eq!(api["100051"]["class_type"], "SayaImagePhaseCheckpointStop");
        assert_eq!(
            api["100051"]["inputs"]["checkpoint_root"],
            "saya-tests/isolated"
        );
        assert_eq!(api["100054"]["class_type"], "SayaImageGenerationReview");
        assert!(api["generic"]["inputs"].get("checkpoint_root").is_none());
        assert!(api["generic"]["inputs"].get("detailer").is_none());
    }

    #[test]
    fn phase_conversion_keeps_shared_base_and_activates_owned_hubs() {
        let source = json!({
            "nodes": [
                {"id": 1, "type": "SetNode", "mode": 0,
                 "properties": {"saya_phase": 1, "saya_shared_base": true}},
                {"id": 2, "type": "CLIPTextEncode", "mode": 0,
                 "properties": {"saya_phase": 3}},
                {"id": 3, "type": "PrivateNode", "mode": 0,
                 "properties": {"saya_phase": 1}},
                {"id": 5, "type": "hub", "mode": 0,
                 "properties": {"saya_phase": 1, "saya_shared_base": true}},
                {"id": 6, "type": "saya-model-hub-detailers", "mode": 2,
                 "properties": {}},
                {"id": 7, "type": "saya-model-hub-phase6", "mode": 2,
                 "properties": {}}
            ],
            "links": [],
            "definitions": {"subgraphs": [
                {"id": "hub", "nodes": [
                    {"id": 4, "type": "QuadrupleCLIPLoader", "mode": 0,
                     "properties": {"saya_phase": 1}}
                ]}
            ]}
        });
        let text = prepare_phase_source(&source.to_string(), 3).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["nodes"][0]["properties"]["saya_phase"], 3);
        assert_eq!(value["nodes"][1]["properties"]["saya_phase"], 3);
        assert_eq!(value["nodes"][2]["properties"]["saya_phase"], 1);
        assert_eq!(
            value["definitions"]["subgraphs"][0]["nodes"][0]["properties"]["saya_phase"], 3,
            "definitions reused by a shared instance must stay active"
        );
        assert_eq!(
            value["nodes"][4]["mode"], 2,
            "phase 3 keeps other hubs muted"
        );
        assert_eq!(
            value["nodes"][5]["mode"], 2,
            "phase 3 keeps other hubs muted"
        );

        let phase5 = prepare_phase_source(&source.to_string(), 5).unwrap();
        let phase5: Value = serde_json::from_str(&phase5).unwrap();
        assert_eq!(
            phase5["nodes"][4]["mode"], 0,
            "detailer hub active in phase 5"
        );
        assert_eq!(
            phase5["nodes"][5]["mode"], 2,
            "phase-6 hub muted outside its phase"
        );

        let phase6 = prepare_phase_source(&source.to_string(), 6).unwrap();
        let phase6: Value = serde_json::from_str(&phase6).unwrap();
        assert_eq!(
            phase6["nodes"][4]["mode"], 2,
            "detailer hub untouched in phase 6"
        );
        assert_eq!(
            phase6["nodes"][5]["mode"], 0,
            "phase-6 hub active in phase 6"
        );
    }

    #[test]
    fn combo_outputs_are_wildcards_for_validation_only() {
        let info = json!({
            "JPS": {"output": [["euler", "heun"], ["simple", "karras"]],
                    "input": {"required": {}}},
            "KSampler": {"output": ["LATENT"], "input": {"required": {}}}
        });
        let schema = schema_for_validation(&info);
        assert_eq!(schema["JPS"]["output"], json!(["*", "*"]));
        assert_eq!(schema["KSampler"]["output"], json!(["LATENT"]));
        assert_eq!(
            info["JPS"]["output"][0][0], "euler",
            "input schema untouched"
        );
    }

    #[test]
    fn fixed_phase_node_matches_hidden_variants_only() {
        assert_eq!(fixed_phase_node("SayaImagePhase2Load").unwrap().0, 2);
        assert_eq!(fixed_phase_node("SayaImagePhase6Stop").unwrap().0, 6);
        assert!(fixed_phase_node("SayaImagePhaseCheckpointLoad").is_none());
        assert!(fixed_phase_node("SayaImagePhase1Stop").is_none());
        assert!(fixed_phase_node("SayaImageGenerationReview").is_none());
    }

    /// Faithful to the real graph imported from a PNG (ILUSTCOUPLE, definition
    /// "saya-model-hub-detailers"): an untagged OPTIONAL model hub,
    /// saved muted, whose MODEL inputs come from a provider tagged
    /// phase 5; the 13 internal SayaLazyModelSelect receive model_1/model_2
    /// through the subgraph's -10 ports (dict links).
    fn hub_fixture() -> String {
        let provider = |id: u64, phase: Option<u32>| {
            let mut node = json!({
                "id": id, "type": "SayaTestUpscaler", "mode": 0,
                "inputs": [],
                "outputs": [{"name": "MODEL", "type": "MODEL", "links": [900, 901]}],
            });
            if let Some(phase) = phase {
                node["properties"] = json!({"saya_phase": phase});
            }
            node
        };
        json!({
            "nodes": [
                provider(1, Some(5)),
                {"id": 2, "type": "SayaTestCheckpoint", "mode": 0, "inputs": [],
                 "outputs": [{"name": "MODEL", "type": "MODEL", "links": [902]}]},
                {"id": 3, "type": "SayaTestPreview", "mode": 0,
                 "inputs": [{"name": "model", "type": "MODEL", "link": 902}],
                 "outputs": [], "properties": {}},
                {"id": 4, "type": "SayaTestPreview", "mode": 0,
                 "inputs": [{"name": "model", "type": "MODEL", "link": 903}],
                 "outputs": [], "properties": {"saya_phase": 5}},
                {"id": 100208, "type": "saya-model-hub-detailers", "mode": 2,
                 "title": "Hub Detailers · Model Routing", "properties": {},
                 "inputs": [
                     {"name": "Model 1 Patched", "type": "MODEL", "link": 900},
                     {"name": "Model 2 Patched", "type": "MODEL", "link": 901}
                 ],
                 "outputs": [{"name": "MODEL", "type": "MODEL", "links": [903]}]}
            ],
            "links": [
                [900, 1, 0, 100208, 0, "MODEL"],
                [901, 1, 0, 100208, 1, "MODEL"],
                [902, 2, 0, 3, 0, "MODEL"],
                [903, 100208, 0, 4, 0, "MODEL"]
            ],
            "definitions": {"subgraphs": [{
                "id": "saya-model-hub-detailers",
                "inputs": [
                    {"name": "Model 1 Patched", "type": "MODEL", "linkIds": [98319]},
                    {"name": "Model 2 Patched", "type": "MODEL", "linkIds": [98320]}
                ],
                "outputs": [{"name": "MODEL", "type": "MODEL", "linkIds": [98321]}],
                "nodes": [{
                    "id": 100185, "type": "SayaLazyModelSelect", "mode": 0,
                    "properties": {},
                    "inputs": [
                        {"name": "model_1", "type": "MODEL", "link": 98319},
                        {"name": "model_2", "type": "MODEL", "link": 98320}
                    ],
                    "outputs": [{"name": "MODEL", "type": "MODEL", "links": [98321]}]
                }],
                "links": [
                    {"id": 98319, "origin_id": -10, "origin_slot": 0,
                     "target_id": 100185, "target_slot": 0, "type": "MODEL"},
                    {"id": 98320, "origin_id": -10, "origin_slot": 1,
                     "target_id": 100185, "target_slot": 1, "type": "MODEL"},
                    {"id": 98321, "origin_id": 100185, "origin_slot": 0,
                     "target_id": -20, "target_slot": 0, "type": "MODEL"}
                ]
            }]},
            "extra": {}
        })
        .to_string()
    }

    fn hub_info() -> Value {
        json!({
            "SayaTestUpscaler": {"input": {"required": {}}, "output": ["MODEL"], "output_node": false},
            "SayaTestCheckpoint": {"input": {"required": {}}, "output": ["MODEL"], "output_node": false},
            "SayaTestPreview": {"input": {"required": {"model": ["MODEL"]}}, "output": ["IMAGE"], "output_node": true},
            "SayaLazyModelSelect": {"input": {"required": {
                "model_1": ["MODEL", {"forceInput": true}],
                "model_2": ["MODEL", {"forceInput": true}]
            }}, "output": ["MODEL"], "output_node": false}
        })
    }

    #[test]
    fn temporary_image_import_converts_phase_1_without_optional_hubs() {
        let source = hub_fixture();
        // Image import activates the working copy (ImageTemporaryOpen),
        // which also wakes the optional hub saved muted.
        let mut workflow: Value = serde_json::from_str(&source).unwrap();
        crate::image_restore::activate_workflow(&mut workflow);
        assert!(crate::image_restore::is_saya_workflow(&workflow));
        let activated = workflow.to_string();

        // The original file (hub still muted) already converts in phase 1.
        super::prepare_phase_api(&source, &hub_info(), 1, None, None).unwrap();

        // Copy activated by the import: must convert without the optional hubs.
        let phase1 = super::prepare_phase_api(&activated, &hub_info(), 1, None, None).unwrap();
        let api = phase1["api"].as_object().unwrap();
        assert!(
            !api.keys().any(|id| id.starts_with("100208")),
            "le hub optionnel muet ne doit pas être exécuté en phase 1"
        );
        assert!(api.contains_key("2") && api.contains_key("3"));

        // Phase 5: the hub belongs to the phase, the lazy ones stay resolved.
        let phase5 = super::prepare_phase_api(&activated, &hub_info(), 5, None, None).unwrap();
        let lazy = &phase5["api"]["100208:100185"];
        assert_eq!(lazy["inputs"]["model_1"], json!(["1", 0]));
        assert_eq!(lazy["inputs"]["model_2"], json!(["1", 0]));
    }

    /// Phase 5 OOM (2026-10-26): the unwired PRECHARGE loaders that the engine
    /// decorates in the embedded workflow (moteur/comfy.py:845) must never
    /// go into execution — otherwise HiDream+T5+Llama (~20 GB) stay resident
    /// during the detailers. Closure faithful to comfy.minimal_prompt: a node
    /// with no consumer reachable from an output is not submitted.
    #[test]
    fn unwired_loader_nodes_are_pruned_from_phase_api() {
        let source = json!({
            "nodes": [
                {"id": 900001, "type": "SayaTestUpscaler", "mode": 0,
                 "inputs": [],
                 "outputs": [{"name": "MODEL", "type": "MODEL", "links": null}],
                 "properties": {}, "title": "PRECHARGE · HiDream"},
                {"id": 1, "type": "SayaTestCheckpoint", "mode": 0, "inputs": [],
                 "outputs": [{"name": "MODEL", "type": "MODEL", "links": [10]}]},
                {"id": 2, "type": "SayaTestPreview", "mode": 0,
                 "inputs": [{"name": "model", "type": "MODEL", "link": 10}],
                 "outputs": [], "properties": {}}
            ],
            "links": [[10, 1, 0, 2, 0, "MODEL"]],
            "definitions": {"subgraphs": []},
            "extra": {}
        })
        .to_string();
        let phase = super::prepare_phase_api(&source, &hub_info(), 1, None, None).unwrap();
        let api = phase["api"].as_object().unwrap();
        assert!(
            api.contains_key("1") && api.contains_key("2"),
            "chaîne vivante conservée"
        );
        assert!(
            !api.contains_key("900001"),
            "loader non câblé (PRECHARGE) élagué : jamais soumis à ComfyUI"
        );
    }
}

#[cfg(test)]
mod automatic_tests {
    use super::*;
    #[test]
    fn only_a_successful_contiguous_chain_can_advance() {
        let mut run = json!({"state":"running","phases":{"1":{"state":"succeeded"}}});
        assert_eq!(next_ready_phase(&run), Some(2));
        run["phases"]["2"] = json!({"state":"succeeded","job":"two"});
        assert_eq!(next_ready_phase(&run), Some(3));
        for state in [
            "submitting",
            "queued",
            "running",
            "failed",
            "cancelled",
            "interrupted",
            "submission_unknown",
        ] {
            run["phases"]["3"] = json!({"state":state,"job":"three"});
            assert_eq!(next_ready_phase(&run), None, "{state}");
        }
        run["phases"].as_object_mut().unwrap().remove("3");
        run["state"] = json!("stopped");
        assert_eq!(next_ready_phase(&run), None);
        run["state"] = json!("running");
        run["phases"]["1"]["state"] = json!("failed");
        assert_eq!(next_ready_phase(&run), None);
        for n in 1..=6 {
            run["phases"][n.to_string()] = json!({"state":"succeeded"});
        }
        assert_eq!(next_ready_phase(&run), None);
    }
}

#[cfg(test)]
mod casting_recovery_tests {
    use super::*;
    #[test]
    fn casting_history_recovers_no_and_catalogue_without_engine_access() {
        let root = std::env::temp_dir().join(format!("casting-recovery-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("db.sqlite3");
        let mut db = Database::open(&path).unwrap();
        let id = "casting-recovery-001";
        let shot = json!({"stem":"hum-one","name":"One","famille":"hum","model":"noobai","seed":"18446744073709551615","casting":{"p1":"exact,  prompt","prefixe":"quality","lora_loaders":{}}});
        write_run(&db,id,&json!({"id":id,"state":"running","masterSeed":"18446744073709551615","createdAt":"2026-10-25","phases":{"1":{"state":"succeeded","asset":"image-1","job":"job-1"}},"casting":{"graine":"9007199254740993","catalogue":"immutable/catalogue.json","shot":shot,"decision":""}})).unwrap();
        db.casting_decide(id, false, &Comfy::loopback(1).unwrap(), &root)
            .unwrap();
        drop(db);
        let mut db = Database::open(&path).unwrap();
        let rows = db.casting_history().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["decision"], "non");
        assert_eq!(rows[0]["catalogue"], "immutable/catalogue.json");
        assert_eq!(rows[0]["seed"], "18446744073709551615");
        assert_eq!(rows[0]["asset"], "image-1");
        assert!(db
            .casting_decide(id, true, &Comfy::loopback(1).unwrap(), &root)
            .unwrap_err()
            .to_string()
            .contains("déjà enregistré"));
    }
}

#[cfg(test)]
mod casting_duo_tests {
    use super::*;
    #[test]
    fn a_duo_trial_never_starts_the_six_passes() {
        let root = std::env::temp_dir().join(format!("casting-duo-keep-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let mut db = Database::open(&root.join("db.sqlite3")).unwrap();
        let id = "casting-duo-keep-001";
        write_run(&db, id, &json!({"id":id,"state":"running","phases":{"1":{"state":"succeeded"}},
            "casting":{"shot":{"duo":true,"stem":"duo-one","name":"A x B"}}})).unwrap();
        let client = Comfy::loopback(1).unwrap();
        let error = db.casting_decide(id, true, &client, &root).unwrap_err().to_string();
        assert!(error.contains("passe 1"), "{error}");
        db.casting_decide(id, false, &client, &root).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
    }
}

#[cfg(test)]
mod run_manifest_resilience_tests {
    use super::*;

    fn base() -> (PathBuf, Database) {
        let dir = std::env::temp_dir().join(format!("saya-manifests-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
        (dir, db)
    }

    fn run_json(id: &str, phases: Value) -> Value {
        json!({"id": id, "state": "running", "createdAt": "2026-10-26T10:00:00Z", "phases": phases})
    }

    #[test]
    fn write_run_leaves_no_temp_file() {
        let (_dir, db) = base();
        write_run(&db, "atomic-run-01", &run_json("atomic-run-01", json!({}))).unwrap();
        write_run(
            &db,
            "atomic-run-01",
            &run_json("atomic-run-01", json!({"1": {"job": "j"}})),
        )
        .unwrap();
        let names: Vec<String> = std::fs::read_dir(runs_root(&db).unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["atomic-run-01.json".to_owned()]);
        assert_eq!(
            read_run(&db, "atomic-run-01").unwrap()["phases"]["1"]["job"],
            "j"
        );
    }

    #[test]
    fn corrupt_manifest_is_skipped_and_stale_temps_are_removed() {
        let (_dir, db) = base();
        write_run(&db, "good-run-0001", &run_json("good-run-0001", json!({}))).unwrap();
        let root = runs_root(&db).unwrap();
        std::fs::write(root.join("bad-run-0001.json"), b"{not json").unwrap();
        std::fs::write(root.join("array-run-01.json"), b"[1,2]").unwrap();
        let stale = root.join("old.tmp");
        let fresh = root.join("new.tmp");
        std::fs::write(&stale, b"x").unwrap();
        std::fs::write(&fresh, b"x").unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
        std::fs::File::options()
            .write(true)
            .open(&stale)
            .unwrap()
            .set_modified(old)
            .unwrap();
        let runs = db.saya_runs().unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0]["id"], "good-run-0001");
        assert!(!stale.exists(), "stale tmp removed");
        assert!(fresh.exists(), "recent tmp kept");
        assert!(
            root.join("bad-run-0001.json").exists(),
            "json never deleted"
        );
    }

    fn insert_job_asset(db: &Database, dir: &Path, job: &str, asset: &str) {
        let library = dir.join("library");
        std::fs::create_dir_all(&library).unwrap();
        std::fs::write(library.join(format!("{asset}.png")), b"synthetic").unwrap();
        let root = library.to_string_lossy().replace('\'', "''");
        db.connection
            .execute_batch(&format!(
                "INSERT OR IGNORE INTO libraries(id,label,root_path,platform) VALUES('lib','Synthetic','{root}','linux');
                 INSERT OR IGNORE INTO workflow_revisions(id,name,source_sha256,format,document_json) VALUES('wf','Synthetic','hash','comfy_api','{{}}');
                 INSERT INTO assets(id,library_id,relative_path,availability) VALUES('{asset}','lib','{asset}.png','present');
                 INSERT INTO generation_jobs(id,workflow_revision_id,seed,state,request_snapshot_json) VALUES('{job}','wf','1','succeeded','{{}}');
                 INSERT INTO job_outputs(job_id,asset_id,node_id,output_index,output_kind) VALUES('{job}','{asset}','9',0,'saved');"
            ))
            .unwrap();
    }

    fn visible_ids(db: &Database) -> Vec<String> {
        let mut ids: Vec<String> = db
            .gallery(&crate::gallery::Filters::default())
            .unwrap()
            .items
            .into_iter()
            .map(|a| a.id)
            .collect();
        ids.sort();
        ids
    }

    #[test]
    fn gallery_skips_corrupt_manifest_and_hides_retired_jobs() {
        let (dir, db) = base();
        insert_job_asset(&db, &dir, "job-old-0001", "asset-old");
        insert_job_asset(&db, &dir, "job-new-0001", "asset-new");
        let root = runs_root(&db).unwrap();
        std::fs::write(root.join("bad-run-0001.json"), b"\x00garbage").unwrap();
        // Before any valid manifest references the jobs, both are visible.
        assert_eq!(visible_ids(&db), vec!["asset-new", "asset-old"]);
        let mut run = run_json(
            "restart-run-1",
            json!({"1": {"job": "job-new-0001", "state": "submitted"}}),
        );
        run["retiredJobs"] = json!(["job-old-0001"]);
        write_run(&db, "restart-run-1", &run).unwrap();
        assert_eq!(visible_ids(&db), vec!["asset-new"]);
    }
}

#[cfg(test)]
mod run_cache_tests {
    use super::{saya_runs_root_for_tests, write_run};
    use crate::Database;
    use serde_json::json;

    /// The listing cache must never serve a stale run: an update, a new run and a deletion are
    /// all visible at the next listing, and `source` never reaches the listing.
    #[test]
    fn listing_follows_updates_creations_and_deletions() {
        let dir = std::env::temp_dir().join(format!("saya-run-cache-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
        write_run(&db, "run-cache-0001", &json!({"id":"run-cache-0001","state":"running","source":"{}"})).unwrap();
        assert_eq!(db.saya_runs().unwrap()[0]["state"], "running");
        assert!(db.saya_runs().unwrap()[0].get("source").is_none());
        write_run(&db, "run-cache-0001", &json!({"id":"run-cache-0001","state":"completed!","source":"{}"})).unwrap();
        assert_eq!(db.saya_runs().unwrap()[0]["state"], "completed!", "update seen");
        write_run(&db, "run-cache-0002", &json!({"id":"run-cache-0002","state":"running"})).unwrap();
        assert_eq!(db.saya_runs().unwrap().len(), 2, "creation seen");
        std::fs::remove_file(saya_runs_root_for_tests(&db).join("run-cache-0001.json")).unwrap();
        let runs = db.saya_runs().unwrap();
        assert_eq!(runs.len(), 1, "deletion seen");
        assert_eq!(runs[0]["id"], "run-cache-0002");
        let _ = std::fs::remove_dir_all(dir);
    }
}
