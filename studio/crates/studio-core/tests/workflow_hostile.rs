//! Crafted workflows end in a precise refusal: never a crash of the app, never a false "complete".
use serde_json::{json, Value};
use studio_core::workflow_ui_api::convert_ui;

const A: &str = "aaaaaaaa-0000-4000-8000-000000000001";

fn chain(length: u64) -> Value {
    let mut nodes: Vec<Value> = (1..=length)
        .map(|i| json!({"id":i,"type":"Reroute","mode":0,
            "inputs":[{"name":"","type":"*","link":if i > 1 { Some(i - 1) } else { None }}],
            "outputs":[{"name":"","type":"*","links":[i]}]}))
        .collect();
    let mut links: Vec<Value> = (1..length).map(|i| json!([i, i, 0, i + 1, 0, "*"])).collect();
    nodes.insert(0, json!({"id":0,"type":"EmptyImage","mode":0,"inputs":[],"outputs":[{"name":"IMAGE","type":"IMAGE","links":[1000000]}],"widgets_values":[]}));
    nodes[1]["inputs"][0]["link"] = json!(1000000);
    links.push(json!([1000000, 0, 0, 1, 0, "IMAGE"]));
    nodes.push(json!({"id":length+1,"type":"PreviewImage","mode":0,"inputs":[{"name":"images","type":"IMAGE","link":length}],"outputs":[]}));
    links.push(json!([length, length, 0, length + 1, 0, "*"]));
    json!({"nodes":nodes,"links":links})
}

#[test]
fn a_very_long_chain_is_reported_instead_of_overflowing_the_stack() {
    for length in [5_000, 200_000] {
        let converted = convert_ui(&chain(length).to_string(), None).unwrap();
        assert_eq!(converted["report"]["complete"], false, "{length}");
        assert!(converted["report"]["unresolved"].to_string().contains("trop longue"), "{}", converted["report"]);
    }
}

#[test]
fn an_ordinary_chain_still_converts() {
    let converted = convert_ui(&chain(100).to_string(), None).unwrap();
    assert_eq!(converted["report"]["complete"], true, "{}", converted["report"]);
    let preview = converted["api"].as_object().unwrap().values().find(|n| n["class_type"] == "PreviewImage").unwrap();
    assert_eq!(preview["inputs"]["images"][0], "0", "the chain resolves to its source");
}

#[test]
fn deeply_nested_subgraphs_are_refused() {
    let id = |i: u64| format!("aaaaaaaa-0000-4000-8000-{i:012}");
    let subgraphs: Vec<Value> = (0..200u64)
        .map(|i| json!({"id":id(i),"name":"n","nodes":[{"id":1,"type":id(i + 1),"mode":0,"inputs":[],"outputs":[]}],"links":[],"inputs":[],"outputs":[]}))
        .collect();
    let doc = json!({"nodes":[{"id":1,"type":id(0),"mode":0,"inputs":[],"outputs":[]}],"links":[],"definitions":{"subgraphs":subgraphs}});
    let error = convert_ui(&doc.to_string(), None).unwrap_err().to_string();
    assert!(error.contains("trop imbriqués"), "{error}");
}

#[test]
fn unreadable_documents_are_refused_not_reported_complete() {
    for doc in [
        json!({"nodes":[1,"x",null,[]]}),
        json!({"nodes":[{"id":null,"type":"X"}],"links":[]}),
        json!({"nodes":[{"id":1,"type":5}],"links":[]}),
        json!({"nodes":[{"id":1}],"links":[]}),
        json!({"nodes":[{"id":1,"type":"X"}],"links":[[1],5]}),
        json!({"nodes":[{"id":1,"type":A}],"links":[],"definitions":{"subgraphs":[{"id":A,"nodes":[{"bad":true}]}]}}),
    ] {
        assert!(convert_ui(&doc.to_string(), None).is_err(), "{doc}");
    }
}
