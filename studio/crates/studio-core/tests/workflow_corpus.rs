//! Synthetic corpus: the check must find the ComfyUI classes a workflow needs and the missing ones.
//! Fixtures: `tests/fixtures/workflow-corpus/` (synthetic object_info, 30 valid UI workflows,
//! 10 workflows with invented node classes, and `expected.json` for the latter).

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use studio_core::workflow::validate_graph;
use studio_core::workflow_check::{check_workflow, missing_classes};
use studio_core::workflow_ui_api::convert_ui;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/workflow-corpus")
}

fn read(name: &str) -> String {
    std::fs::read_to_string(corpus().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn json_file(name: &str) -> Value {
    serde_json::from_str(&read(name)).unwrap()
}

fn names(prefix: &str) -> Vec<String> {
    (1..=10).map(|i| format!("{prefix}-{i:02}")).collect()
}

fn fake_classes() -> Vec<String> {
    serde_json::from_value(json_file("fake-nodes.json")).unwrap()
}

#[test]
fn corpus_has_the_announced_shape() {
    let info = json_file("object_info.json");
    assert!(info.as_object().unwrap().len() >= 40);
    for class in [
        "KSampler",
        "SaveImage",
        "LoraLoader",
        "ControlNetApply",
        "FreeU_V2",
    ] {
        assert!(info.get(class).is_some(), "{class}");
    }
    // Model choices are neutral synthetic names only.
    let text = info.to_string();
    assert!(text.contains("example-sdxl.safetensors"));
    let fakes = fake_classes();
    assert_eq!(fakes.len(), 30);
    for class in &fakes {
        assert!(
            info.get(class).is_none(),
            "fake class exists in object_info: {class:?}"
        );
    }
    assert!(fakes.iter().any(|c| c.len() > 200), "very long name");
    assert!(fakes.iter().any(|c| !c.is_ascii()), "unicode name");
    assert!(fakes.iter().any(|c| c.contains(' ')), "name with spaces");
    assert!(
        fakes.iter().any(|c| c != c.trim() || c.contains('\u{a0}')),
        "trailing space"
    );
    assert!(
        fakes.iter().any(|c| c == "KSampler ") && fakes.iter().any(|c| c == "ksampler"),
        "near-miss of a real class"
    );
}

#[test]
fn every_valid_workflow_is_complete() {
    let info = json_file("object_info.json");
    for name in names("simple")
        .into_iter()
        .chain(names("medium"))
        .chain(names("complex"))
    {
        let report = check_workflow(&read(&format!("{name}.json")), &info).unwrap();
        assert_eq!(report["complete"], true, "{name}: {report}");
        assert_eq!(report["format"], "ui", "{name}");
        assert!(report["missing"].as_array().unwrap().is_empty(), "{name}");
        assert!(
            report["unresolved"].as_array().unwrap().is_empty(),
            "{name}"
        );
        assert!(
            report["nodeCount"].as_u64().unwrap() >= 7,
            "{name}: {report}"
        );
        for class in report["classes"].as_array().unwrap() {
            assert!(
                info.get(class.as_str().unwrap()).is_some(),
                "{name}: {class}"
            );
        }
    }
}

#[test]
fn the_converted_api_graph_of_valid_workflows_passes_validation_and_is_complete() {
    let info = json_file("object_info.json");
    for name in names("simple")
        .into_iter()
        .chain(names("medium"))
        .chain(names("complex"))
    {
        let api = convert_ui(&read(&format!("{name}.json")), Some(&info)).unwrap()["api"].clone();
        validate_graph(&api, &info).unwrap_or_else(|e| panic!("{name}: {e}"));
        let report = check_workflow(&api.to_string(), &info).unwrap();
        assert_eq!(report["format"], "api", "{name}");
        assert_eq!(report["complete"], true, "{name}: {report}");
    }
}

#[test]
fn fake_nodes_are_reported_exactly() {
    let info = json_file("object_info.json");
    let expected = json_file("expected.json");
    assert_eq!(expected.as_object().unwrap().len(), 10);
    let mut with_nested = 0;
    for name in names("fakes") {
        let want = &expected[&name];
        let report = check_workflow(&read(&format!("{name}.json")), &info).unwrap();
        let mut classes: Vec<String> = serde_json::from_value(want["missing"].clone()).unwrap();
        classes.sort();
        assert_eq!(missing_classes(&report), classes, "{name}: {report}");
        // Every missing node is located by its execution id.
        for entry in report["missing"].as_array().unwrap() {
            let class = entry["class"].as_str().unwrap();
            let mut nodes: Vec<String> = serde_json::from_value(entry["nodes"].clone()).unwrap();
            nodes.sort();
            let mut want_nodes: Vec<String> =
                serde_json::from_value(want["nodes"][class].clone()).unwrap();
            want_nodes.sort();
            assert_eq!(nodes, want_nodes, "{name}: {class:?}");
            with_nested += nodes.iter().filter(|n| n.contains(':')).count();
        }
        // Complete if and only if nothing is missing (muted/bypassed fakes do not count).
        assert_eq!(report["complete"], classes.is_empty(), "{name}: {report}");
        // Missing classes are also part of the used classes.
        for class in &classes {
            assert!(
                report["classes"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(class)),
                "{name}"
            );
        }
    }
    assert!(
        with_nested >= 6,
        "nested subgraph fakes must be located by path ({with_nested})"
    );
}

#[test]
fn muted_and_bypassed_fakes_are_not_required() {
    let info = json_file("object_info.json");
    let expected = json_file("expected.json");
    // fakes-05: one muted fake; fakes-06: one bypassed fake inside the chain.
    for name in ["fakes-05", "fakes-06"] {
        assert!(expected[name]["missing"].as_array().unwrap().is_empty());
        let report = check_workflow(&read(&format!("{name}.json")), &info).unwrap();
        assert_eq!(report["complete"], true, "{name}: {report}");
    }
}

#[test]
fn each_fake_name_alone_is_missing() {
    let info = json_file("object_info.json");
    for class in fake_classes() {
        let ui = json!({"nodes": [{"id": 1, "type": class, "mode": 0, "inputs": [], "outputs": [], "widgets_values": []}], "links": []});
        let report = check_workflow(&ui.to_string(), &info).unwrap();
        assert_eq!(missing_classes(&report), vec![class.clone()], "{class:?}");
        assert_eq!(report["missing"][0]["nodes"], json!(["1"]));
        assert_eq!(report["complete"], false);

        let api = json!({"1": {"class_type": class, "inputs": {}}});
        let report = check_workflow(&api.to_string(), &info).unwrap();
        assert_eq!(report["format"], "api");
        assert_eq!(missing_classes(&report), vec![class.clone()], "{class:?}");
    }
}

#[test]
fn api_graph_with_a_fake_class_is_refused_by_validate_graph() {
    let info = json_file("object_info.json");
    let converted = convert_ui(&read("simple-01.json"), Some(&info)).unwrap()["api"].clone();
    validate_graph(&converted, &info).unwrap();
    let fakes = fake_classes();
    let sampler = converted
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, n)| n["class_type"] == "KSampler")
        .map(|(id, _)| id.clone())
        .unwrap();
    for class in &fakes {
        let mut api = converted.clone();
        api[&sampler]["class_type"] = json!(class);
        let error = validate_graph(&api, &info).unwrap_err().to_string();
        assert!(error.contains("absente"), "{class:?}: {error}");
        let report = check_workflow(&api.to_string(), &info).unwrap();
        assert_eq!(missing_classes(&report), vec![class.clone()]);
        assert_eq!(report["missing"][0]["nodes"], json!([sampler]));
        assert_eq!(report["complete"], false);
    }
}

