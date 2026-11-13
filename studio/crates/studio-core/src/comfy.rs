use crate::{gallery::digest, Database, Result};
use rusqlite::params;
use serde_json::{json, Value};
use std::{
    io::Read,
    path::{Component, Path, PathBuf},
    time::Duration,
};

pub const ENDPOINT: &str = "http://127.0.0.1:8188";

/// The engine could not be reached at all: the request was never sent.
#[derive(Debug)]
pub struct EngineOffline(String);
impl std::fmt::Display for EngineOffline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ComfyUI hors ligne : connexion refusée par le moteur local ({})",
            self.0
        )
    }
}
impl std::error::Error for EngineOffline {}

/// The engine answered with an HTTP error status. `body` is truncated for display.
#[derive(Debug)]
pub struct EngineHttpError {
    pub status: reqwest::StatusCode,
    pub body: String,
}
impl std::fmt::Display for EngineHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ComfyUI HTTP {}: {}", self.status, self.body)
    }
}
impl std::error::Error for EngineHttpError {}

/// True when the engine certainly did not take the request: it was unreachable, or it answered
/// with a client error (ComfyUI validates a prompt before queuing it and answers 400). A timeout,
/// a dropped response or a server error stay uncertain.
pub(crate) fn definitely_not_accepted(error: &(dyn std::error::Error + Send + Sync + 'static)) -> bool {
    error.downcast_ref::<EngineOffline>().is_some()
        || error
            .downcast_ref::<EngineHttpError>()
            .is_some_and(|e| e.status.is_client_error())
}

/// The engine reports a finished prompt whose output can never be fetched or read (file gone,
/// not an image, over the size limit). Retrying cannot help.
#[derive(Debug)]
pub struct OutputUnavailable(String);
impl std::fmt::Display for OutputUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for OutputUnavailable {}
pub struct Comfy {
    client: reqwest::blocking::Client,
    endpoint: String,
}
impl Comfy {
    pub fn new() -> Result<Self> {
        Self::loopback(8188)
    }
    pub fn loopback(port: u16) -> Result<Self> {
        Self::loopback_with_timeout(port, Duration::from_secs(2), Duration::from_secs(15))
    }
    /// Loopback client with explicit timeouts, for best-effort probes that must never stall.
    pub fn loopback_with_timeout(port: u16, connect: Duration, total: Duration) -> Result<Self> {
        Ok(Self {
            endpoint: format!("http://127.0.0.1:{port}"),
            client: reqwest::blocking::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(connect)
                .timeout(total)
                .build()?,
        })
    }
    pub fn get(&self, path: &str) -> Result<Value> {
        self.json(
            self.client
                .get(format!("{}{path}", self.endpoint))
                .send()
                .map_err(|e| self.connect_error(e))?,
        )
    }
    /// A closed engine must read as one short sentence, not a raw reqwest
    /// dump. Only connection failures are rewritten; other errors keep their
    /// full detail for diagnosis.
    fn connect_error(&self, error: reqwest::Error) -> Box<dyn std::error::Error + Send + Sync> {
        if error.is_connect() {
            Box::new(EngineOffline(self.endpoint.clone()))
        } else {
            error.into()
        }
    }
    fn json(&self, mut response: reqwest::blocking::Response) -> Result<Value> {
        let status = response.status();
        let mut body = Vec::new();
        response
            .by_ref()
            .take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut body)?;
        if body.len() > 32 * 1024 * 1024 {
            return Err("Réponse ComfyUI trop grande".into());
        }
        if !status.is_success() {
            return Err(Box::new(EngineHttpError {
                status,
                body: String::from_utf8_lossy(&body[..body.len().min(2000)]).into_owned(),
            }));
        }
        Ok(serde_json::from_slice(&body)?)
    }
    pub fn status(&self) -> Value {
        match self
            .get("/system_stats")
            .and_then(|stats| describe_engine(&stats, &self.get("/queue")?))
        {
            Ok(status) => status,
            Err(e) => {
                json!({"connected":false,"version":null,"running":null,"pending":null,"queue":null,"error":e.to_string()})
            }
        }
    }
    pub fn models(&self) -> Result<Value> {
        model_choices(&self.get("/object_info")?)
    }
    fn submit(&self, payload: Value) -> Result<Value> {
        self.json(
            self.client
                .post(format!("{}/prompt", self.endpoint))
                .json(&payload)
                .send()
                .map_err(|e| self.connect_error(e))?,
        )
    }
    /// Uploads one PNG into the engine's input folder (`/upload/image`) under a
    /// plain file name, so a LoadImage node can read it. Multipart is built by
    /// hand (no extra dependency). Returns the name the engine reports.
    pub fn upload_image(&self, name: &str, bytes: &[u8]) -> Result<String> {
        if name.is_empty()
            || name.len() > 128
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
            || !name.ends_with(".png")
        {
            return Err("Nom d'entrée moteur invalide".into());
        }
        if bytes.len() > 64 * 1024 * 1024 {
            return Err("Image d'entrée supérieure à 64 Mio".into());
        }
        let boundary = format!("saya-{}", uuid::Uuid::new_v4().simple());
        let mut body = Vec::with_capacity(bytes.len() + 512);
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"{name}\"\r\nContent-Type: image/png\r\n\r\n").as_bytes());
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"type\"\r\n\r\ninput\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"overwrite\"\r\n\r\ntrue\r\n--{boundary}--\r\n").as_bytes());
        let reply = self.json(
            self.client
                .post(format!("{}/upload/image", self.endpoint))
                .header(
                    "Content-Type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(body)
                .send()
                .map_err(|e| self.connect_error(e))?,
        )?;
        let stored = reply["name"].as_str().ok_or("Réponse d'upload sans nom")?;
        let subfolder = reply["subfolder"].as_str().unwrap_or("");
        if !subfolder.is_empty() {
            return Err("Upload rangé dans un sous-dossier inattendu".into());
        }
        Ok(stored.to_owned())
    }
    /// Saya-controlled memory release. ComfyUI's /free only queues a flag, so
    /// we force a minimal prompt to make its worker process the flag, then
    /// verify the reported free VRAM. No model node or workflow change.
    pub fn force_unload_models(&self, minimum_free_mib: u64) -> Result<Value> {
        self.post_empty("/free", json!({"unload_models":true,"free_memory":true}))?;
        let flush = self.post_json("/prompt", json!({
            "prompt":{
                "1":{"class_type":"EmptyImage","inputs":{"width":64,"height":64,"batch_size":1,"color":0}},
                "2":{"class_type":"PreviewImage","inputs":{"images":["1",0]}}
            },
            "client_id":"saya-memory-flush"
        }))?;
        if let Some(id) = flush["prompt_id"].as_str() {
            for _ in 0..40 {
                if self
                    .get(&format!("/history/{id}"))
                    .ok()
                    .is_some_and(|h| h.get(id).is_some())
                {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
        }
        self.post_empty("/free", json!({"unload_models":true,"free_memory":true}))?;
        let mut free = 0;
        for _ in 0..40 {
            free = self
                .get("/system_stats")
                .ok()
                .and_then(|s| {
                    s["devices"]
                        .as_array()
                        .and_then(|d| d.first())
                        .and_then(|d| d["vram_free"].as_u64())
                })
                .unwrap_or(0)
                / 1_048_576;
            if free >= minimum_free_mib {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        if free < minimum_free_mib {
            return Err(format!(
                "Déchargement insuffisant : {free} MiB libres < {minimum_free_mib} MiB"
            )
            .into());
        }
        Ok(
            json!({"freeMib":free,"minimumFreeMib":minimum_free_mib,"flushPrompt":flush["prompt_id"]}),
        )
    }

    pub fn post_empty(&self, path: &str, payload: Value) -> Result<()> {
        self.client
            .post(format!("{}{path}", self.endpoint))
            .json(&payload)
            .send()
            .map_err(|e| self.connect_error(e))?
            .error_for_status()?;
        Ok(())
    }

    pub fn post_json(&self, path: &str, payload: Value) -> Result<Value> {
        self.json(
            self.client
                .post(format!("{}{path}", self.endpoint))
                .json(&payload)
                .send()
                .map_err(|e| self.connect_error(e))?,
        )
    }
    pub(crate) fn control(&self, path: &str, payload: Value) -> Result<()> {
        self.client
            .post(format!("{}{path}", self.endpoint))
            .json(&payload)
            .send()
            .map_err(|e| self.connect_error(e))?
            .error_for_status()?;
        Ok(())
    }
    /// Upload a verified latent container to the engine input folder. ComfyUI
    /// LoadLatent resolves it by file name; the extension is the one produced
    /// by SaveLatent (`.latent`), never assumed for display.
    pub fn upload_latent(&self, name: &str, bytes: &[u8]) -> Result<String> {
        if name.is_empty()
            || name.len() > 128
            || !name.ends_with(".latent")
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        {
            return Err("Nom de latent moteur invalide".into());
        }
        if bytes.len() > 64 * 1024 * 1024 {
            return Err("Latent supérieur à 64 Mio".into());
        }
        validate_latent(bytes)?;
        let boundary = format!("saya-{}", uuid::Uuid::new_v4().simple());
        let mut body = Vec::with_capacity(bytes.len() + 512);
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes());
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"type\"\r\n\r\ninput\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"overwrite\"\r\n\r\ntrue\r\n--{boundary}--\r\n").as_bytes());
        let reply = self.json(
            self.client
                .post(format!("{}/upload/image", self.endpoint))
                .header(
                    "Content-Type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(body)
                .send()
                .map_err(|e| self.connect_error(e))?,
        )?;
        let stored = reply["name"]
            .as_str()
            .ok_or("Réponse d'upload latent sans nom")?;
        if !reply["subfolder"].as_str().unwrap_or("").is_empty() {
            return Err("Upload latent rangé dans un sous-dossier inattendu".into());
        }
        Ok(stored.to_owned())
    }

    fn download_latent(&self, descriptor: &Value) -> Result<Vec<u8>> {
        let file = descriptor["filename"]
            .as_str()
            .ok_or("Nom de latent absent")?;
        let sub = descriptor["subfolder"].as_str().unwrap_or("");
        let kind = descriptor["type"].as_str().unwrap_or("output");
        if file.contains(['/', '\\'])
            || file.is_empty()
            || file == "."
            || file == ".."
            || sub.contains('\\')
            || Path::new(sub)
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            || !["output", "temp"].contains(&kind)
        {
            return Err("Chemin de latent non autorisé".into());
        }
        let response = self
            .client
            .get(format!("{}/view", self.endpoint))
            .query(&[("filename", file), ("subfolder", sub), ("type", kind)])
            .send()
            .map_err(|e| self.connect_error(e))?
            .error_for_status()?;
        let mut bytes = Vec::new();
        response
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err("Latent supérieur à 64 Mio".into());
        }
        validate_latent(&bytes)?;
        Ok(bytes)
    }

    fn download(&self, descriptor: &Value) -> Result<Vec<u8>> {
        let file = descriptor["filename"]
            .as_str()
            .ok_or("Nom de sortie absent")?;
        let sub = descriptor["subfolder"].as_str().unwrap_or("");
        let kind = descriptor["type"].as_str().unwrap_or("output");
        if file.contains(['/', '\\'])
            || file.is_empty()
            || file == "."
            || file == ".."
            || sub.contains('\\')
            || Path::new(sub)
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            || !["output", "temp"].contains(&kind)
        {
            return Err("Chemin de sortie non autorisé".into());
        }
        let response = self
            .client
            .get(format!("{}/view", self.endpoint))
            .query(&[("filename", file), ("subfolder", sub), ("type", kind)])
            .send()
            .map_err(|e| self.connect_error(e))?;
        if response.status().is_client_error() {
            return Err(Box::new(OutputUnavailable(format!(
                "Sortie « {file} » introuvable dans ComfyUI (HTTP {})",
                response.status()
            ))));
        }
        let response = response.error_for_status()?;
        let mut bytes = Vec::new();
        response
            .take(MAX_OUTPUT_BYTES + 1)
            .read_to_end(&mut bytes)?;
        validate_output(&bytes)
            .map_err(|error| Box::new(OutputUnavailable(format!("Sortie « {file} » illisible : {error}"))))?;
        Ok(bytes)
    }
}
fn validate_latent(bytes: &[u8]) -> Result<()> {
    let Some(header) = bytes
        .first_chunk::<8>()
        .filter(|_| bytes.len() <= 64 * 1024 * 1024)
    else {
        return Err("Conteneur latent de taille invalide".into());
    };
    let header_len = u64::from_le_bytes(*header) as usize;
    if header_len == 0 || header_len > bytes.len().saturating_sub(8) {
        return Err("En-tête latent invalide".into());
    }
    let header: Value = serde_json::from_slice(&bytes[8..8 + header_len])
        .map_err(|_| "En-tête safetensors latent invalide")?;
    let tensor = header
        .get("latent_tensor")
        .ok_or("latent_tensor absent du conteneur SaveLatent")?;
    let data_len = bytes.len() - 8 - header_len;
    let end = tensor["data_offsets"][1]
        .as_u64()
        .ok_or("Offsets latent invalides")? as usize;
    if end > data_len || tensor["dtype"] != "F32" {
        return Err("Tenseur latent incohérent".into());
    }
    Ok(())
}

