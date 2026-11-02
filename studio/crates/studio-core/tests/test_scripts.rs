use std::{fs, path::PathBuf, time::Duration};
use studio_core::engine_runtime::{list_test_scripts, run_test_script};
static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("saya-test-scripts-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("GROK BUILD/generez")).unwrap();
        fs::create_dir_all(root.join("scripts-tests")).unwrap();
        Self(root)
    }
    fn engine(&self) -> PathBuf {
        self.0.join("GROK BUILD/generez")
    }
    fn scripts(&self) -> PathBuf {
        self.0.join("scripts-tests")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn discovers_existing_and_new_tests_without_execution() {
    let f = Fixture::new();
    fs::write(
        f.scripts().join("test_portrait.py"),
        "raise Exception('listing must not execute')",
    )
    .unwrap();
    fs::write(
        f.scripts().join("production.py"),
        "raise Exception('not a test')",
    )
    .unwrap();
    fs::write(f.scripts().join("test_notes.txt"), "").unwrap();
    fs::create_dir(f.scripts().join("test_folder.py")).unwrap();
    let root = f.engine().parent().unwrap().to_path_buf();
    for dir in [
        "solo_x7/attente_solo",
        "agent-new",
        "tests",
        "_archives",
        "data",
    ] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    for file in [
        "solo_x7/test_bny20.py",
        "solo_x7/attente_solo/tester.py",
        "agent-new/test_another.py",
        "tests/test_unit.py",
        "_archives/test_old.py",
        "data/test_copy.py",
        "generez/test_solo.py",
    ] {
        fs::write(
            root.join(file),
            "raise Exception('listing must not execute')",
        )
        .unwrap();
    }
    let scripts = list_test_scripts(&f.engine(), &f.scripts()).unwrap();
    assert_eq!(
        scripts.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        vec![
            "engine/agent-new/test_another.py",
            "engine/solo_x7/attente_solo/tester.py",
            "engine/solo_x7/test_bny20.py",
            "local/test_portrait.py"
        ]
    );
    fs::write(root.join("agent-new/test_new.py"), "").unwrap();
    assert_eq!(
        list_test_scripts(&f.engine(), &f.scripts()).unwrap().len(),
        5
    );
    assert_eq!(
        list_test_scripts(&f.engine(), &f.0.join("absent"))
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn runs_selected_test_with_its_flag_and_reports_real_failure() {
    let _guard = TEST_LOCK.lock().unwrap();
    let f = Fixture::new();
    fs::write(f.scripts().join("test_demo.py"),"import sys,os\nassert '--gpu' in sys.argv\nprint(sys.argv[1])\nprint(os.getcwd())\nprint(os.environ['PYTHONHASHSEED'])\nsys.exit(7)\n").unwrap();
    let result = run_test_script(
        &f.engine(),
        &f.scripts(),
        "local/test_demo.py",
        Duration::from_secs(5),
    )
    .unwrap();
    assert!(!result.success);
    assert_eq!(result.code, Some(7));
    assert!(result.stdout.contains("--gpu"));
    assert!(result.stdout.contains(f.scripts().to_str().unwrap()));
    assert!(result.stdout.ends_with("0\n"));
    assert_eq!(result.argv.len(), 3);
    fs::write(
        f.scripts().join("test_no_flag.py"),
        "import sys\nassert len(sys.argv)==1\nprint('ok')\n",
    )
    .unwrap();
    let result = run_test_script(
        &f.engine(),
        &f.scripts(),
        "local/test_no_flag.py",
        Duration::from_secs(5),
    )
    .unwrap();
    assert!(result.success);
    assert_eq!(result.argv.len(), 2);
    for id in [
        "../test_demo.py",
        "/tmp/test_demo.py",
        "production.py",
        "duo",
        "lcs",
        "local/../test_demo.py",
        "local/test_demo.py;echo",
    ] {
        assert!(
            run_test_script(&f.engine(), &f.scripts(), id, Duration::from_secs(5)).is_err(),
            "{id}"
        );
    }
}

#[test]
fn refuses_removed_tests_and_symlinks() {
    let _guard = TEST_LOCK.lock().unwrap();
    let f = Fixture::new();
    let target = f.scripts().join("test_removed.py");
    fs::write(&target, "").unwrap();
    assert_eq!(
        list_test_scripts(&f.engine(), &f.scripts()).unwrap().len(),
        1
    );
    fs::remove_file(target).unwrap();
    assert!(run_test_script(
        &f.engine(),
        &f.scripts(),
        "local/test_removed.py",
        Duration::from_secs(5)
    )
    .is_err());
    #[cfg(unix)]
    {
        fs::write(f.0.join("outside.py"), "print('must not execute')").unwrap();
        std::os::unix::fs::symlink(f.0.join("outside.py"), f.scripts().join("test_link.py"))
            .unwrap();
        assert!(list_test_scripts(&f.engine(), &f.scripts())
            .unwrap()
            .is_empty());
        assert!(run_test_script(
            &f.engine(),
            &f.scripts(),
            "local/test_link.py",
            Duration::from_secs(5)
        )
        .is_err());
    }
}