#[test]
fn api_graph_with_an_invalid_input_is_not_complete() {
    let info = json_file("object_info.json");
    let mut api = convert_ui(&read("simple-01.json"), Some(&info)).unwrap()["api"].clone();
    let ks = api
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, n)| n["class_type"] == "KSampler")
        .map(|(i, _)| i.clone())
        .unwrap();
    api[&ks]["inputs"].as_object_mut().unwrap().remove("model");
    let report = check_workflow(&api.to_string(), &info).unwrap();
    assert!(report["missing"].as_array().unwrap().is_empty());
    assert_eq!(report["complete"], false);
    assert_eq!(report["unresolved"].as_array().unwrap().len(), 1);
}

#[test]
fn not_a_workflow_is_an_error() {
    let info = json_file("object_info.json");
    assert!(check_workflow("not json", &info).is_err());
    assert!(check_workflow("{\"hello\": 1}", &info).is_err());
}

// ---- CLI ----

fn cli(args: &[&Path]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_studio-maintenance"))
        .arg("check-workflow")
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
    )
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("workflow-corpus-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn cli_exit_codes() {
    let info = corpus().join("object_info.json");

    let (code, out) = cli(&[&corpus().join("simple-01.json"), &info]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        serde_json::from_str::<Value>(&out).unwrap()["complete"],
        true
    );

    let (code, out) = cli(&[&corpus().join("fakes-01.json"), &info]);
    assert_eq!(code, 2, "{out}");
    assert_eq!(
        serde_json::from_str::<Value>(&out).unwrap()["missing"][0]["class"],
        "QuantumNoodleSampler"
    );

    // Muted / bypassed fakes only: complete.
    assert_eq!(cli(&[&corpus().join("fakes-05.json"), &info]).0, 0);

    // Whole fixture folder (other JSON files are skipped): fakes-NN make it exit 2.
    let (code, out) = cli(&[&corpus(), &info]);
    assert_eq!(code, 2);
    let reports: Vec<Value> = serde_json::from_str(&out).unwrap();
    let checked = reports
        .iter()
        .filter(|r| r.get("complete").is_some())
        .count();
    assert_eq!(checked, 40);
    assert!(reports
        .iter()
        .any(|r| r["file"] == "expected.json" && r["skipped"] == "not a workflow"));

    // Folder with valid workflows only: exit 0.
    let dir = scratch("ok");
    for name in ["simple-01", "medium-03", "complex-02"] {
        std::fs::copy(
            corpus().join(format!("{name}.json")),
            dir.join(format!("{name}.json")),
        )
        .unwrap();
    }
    std::fs::write(dir.join("notes.json"), "{\"note\": true}").unwrap();
    let (code, out) = cli(&[&dir, &info]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(serde_json::from_str::<Vec<Value>>(&out).unwrap().len(), 4);

    // Errors: unreadable file, invalid JSON, invalid JSON inside a folder, bad usage.
    assert_eq!(cli(&[&corpus().join("does-not-exist.json"), &info]).0, 1);
    let bad = dir.join("broken.json");
    std::fs::write(&bad, "{ not json").unwrap();
    assert_eq!(cli(&[&bad, &info]).0, 1);
    assert_eq!(cli(&[&dir, &info]).0, 1);
    assert_eq!(
        cli(&[
            &corpus().join("simple-01.json"),
            &dir.join("missing-info.json")
        ])
        .0,
        1
    );
    assert_eq!(cli(&[&corpus().join("simple-01.json")]).0, 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Regression: a frontend « Primitive » feeding a converted widget (here the KSampler seed) was
/// reported as a missing class, so ordinary ComfyUI workflows were refused. It is a virtual node:
/// its value reaches the target input and the node itself is never sent.
#[test]
fn primitive_node_value_reaches_its_target_and_is_not_sent() {
    let mut workflow: Value = serde_json::from_str(&read("simple-01.json")).unwrap();
    let nodes = workflow["nodes"].as_array_mut().unwrap();
    let sampler = nodes.iter().position(|n| n["type"] == "KSampler").unwrap();
    let primitive_id = nodes.iter().filter_map(|n| n["id"].as_u64()).max().unwrap() + 1;
    let link_id = workflow["links"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l[0].as_u64())
        .max()
        .unwrap_or(0)
        + 1;
    let nodes = workflow["nodes"].as_array_mut().unwrap();
    let sampler_id = nodes[sampler]["id"].clone();
    let inputs = nodes[sampler]["inputs"].as_array_mut().unwrap();
    inputs.push(json!({"name":"seed","type":"INT","link":link_id,"widget":{"name":"seed"}}));
    let slot = inputs.len() - 1;
    nodes.push(json!({"id":primitive_id,"type":"PrimitiveNode","mode":0,"inputs":[],
        "outputs":[{"name":"INT","type":"INT","links":[link_id],"widget":{"name":"seed"}}],
        "widgets_values":[424242,"fixed"]}));
    workflow["links"]
        .as_array_mut()
        .unwrap()
        .push(json!([link_id, primitive_id, 0, sampler_id, slot, "INT"]));
    let info = json_file("object_info.json");
    let source = workflow.to_string();
    let report = check_workflow(&source, &info).unwrap();
    assert_eq!(report["complete"], true, "{report}");
    let converted = convert_ui(&source, Some(&info)).unwrap();
    let api = converted["api"].as_object().unwrap();
    assert!(api.values().all(|n| n["class_type"] != "PrimitiveNode"), "virtual node never sent");
    let seed = api.values().find(|n| n["class_type"] == "KSampler").unwrap()["inputs"]["seed"].clone();
    assert_eq!(seed, json!(424242), "the primitive value wins over the saved widget");
}

/// Notes and rgthree canvas helpers (bookmarks, labels, bypass/mute toggles) only exist in the
/// ComfyUI frontend: a workflow using them stays complete and none of them is sent. Regression:
/// a single « Note » made every workflow incomplete.
#[test]
fn frontend_only_nodes_are_not_required() {
    let mut workflow: Value = serde_json::from_str(&read("simple-01.json")).unwrap();
    let next = workflow["nodes"].as_array().unwrap().iter().filter_map(|n| n["id"].as_u64()).max().unwrap() + 1;
    let helpers = [
        "Note",
        "MarkdownNote",
        "Bookmark (rgthree)",
        "Label (rgthree)",
        "Fast Bypasser (rgthree)",
        "Fast Muter (rgthree)",
        "Fast Groups Muter (rgthree)",
        "Fast Actions Button (rgthree)",
    ];
    for (offset, class) in helpers.iter().enumerate() {
        workflow["nodes"].as_array_mut().unwrap().push(json!({
            "id": next + offset as u64, "type": class, "mode": 0, "inputs": [], "outputs": [], "widgets_values": []
        }));
    }
    let info = json_file("object_info.json");
    let source = workflow.to_string();
    let report = check_workflow(&source, &info).unwrap();
    assert_eq!(report["complete"], true, "{report}");
    let api = convert_ui(&source, Some(&info)).unwrap()["api"].clone();
    for class in helpers {
        assert!(api.as_object().unwrap().values().all(|n| n["class_type"] != class), "{class} sent");
    }
}