/// Largest image output accepted from the engine.
const MAX_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;

fn validate_output(bytes: &[u8]) -> Result<()> {
    if bytes.len() as u64 > MAX_OUTPUT_BYTES {
        return Err("Sortie supérieure à 64 Mio".into());
    }
    let format = image::guess_format(bytes)?;
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader.decode()?;
    Ok(())
}
/// Classes whose image is never a render of its own: the Saya review gate re-emits the pass image
/// without its metadata, `MaskPreview` draws a mask.
const AUXILIARY_IMAGE_CLASSES: &[&str] = &["SayaImageGenerationReview", "MaskPreview"];

/// True for the image of an auxiliary node (review gate, mask preview) when another node of the
/// same job produced a real image: the copy is not a render and must not be delivered beside it.
/// Alone, it is the only image of the job and is kept.
fn auxiliary_copy_is_redundant(snapshot: &Value, outputs: &Value, node: &str) -> bool {
    let auxiliary = |node: &str| {
        snapshot["prompt"][node]["class_type"]
            .as_str()
            .is_some_and(|class| AUXILIARY_IMAGE_CLASSES.contains(&class))
    };
    auxiliary(node)
        && outputs.as_object().into_iter().flatten().any(|(other, output)| {
            !auxiliary(other)
                && output["images"].as_array().is_some_and(|images| !images.is_empty())
        })
}

