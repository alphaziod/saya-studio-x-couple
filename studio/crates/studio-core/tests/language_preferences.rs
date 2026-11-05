use serde_json::json;
use studio_core::Database;

#[test]
fn language_preferences_validate_and_survive_reopening() {
    let root = std::env::temp_dir().join(format!("saya-language-{}",uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let path=root.join("studio.sqlite3");
    let db=Database::open(&path).unwrap();
    for language in ["fr","en","ja","ko","zh-CN","zh-TW","pt-BR","es","ru","tr","ar","vi","th","id"] {
        db.set_preference("language",json!(language)).unwrap();
        assert_eq!(db.preferences().unwrap()["language"],language);
    }
    db.set_preference("language",json!("fr")).unwrap();
    db.set_preference("language_setup_completed",json!(true)).unwrap();
    for invalid in [json!("xx"),json!(null),json!(7),json!(true)] {
        assert!(db.set_preference("language",invalid).is_err());
    }
    assert!(db.set_preference("language_setup_completed",json!("true")).is_err());
    assert!(db.set_preference("probe_test",json!("ok")).is_err());
    // Developer mode is a plain on/off switch.
    db.set_preference("devMode",json!(true)).unwrap();
    assert_eq!(db.preferences().unwrap()["devMode"],true);
    assert!(db.set_preference("devMode",json!("on")).is_err());
    // Update check at startup: on/off switch, absent = on.
    assert!(db.preferences().unwrap().get("updateCheck").is_none());
    db.set_preference("updateCheck",json!(false)).unwrap();
    assert_eq!(db.preferences().unwrap()["updateCheck"],false);
    assert!(db.set_preference("updateCheck",json!("off")).is_err());
    drop(db);
    let db=Database::open(&path).unwrap();
    assert_eq!(db.preferences().unwrap()["language"],"fr");
    assert_eq!(db.preferences().unwrap()["language_setup_completed"],true);
    drop(db);std::fs::remove_dir_all(root).unwrap();
}
