use std::path::Path;
use serde_json::{json, Value};
use studio_core::{backup, gallery::Filters, workflow_check, Database, Result};

/// Exit codes of `check-workflow`: 0 complete, 2 missing nodes (or otherwise incomplete), 1 error.
fn check_workflows(target: &Path, object_info: &Path) -> Result<i32> {
    let info: Value = serde_json::from_str(&std::fs::read_to_string(object_info)?)?;
    if !target.is_dir() {
        let report = workflow_check::check_workflow(&std::fs::read_to_string(target)?, &info)?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(if report["complete"] == true { 0 } else { 2 });
    }
    let mut files: Vec<_> = std::fs::read_dir(target)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.is_file() && path.extension().is_some_and(|e| e == "json"))
        .filter(|path| path != object_info)
        .collect();
    files.sort();
    let (mut incomplete, mut failed) = (false, false);
    let mut reports = Vec::new();
    for path in files {
        let file = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&path)?;
        // A folder may hold other JSON files (notes, fixtures): only workflows are checked.
        match serde_json::from_str::<Value>(&text) {
            Ok(v) if v.get("nodes").is_some() || v.as_object().is_some_and(|o| !o.is_empty() && o.values().all(|n| n.get("class_type").is_some())) => {}
            Ok(_) => {
                reports.push(json!({"file": file, "skipped": "not a workflow"}));
                continue;
            }
            Err(error) => {
                failed = true;
                reports.push(json!({"file": file, "error": error.to_string()}));
                continue;
            }
        }
        match workflow_check::check_workflow(&text, &info) {
            Ok(mut report) => {
                incomplete |= report["complete"] != true;
                report["file"] = json!(file);
                reports.push(report);
            }
            Err(error) => {
                failed = true;
                reports.push(json!({"file": file, "error": error.to_string()}));
            }
        }
    }
    println!("{}", serde_json::to_string_pretty(&reports)?);
    Ok(if failed { 1 } else if incomplete { 2 } else { 0 })
}
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let usage="Usage: studio-maintenance status DATABASE | index DATABASE LIBRARY | backup DATABASE NEW_DIRECTORY [--include-pass-states] | verify BACKUP_DIRECTORY | restore BACKUP_DIRECTORY NEW_DIRECTORY | cleanup-duplicates DATABASE | check-workflow WORKFLOW_OR_DIR OBJECT_INFO.json (exit 0 complete, 2 missing nodes, 1 error)";
    let action = args.get(1).ok_or(usage)?;
    let source = Path::new(args.get(2).ok_or(usage)?);
    match action.as_str() {
        "check-workflow" => {
            if args.len() != 4 {
                return Err(usage.into());
            }
            match check_workflows(source, Path::new(&args[3])) {
                Ok(0) => {}
                Ok(code) => std::process::exit(code),
                Err(error) => {
                    eprintln!("check-workflow: {error}");
                    std::process::exit(1);
                }
            }
        }
        "cleanup-duplicates" => {
            let mut db = Database::open(source)?;
            println!("{}", db.cleanup_exact_duplicates()?);
        }
        "backup" | "restore" => {
            let include_states =
                action == "backup" && args.len() == 5 && args[4] == "--include-pass-states";
            if args.len() != 4 && !include_states {
                return Err(usage.into());
            }
            let destination = Path::new(&args[3]);
            let result = if action == "backup" {
                backup::create_with_options(source, destination, include_states)?
            } else {
                backup::restore(source, destination)?
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        "verify" => {
            if args.len() != 3 {
                return Err(usage.into());
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&backup::verify(source)?)?
            );
        }
        "status" | "index" => {
            if args.len() != if action == "status" { 3 } else { 4 } {
                return Err(usage.into());
            }
            let mut db = Database::open(source)?;
            if action == "status" {
                println!("{}", serde_json::to_string_pretty(&db.status()?)?);
                println!("{}", serde_json::to_string(&db.facets()?)?);
                println!("{}", serde_json::to_string(&db.latest_index_run()?)?);
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&db.index_library_progress(
                        Path::new(&args[3]),
                        |done, total| {
                            if done % 100 == 0 || done == total {
                                eprintln!("Indexation {done}/{total}");
                            }
                        }
                    )?)?
                );
                println!("Total images: {}", db.gallery(&Filters::default())?.total);
            }
        }
        _ => return Err(usage.into()),
    }
    Ok(())
}