/// Output kind recorded for a node class. Unlisted savers stay "unknown" (a real render).
fn output_kind(class: Option<&str>) -> &'static str {
    match class {
        Some("SaveImage") => "saved",
        Some("PreviewImage" | "MaskPreview") => "preview",
        Some("SayaImageGenerationReview") => "review",
        _ => "unknown",
    }
}

/// One line a user can act on, from a job's recorded failure. The three recorded shapes are
/// read: a live execution event, a ComfyUI history status, and a Studio `{"message"}`.
pub(crate) fn failure_summary(error: &Value) -> Option<String> {
    let execution = error["messages"]
        .as_array()
        .and_then(|messages| messages.iter().find(|m| m[0] == "execution_error"))
        .map(|m| &m[1]);
    let source = execution.unwrap_or(error);
    let text = source["exception_message"]
        .as_str()
        .or_else(|| error["message"].as_str())?;
    let first: String = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?
        .chars()
        .take(240)
        .collect();
    let out_of_memory = source["exception_type"]
        .as_str()
        .is_some_and(|kind| kind.contains("OutOfMemory"))
        || text.to_ascii_lowercase().contains("out of memory");
    let mut line = if out_of_memory {
        format!("Mémoire GPU insuffisante (OOM) : {first}")
    } else {
        first
    };
    if let Some(node) = source["node"].as_str().or_else(|| source["node_id"].as_str()) {
        line.push_str(&format!(" (node {node})"));
    }
    Some(line)
}

/// The image that stands for a job: a real render first; a preview or review copy only when the
/// job has nothing else. Output order follows node ids, so "the first one" could be a mask
/// preview sorted before the pass's saved image.
pub(crate) fn representative_asset(outputs: &Value) -> Option<&str> {
    let outputs = outputs.as_array()?;
    outputs
        .iter()
        .filter(|o| !matches!(o["kind"].as_str(), Some("preview" | "review")))
        .find_map(|o| o["assetId"].as_str())
        .or_else(|| outputs.iter().find_map(|o| o["assetId"].as_str()))
}

