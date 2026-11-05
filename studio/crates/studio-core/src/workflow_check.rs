//! Which ComfyUI classes does a workflow need, and which of them are missing?
//!
//! The check is built on top of [`workflow_ui_api::convert_ui`] (UI format saved by the
//! ComfyUI frontend, subgraphs included) and on [`workflow::validate_graph`] (API format),
//! compared with the `/object_info` of the running ComfyUI. Nothing is reimplemented here:
//! the converter decides which nodes are executable.
//!
//! Inactive nodes are not executed, so their class is not required: a muted (mode 2) or
//! bypassed (mode 4) node is skipped by the converter and never reported as missing.

use crate::{workflow, workflow_ui_api, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Cause text the converter attaches to a node whose class is absent from `object_info`.
const MISSING_CAUSE: &str = "classe absente d'object_info";

/// True for an API-format prompt: no `nodes` array, only objects carrying a `class_type`.
fn is_api_graph(value: &Value) -> bool {
    value.as_object().is_some_and(|nodes| {
        !nodes.is_empty()
            && value.get("nodes").is_none()
            && nodes
                .values()
                .all(|node| node.get("class_type").is_some_and(Value::is_string))
    })
}

fn node_list(by_class: BTreeMap<String, BTreeSet<String>>) -> Vec<Value> {
    by_class
        .into_iter()
        .map(|(class, nodes)| json!({"class": class, "nodes": nodes}))
        .collect()
}

fn check_api(graph: &Value, info: &Value) -> Value {
    let mut classes = BTreeSet::new();
    let mut missing: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let nodes = graph.as_object().cloned().unwrap_or_default();
    for (id, node) in &nodes {
        let class = node["class_type"].as_str().unwrap_or_default();
        classes.insert(class.to_owned());
        if info.get(class).is_none() {
            missing
                .entry(class.to_owned())
                .or_default()
                .insert(id.clone());
        }
    }
    let mut unresolved = Vec::new();
    // With every class known, the full validation (inputs, links, choices) is meaningful.
    if missing.is_empty() {
        if let Err(error) = workflow::validate_graph(graph, info) {
            unresolved
                .push(json!({"executionId": "", "classType": "", "cause": error.to_string()}));
        }
    }
    json!({
        "complete": missing.is_empty() && unresolved.is_empty(),
        "format": "api",
        "nodeCount": nodes.len(),
        "classes": classes,
        "missing": node_list(missing),
        "unresolved": unresolved,
    })
}

fn check_ui(source: &str, info: &Value) -> Result<Value> {
    let converted = workflow_ui_api::convert_ui(source, Some(info))?;
    let api_nodes = converted["api"].as_object().cloned().unwrap_or_default();
    let mut classes: BTreeSet<String> = api_nodes
        .values()
        .filter_map(|node| node["class_type"].as_str().map(str::to_owned))
        .collect();
    let mut missing: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut unresolved = Vec::new();
    for entry in converted["report"]["unresolved"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let class = entry["classType"].as_str().unwrap_or_default();
        if entry["cause"].as_str() == Some(MISSING_CAUSE) {
            classes.insert(class.to_owned());
            missing
                .entry(class.to_owned())
                .or_default()
                .insert(entry["executionId"].as_str().unwrap_or_default().to_owned());
        } else {
            unresolved.push(entry.clone());
        }
    }
    let missing_nodes: usize = missing.values().map(BTreeSet::len).sum();
    Ok(json!({
        "complete": missing.is_empty() && unresolved.is_empty(),
        "format": "ui",
        "nodeCount": api_nodes.len() + missing_nodes,
        "classes": classes,
        "missing": node_list(missing),
        "unresolved": unresolved,
    }))
}

/// Reports the classes a workflow needs and the ones `object_info` does not provide.
///
/// `ui_source` is the JSON text of a UI workflow or of an API prompt (detected from its
/// shape). The result is
/// `{complete, format, nodeCount, classes, missing:[{class, nodes}], unresolved}`:
/// `nodes` are execution ids (`"12"` at top level, `"5:3"` inside the subgraph instance 5),
/// `classes` are the classes of the executed nodes plus the missing ones, `nodeCount`
/// counts executed nodes (muted, bypassed and purely visual nodes excluded) and
/// `unresolved` carries the converter report entries that are not a missing class.
pub fn check_workflow(ui_source: &str, object_info: &Value) -> Result<Value> {
    let parsed: Value = serde_json::from_str(ui_source)?;
    if is_api_graph(&parsed) {
        return Ok(check_api(&parsed, object_info));
    }
    check_ui(ui_source, object_info)
}

/// Missing class names of a report produced by [`check_workflow`], sorted.
pub fn missing_classes(report: &Value) -> Vec<String> {
    let mut out: Vec<String> = report["missing"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["class"].as_str().map(str::to_owned))
        .collect();
    out.sort();
    out
}