impl Database {
    /// Latest 100 jobs, for display. Never use it as a lookup table: older jobs are absent.
    pub fn jobs(&self) -> Result<Vec<Value>> {
        self.job_page(None, None, None)
    }
    pub fn character_jobs(&self, character: &str, before: Option<&str>) -> Result<Vec<Value>> {
        self.job_page(Some(character), before, None)
    }
    /// Exact lookup of the given jobs, whatever their age (run and lot synchronization).
    pub fn jobs_by_ids(&self, ids: &[String]) -> Result<Vec<Value>> {
        self.job_page(None, None, Some(serde_json::to_string(ids)?))
    }
    fn job_page(
        &self,
        character: Option<&str>,
        before: Option<&str>,
        ids: Option<String>,
    ) -> Result<Vec<Value>> {
        let limit = if ids.is_some() { "" } else { " LIMIT 100" };
        let mut q=self.connection.prepare(&format!("SELECT j.id,j.state,j.comfy_prompt_id,j.failure_json,j.outputs_json,j.created_at,w.name,json_extract(j.request_snapshot_json,'$.characters') FROM generation_jobs j JOIN workflow_revisions w ON w.id=j.workflow_revision_id WHERE (?1 IS NULL OR EXISTS(SELECT 1 FROM job_characters c WHERE c.job_id=j.id AND c.character_id=?1)) AND (?2 IS NULL OR (j.created_at,j.rowid)<(SELECT created_at,rowid FROM generation_jobs WHERE id=?2)) AND (?3 IS NULL OR j.id IN (SELECT value FROM json_each(?3))) ORDER BY j.created_at DESC,j.rowid DESC{limit}"))?;
        let rows = q.query_map(params![character, before, ids], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, Option<String>>(7)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (id, state, prompt, error, outputs, created, name, characters) = row?;
            result.push(json!({"id":id,"state":state,"promptId":prompt,"error":error.and_then(|s|serde_json::from_str::<Value>(&s).ok()),"outputs":serde_json::from_str::<Value>(&outputs)?,"createdAt":created,"workflow":name,"characters":characters.and_then(|s|serde_json::from_str::<Value>(&s).ok()).unwrap_or(json!([]))}));
        }
        Ok(result)
    }
    pub fn submit_job(
        &self,
        job: &str,
        hash: &str,
        confirmed: bool,
        allow_removals: bool,
        client: &Comfy,
    ) -> Result<Value> {
        if !confirmed {
            return Err("Confirmation explicite requise".into());
        }
        let (snapshot, state): (String, String) = self.connection.query_row(
            "SELECT request_snapshot_json,state FROM generation_jobs WHERE id=?",
            [job],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if state != "draft" || digest(snapshot.as_bytes()) != hash {
            return Err("Brouillon déjà envoyé ou modifié".into());
        }
        let data: Value = serde_json::from_str(&snapshot)?;
        if data["removed"].as_array().is_some_and(|r| !r.is_empty()) && !allow_removals {
            return Err("Autoriser explicitement les suppressions de tags listées".into());
        }
        if self.connection.execute(
            "UPDATE generation_jobs SET state='submitting' WHERE id=? AND state='draft'",
            [job],
        )? != 1
        {
            return Err("Soumission déjà prise en charge".into());
        }
        self.job_event(job, "submission_requested", &json!({"snapshotHash":hash}))?;
        let response = client.submit(json!({"prompt":data["prompt"],"client_id":data["clientId"],"extra_data":{"saya_studio_job_id":job,"extra_pnginfo":{"workflow":data.pointer("/orchestration/uiWorkflow").filter(|v| v.is_object()).cloned().unwrap_or_else(||json!({"nodes":[]}))}}})).and_then(|value| {
            let prompt = value["prompt_id"].as_str().ok_or("Réponse sans prompt_id")?.to_owned();
            uuid::Uuid::parse_str(&prompt)?;
            Ok((value,prompt))
        });
        match response {
            Ok((value, prompt)) => {
                // Reconciliation or a WS terminal event can win while POST waits for its response.
                self.connection.execute("UPDATE generation_jobs SET state=CASE WHEN state IN ('submitting','submission_unknown') THEN 'queued' ELSE state END,comfy_prompt_id=?2,failure_json=CASE WHEN state IN ('submitting','submission_unknown') THEN NULL ELSE failure_json END WHERE id=?1 AND (comfy_prompt_id IS NULL OR comfy_prompt_id=?2)",params![job,prompt])?;
                self.job_event(job, "submitted", &json!({"promptId":prompt}))?;
                Ok(value)
            }
            Err(error) if definitely_not_accepted(error.as_ref()) => {
                // Nothing was queued: an honest failure, replayable at once. Recording it as
                // "unknown" left a job that no sync could ever find, stop or resume.
                let detail = json!({"type":"submission_refused","message":error.to_string()});
                self.connection.execute("UPDATE generation_jobs SET state='failed',failure_json=?2 WHERE id=?1 AND comfy_prompt_id IS NULL AND state IN ('submitting','submission_unknown')",params![job,detail.to_string()])?;
                self.job_event(job, "submission_refused", &detail)?;
                Err(error)
            }
            Err(error) => {
                let detail = json!({"message":error.to_string()});
                self.connection.execute("UPDATE generation_jobs SET state='submission_unknown',failure_json=?2 WHERE id=?1 AND comfy_prompt_id IS NULL AND state IN ('submitting','submission_unknown')",params![job,detail.to_string()])?;
                self.job_event(job, "submission_uncertain", &detail)?;
                Err(error)
            }
        }
    }

    /// Prompt ids the engine holds for this job (queue and given history): same job id, same
    /// client id and the exact graph of the snapshot. Normally zero or one.
    pub(crate) fn engine_candidates(
        &self,
        job: &str,
        queue: &Value,
        history: &Value,
    ) -> Result<std::collections::BTreeSet<String>> {
        let snapshot: String = self.connection.query_row(
            "SELECT request_snapshot_json FROM generation_jobs WHERE id=?",
            [job],
            |r| r.get(0),
        )?;
        let snapshot: Value = serde_json::from_str(&snapshot)?;
        if !history.is_object() {
            return Err("Historique ComfyUI invalide".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        let rows = queue["queue_running"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(queue["queue_pending"].as_array().into_iter().flatten())
            .chain(
                history
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(_, entry)| &entry["prompt"]),
            );
        for row in rows {
            if row[3]["saya_studio_job_id"] == job
                && row[3]["client_id"] == snapshot["clientId"]
                && row[2] == snapshot["prompt"]
            {
                if let Some(id) = row[1].as_str().filter(|s| uuid::Uuid::parse_str(s).is_ok()) {
                    ids.insert(id.to_owned());
                }
            }
        }
        Ok(ids)
    }

    pub(crate) fn recover_prompt(
        &self,
        job: &str,
        queue: &Value,
        history: &Value,
    ) -> Result<Option<String>> {
        let mut ids = self.engine_candidates(job, queue, history)?.into_iter();
        let (Some(id), None) = (ids.next(), ids.next()) else {
            return Ok(None);
        };
        let changed=self.connection.execute("UPDATE generation_jobs SET comfy_prompt_id=?2,failure_json=NULL WHERE id=?1 AND comfy_prompt_id IS NULL AND state IN ('submitting','submission_unknown')",params![job,id])?;
        if changed == 1 {
            self.job_event(
                job,
                "submission_recovered",
                &json!({"promptId":id,"evidence":"job_id_client_id_exact_graph"}),
            )?;
        }
        Ok(Some(id))
    }
    pub fn sync_jobs(&mut self, client: &Comfy, results: &Path) -> Result<()> {
        self.sync_jobs_until(client, results, None)
    }
    pub fn sync_jobs_until(
        &mut self,
        client: &Comfy,
        results: &Path,
        stop: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<()> {
        let stopped = || stop.is_some_and(|s| s.load(std::sync::atomic::Ordering::Acquire));
        let active = {
            let mut q=self.connection.prepare("SELECT id,comfy_prompt_id FROM generation_jobs WHERE state IN ('queued','running','submitting','submission_unknown')")?;
            let r = q
                .query_map([], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            r
        };
        if active.is_empty() {
            return Ok(());
        }
        let queue = client.get("/queue")?;
        queue_contains(&queue, "queue_running", "")?;
        queue_contains(&queue, "queue_pending", "")?;
        let recent = if active.iter().any(|(_, p)| p.is_none()) {
            Some(client.get("/history?max_items=200")?)
        } else {
            None
        };
        let mut failures = Vec::new();
        for (job, prompt) in active {
            if stopped() {
                return Ok(());
            }
            let outcome = (|| -> Result<()> {
                let prompt = match prompt {
                    Some(p) => Some(p),
                    None => self.recover_prompt(
                        &job,
                        &queue,
                        recent.as_ref().ok_or("Historique ComfyUI non chargé")?,
                    )?,
                };
                let Some(prompt) = prompt else {
                    self.connection.execute(
                    "UPDATE generation_jobs SET state='submission_unknown' WHERE id=? AND state IN ('queued','running','submitting','submission_unknown')",
                    [&job],
                )?;
                    return Ok(());
                };
                uuid::Uuid::parse_str(&prompt)?;
                let history = client.get(&format!("/history/{prompt}"))?;
                if !history.is_object() {
                    return Err("Historique ComfyUI invalide".into());
                }
                if let Some(entry) = history.get(&prompt) {
                    if entry.pointer("/status/status_str").and_then(Value::as_str) == Some("error")
                    {
                        self.connection.execute(
                        "UPDATE generation_jobs SET state=?3,failure_json=?2 WHERE id=?1 AND state IN ('queued','running','submitting','submission_unknown')",
                        params![job, entry["status"].to_string(), if entry["status"]["messages"].as_array().is_some_and(|m|m.iter().any(|v|v[0]=="execution_interrupted")) {"interrupted"}else{"failed"}],
                    )?;
                        return Ok(());
                    }
                    if entry.pointer("/status/completed").and_then(Value::as_bool) == Some(true) {
                        std::fs::create_dir_all(results)?;
                        let review_by_node: std::collections::BTreeMap<String, Value> = entry
                            ["outputs"]
                            .as_object()
                            .into_iter()
                            .flatten()
                            .filter_map(|(node, out)| {
                                out.get("saya_review").cloned().map(|v| (node.clone(), v))
                            })
                            .collect();
                        let snapshot: String = self.connection.query_row(
                            "SELECT request_snapshot_json FROM generation_jobs WHERE id=?",
                            [&job],
                            |r| r.get(0),
                        )?;
                        let snapshot: Value = serde_json::from_str(&snapshot)?;
                        let mut descriptors = Vec::new();
                        let mut latent_descriptors = Vec::new();
                        let mut review_entries = Vec::new();
                        for (node, output) in entry["outputs"].as_object().into_iter().flatten() {
                            for (index, image) in output["images"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .enumerate()
                            {
                                if stopped() {
                                    return Ok(());
                                }
                                // One image per render: the review gate's copy and mask previews
                                // are never downloaded when the pass saved its own image. The
                                // review decision data stays on the job.
                                if auxiliary_copy_is_redundant(&snapshot, &entry["outputs"], node) {
                                    let kind = output_kind(snapshot["prompt"][node]["class_type"].as_str());
                                    let mut review = json!({"assetId":Value::Null,"source":image,"nodeId":node,
                                        "outputIndex":index,"kind":kind});
                                    if let Some(data) = review_by_node.get(node) {
                                        review["sayaReview"] = data.clone();
                                    }
                                    review_entries.push(review);
                                    continue;
                                }
                                let name = format!(
                                    "{}-{}-{}.png",
                                    job,
                                    &digest(node.as_bytes())[..12],
                                    index
                                );
                                let path = results.join(&name);
                                // A buffer copy left by an earlier sync is reused only when it
                                // still reads as an image; otherwise it is fetched again.
                                let buffered = path.exists() && {
                                    let mut bytes = Vec::new();
                                    std::fs::File::open(&path)?
                                        .take(MAX_OUTPUT_BYTES + 1)
                                        .read_to_end(&mut bytes)?;
                                    validate_output(&bytes).is_ok()
                                };
                                if !buffered {
                                    let bytes = client.download(image)?;
                                    crate::fsutil::write_atomic(&path, &bytes)?;
                                }
                                descriptors.push((name, image.clone(), node.clone(), index));
                            }
                            for (index, latent) in output["latents"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .enumerate()
                            {
                                if stopped() {
                                    return Ok(());
                                }
                                let source_name = latent["filename"]
                                    .as_str()
                                    .ok_or("Latent sans nom de fichier")?;
                                let ext = Path::new(source_name)
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .unwrap_or("");
                                if ext.is_empty() || !ext.chars().all(|c| c.is_ascii_alphanumeric())
                                {
                                    return Err("Extension de latent invalide".into());
                                }
                                let name = format!(
                                    "{}-{}-{}.{}",
                                    job,
                                    &digest(node.as_bytes())[..12],
                                    index,
                                    ext
                                );
                                let path = results.join(&name);
                                if path.exists() {
                                    validate_latent(&std::fs::read(&path)?)?;
                                } else {
                                    let bytes = client.download_latent(latent)?;
                                    crate::fsutil::write_atomic(&path, &bytes)?;
                                }
                                latent_descriptors.push((
                                    name,
                                    path,
                                    latent.clone(),
                                    node.clone(),
                                    index,
                                ));
                            }
                        }
                        if descriptors.is_empty() && latent_descriptors.is_empty() {
                            self.connection.execute("UPDATE generation_jobs SET state='failed',failure_json=?2 WHERE id=?1 AND state IN ('queued','running','submitting','submission_unknown')",params![job,json!({"message":"Exécution terminée sans sortie image ni latent."}).to_string()])?;
                            return Ok(());
                        }
                        // The app owns the images (brief point 4): the
                        // delivery first joins the organized tree
                        // <root>/<day>/pull/<family>/ or <day>/pull/Final/, then
                        // the historical call indexes the buffer for any
                        // file that an organization error would have
                        // left there — no image is ever lost.
                        let (placements, _used) = if descriptors.is_empty() {
                            (Vec::new(), None)
                        } else {
                            let outcome = self.organize_delivery(&results, &job)?;
                            if let Some((capture, root)) = &outcome.1 {
                                self.index_library(root)?;
                                self.mark_delivery_captures(&outcome.0, capture)?;
                            }
                            self.index_library(results)?;
                            outcome
                        };
                        let mut outputs = Vec::new();
                        let canonical = results.canonicalize()?;
                        let comfy_output = crate::generation_browser::comfy_output_dir();
                        let mut delivered: Vec<(Value, PathBuf)> = Vec::new();
                        for (name, descriptor, node, index) in descriptors {
                            let placement = placements.iter().find(|p| p.buffer_name == *name);
                            // Asset resolution: content duplicate already
                            // in the database, freshly indexed organized capture,
                            // or historical buffer as a last resort.
                            let asset: String = if let Some(existing) =
                                placement.and_then(|p| p.dedup_asset.clone())
                            {
                                existing
                            } else if let Some(destination) =
                                placement.and_then(|p| p.organized.as_ref())
                            {
                                self.connection.query_row(
                                    "SELECT id FROM assets WHERE absolute_path=?1",
                                    [destination.to_string_lossy()],
                                    |r| r.get(0),
                                )?
                            } else {
                                self.connection.query_row("SELECT a.id FROM assets a JOIN libraries l ON l.id=a.library_id WHERE l.root_path=?1 AND a.relative_path=?2",params![canonical.to_str(),name],|r|r.get(0))?
                            };
                            let kind = output_kind(snapshot["prompt"][&node]["class_type"].as_str());
                            if placement.is_some_and(|p| p.error.is_none()) {
                                if let Ok(kept) = self.asset_path(&asset) {
                                    delivered.push((descriptor.clone(), kept));
                                }
                            }
                            let mut entry = json!({"assetId":asset,"source":descriptor,"nodeId":node,"outputIndex":index,"kind":kind});
                            if let Some(review) = review_by_node.get(&node) {
                                entry["sayaReview"] = review.clone();
                            }
                            // An organization failure is reported on the
                            // affected output, never silently killed.
                            if let Some(warning) = placement.and_then(|p| p.error.clone()) {
                                entry["captureWarning"] = json!(warning);
                            }
                            outputs.push(entry);
                        }
                        for (name, path, descriptor, node, index) in latent_descriptors {
                            outputs.push(
                                json!({"assetId":Value::Null,"source":descriptor,"nodeId":node,
                                "outputIndex":index,"kind":"latent","filename":name,
                                "path":path.to_string_lossy()}),
                            );
                        }
                        outputs.extend(review_entries);
                        self.complete_job(&job, &outputs)?;
                        // The library holds the image now: ComfyUI's copy of it can go. A failure
                        // only leaves an extra file, it never fails a finished render.
                        if let Some(output) = comfy_output.as_deref() {
                            for (descriptor, kept) in &delivered {
                                if let Err(error) = self.remove_delivered_comfy_copy(output, descriptor, kept) {
                                    eprintln!("Copie ComfyUI conservée : {error}");
                                }
                            }
                        }
                    }
                } else {
                    let contains = |key: &str| {
                        queue[key].as_array().is_some_and(|list| {
                            list.iter()
                                .any(|item| item.get(1).and_then(Value::as_str) == Some(&prompt))
                        })
                    };
                    let state = if contains("queue_running") {
                        "running"
                    } else if contains("queue_pending") {
                        "queued"
                    } else {
                        "submission_unknown"
                    };
                    self.connection.execute(
                    "UPDATE generation_jobs SET state=?2 WHERE id=?1 AND state IN ('queued','running','submitting','submission_unknown')",
                    params![job, state],
                )?;
                }
                Ok(())
            })();
            if let Err(error) = outcome {
                if error.downcast_ref::<OutputUnavailable>().is_some() {
                    // The engine finished but its image can never be read: the job fails with
                    // the cause. Left active, it was retried at every poll for ever and made
                    // every status call of every run return an error.
                    let detail = json!({"type":"output_unavailable","message":error.to_string()});
                    self.connection.execute("UPDATE generation_jobs SET state='failed',failure_json=?2 WHERE id=?1 AND state IN ('queued','running','submitting','submission_unknown')",params![job,detail.to_string()])?;
                    self.job_event(&job, "output_unavailable", &detail)?;
                    continue;
                }
                let detail = json!({"type":"reconciliation_error","message":error.to_string()});
                self.connection.execute("UPDATE generation_jobs SET failure_json=?2 WHERE id=?1 AND state IN ('queued','running','submitting','submission_unknown')",params![job,detail.to_string()])?;
                failures.push(format!("{job}: {error}"));
            }
        }
        if !failures.is_empty() {
            return Err(failures.join("\n").into());
        }
        Ok(())
    }
}

pub fn describe_engine(stats: &Value, queue: &Value) -> Result<Value> {
    if !stats["system"].is_object() {
        return Err("Réponse system_stats incompatible".into());
    }
    let list = |key: &str| -> Result<Vec<Value>> {
        queue[key]
            .as_array()
            .ok_or("Réponse de file ComfyUI incompatible")?
            .iter()
            .map(|item| {
                let row = item.as_array().ok_or("Entrée de file invalide")?;
                let number = row
                    .first()
                    .filter(|v| v.is_number())
                    .ok_or("Numéro de file absent")?;
                let id = row
                    .get(1)
                    .and_then(Value::as_str)
                    .ok_or("Identifiant de prompt absent")?;
                if id.is_empty() {
                    return Err("Identifiant de prompt vide".into());
                }
                Ok(json!({"number":number,"promptId":id}))
            })
            .collect()
    };
    let running = list("queue_running")?;
    let pending = list("queue_pending")?;
    Ok(
        json!({"connected":true,"version":stats.pointer("/system/comfyui_version"),"running":running.len(),"pending":pending.len(),"queue":{"running":running,"pending":pending},"error":null}),
    )
}
pub fn model_choices(info: &Value) -> Result<Value> {
    let nodes = info.as_object().ok_or("Réponse object_info incompatible")?;
    let mut checkpoints = std::collections::BTreeSet::new();
    let mut loras = std::collections::BTreeSet::new();
    for node in nodes.values() {
        for section in ["required", "optional"] {
            if let Some(inputs) = node["input"][section].as_object() {
                for (name, schema) in inputs {
                    let target = match name.as_str() {
                        "ckpt_name" => &mut checkpoints,
                        "lora_name" => &mut loras,
                        _ => continue,
                    };
                    if let Some(choices) = schema.get(0).and_then(Value::as_array) {
                        for choice in choices {
                            if let Some(name) = choice.as_str() {
                                target.insert(name.to_owned());
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(json!({"checkpoints":checkpoints,"loras":loras,"nodeClasses":nodes.len()}))
}

pub(crate) fn queue_contains(queue: &Value, key: &str, prompt: &str) -> Result<bool> {
    let rows = queue[key].as_array().ok_or("File ComfyUI invalide")?;
    for row in rows {
        if row.get(1).and_then(Value::as_str).is_none() {
            return Err("Entrée ComfyUI invalide".into());
        }
    }
    Ok(rows.iter().any(|row| row[1].as_str() == Some(prompt)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_copy_is_dropped_only_when_the_pass_saved_its_own_image() {
        let snapshot = json!({"prompt":{
            "1":{"class_type":"DaSiWa_MetadataImageSaver"},
            "2":{"class_type":"SayaImageGenerationReview"}}});
        let both = json!({"1":{"images":[{"filename":"a.png"}]},"2":{"images":[{"filename":"b.png"}]}});
        assert!(auxiliary_copy_is_redundant(&snapshot, &both, "2"));
        assert!(!auxiliary_copy_is_redundant(&snapshot, &both, "1"), "the saved image stays");
        let alone = json!({"2":{"images":[{"filename":"b.png"}]}});
        assert!(!auxiliary_copy_is_redundant(&snapshot, &alone, "2"), "the job's only image stays");
        let no_image = json!({"1":{"images":[]},"2":{"images":[{"filename":"b.png"}]}});
        assert!(!auxiliary_copy_is_redundant(&snapshot, &no_image, "2"));
    }

    /// Pass 3 of the Saya preset: the HiDream refiner's `MaskPreview` (id `100055:12425`) sorts
    /// before the pass's saver (`100096`). The mask is not a render: it is neither delivered
    /// nor chosen as the image of the pass.
    #[test]
    fn mask_preview_is_never_the_image_of_a_pass() {
        let snapshot = json!({"prompt":{
            "100055:12425":{"class_type":"MaskPreview"},
            "100096":{"class_type":"DaSiWa_MetadataImageSaver"}}});
        let outputs = json!({"100055:12425":{"images":[{"filename":"mask.png","type":"temp"}]},
            "100096":{"images":[{"filename":"pass3.png"}]}});
        assert!(auxiliary_copy_is_redundant(&snapshot, &outputs, "100055:12425"));
        assert!(!auxiliary_copy_is_redundant(&snapshot, &outputs, "100096"));
        // Two auxiliary images and no render: both stay, nothing is lost.
        let only_aux = json!({"prompt":{"1":{"class_type":"MaskPreview"},"2":{"class_type":"SayaImageGenerationReview"}}});
        let aux_outputs = json!({"1":{"images":[{"filename":"m.png"}]},"2":{"images":[{"filename":"r.png"}]}});
        assert!(!auxiliary_copy_is_redundant(&only_aux, &aux_outputs, "1"));
        assert!(!auxiliary_copy_is_redundant(&only_aux, &aux_outputs, "2"));
        assert_eq!(output_kind(Some("MaskPreview")), "preview");
        assert_eq!(output_kind(Some("DaSiWa_MetadataImageSaver")), "unknown");
    }

    #[test]
    fn failure_summary_reads_the_three_recorded_shapes() {
        // Live event shape, as stored for the owner's pass-3 failures.
        let live = json!({"type":"execution_error","node":"100055:100088",
            "exception_type":"torch.OutOfMemoryError",
            "exception_message":"CUDA out of memory. Tried to allocate 300.00 MiB.\nThis error means you ran out of memory on your GPU."});
        assert_eq!(
            failure_summary(&live).unwrap(),
            "Mémoire GPU insuffisante (OOM) : CUDA out of memory. Tried to allocate 300.00 MiB. (node 100055:100088)"
        );
        // History status shape.
        let history = json!({"status_str":"error","completed":false,"messages":[
            ["execution_start",{}],
            ["execution_error",{"node_id":"7","exception_type":"ValueError","exception_message":"\n  bad mask size\n"}]]});
        assert_eq!(failure_summary(&history).unwrap(), "bad mask size (node 7)");
        // Studio message shape.
        assert_eq!(
            failure_summary(&json!({"message":"Exécution terminée sans sortie image ni latent."})).unwrap(),
            "Exécution terminée sans sortie image ni latent."
        );
        assert_eq!(failure_summary(&Value::Null), None);
        assert_eq!(failure_summary(&json!({"status_str":"error","messages":[]})), None);
    }

    #[test]
    fn only_certain_refusals_are_definite() {
        let offline: Box<dyn std::error::Error + Send + Sync> = Box::new(EngineOffline("x".into()));
        assert!(definitely_not_accepted(offline.as_ref()));
        let http = |code: u16| -> Box<dyn std::error::Error + Send + Sync> {
            Box::new(EngineHttpError {
                status: reqwest::StatusCode::from_u16(code).unwrap(),
                body: String::new(),
            })
        };
        assert!(definitely_not_accepted(http(400).as_ref()));
        assert!(!definitely_not_accepted(http(500).as_ref()), "a server error may follow queuing");
        let other: Box<dyn std::error::Error + Send + Sync> = "timeout".into();
        assert!(!definitely_not_accepted(other.as_ref()));
    }

    #[test]
    fn representative_asset_prefers_a_render_over_previews() {
        // Recorded order follows node ids: the preview comes first.
        let outputs = json!([
            {"assetId":"mask","kind":"preview","nodeId":"100055:12425"},
            {"assetId":null,"kind":"review","nodeId":"100054"},
            {"assetId":"render","kind":"unknown","nodeId":"100096"}]);
        assert_eq!(representative_asset(&outputs), Some("render"));
        let preview_only = json!([{"assetId":"only","kind":"preview"}]);
        assert_eq!(representative_asset(&preview_only), Some("only"), "a job's only image stays");
        assert_eq!(representative_asset(&json!([{"assetId":null,"kind":"latent"}])), None);
        assert_eq!(representative_asset(&json!(null)), None);
    }


    #[test]
    fn closed_engine_reads_as_one_short_sentence() {
        // Port 1: nothing is listening, the connection is refused immediately.
        let comfy = Comfy::loopback(1).unwrap();
        let error = comfy.get("/queue").expect_err("nothing listens on port 1");
        let text = error.to_string();
        assert!(
            text.starts_with("ComfyUI hors ligne : connexion refusée"),
            "connect error must be the short sentence, got: {text}"
        );
        assert!(
            !text.contains("error sending request"),
            "raw reqwest text leaked: {text}"
        );
    }
}
