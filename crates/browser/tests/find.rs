use browser::{find, FindError, Places};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn scratch(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("browser-find").join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn program(path: &Path) -> PathBuf {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_path_buf()
}

#[test]
fn t040_c3_the_named_browser_is_used_and_never_replaced() {
    let root = scratch("named");
    let named = program(&root.join("my-browser"));
    let other = program(&root.join("bin/chromium"));
    let places = Places {
        named: Some(named.clone().into()),
        path: vec![other.parent().unwrap().to_path_buf()],
        ..Places::default()
    };
    assert_eq!(find(&places), Ok(named));

    // A named browser that is missing is an error even though another one exists.
    let missing = root.join("gone");
    let places = Places {
        named: Some(missing.clone().into()),
        ..places
    };
    assert_eq!(
        find(&places),
        Err(FindError::Unusable {
            path: missing,
            reason: "it does not exist".to_string()
        })
    );

    let plain = root.join("notes.txt");
    std::fs::write(&plain, "x").unwrap();
    let places = Places {
        named: Some(plain.clone().into()),
        ..places
    };
    assert!(matches!(find(&places), Err(FindError::Unusable { reason, .. }) if reason.contains("cannot") || reason.contains("not marked")));
    let places = Places {
        named: Some(root.clone().into()),
        ..places
    };
    assert!(matches!(find(&places), Err(FindError::Unusable { reason, .. }) if reason == "it is not a file"));
}

#[test]
fn t040_c2_the_search_order_is_applications_then_the_path_then_the_caches() {
    let root = scratch("order");
    let app = program(&root.join("Applications/Chromium.app/Contents/MacOS/Chromium"));
    let later_app = program(&root.join("Applications/Vivaldi.app/Contents/MacOS/Vivaldi"));
    let on_path = program(&root.join("bin/google-chrome"));
    let cached = program(&root.join("cache/chrome/mac-1/chrome-mac/Chromium.app/Contents/MacOS/Chromium"));
    let mut places = Places {
        named: None,
        applications: vec![root.join("Applications")],
        path: vec![root.join("empty"), root.join("bin")],
        caches: vec![root.join("cache")],
    };
    assert_eq!(find(&places), Ok(app.clone()));
    std::fs::remove_file(&app).unwrap();
    assert_eq!(find(&places), Ok(later_app));
    places.applications.clear();
    assert_eq!(find(&places), Ok(on_path));
    places.path.clear();
    assert_eq!(find(&places), Ok(cached));
    places.caches.clear();
    assert_eq!(find(&places), Err(FindError::NotFound));
}

#[test]
fn the_newest_cached_build_is_chosen() {
    let root = scratch("newest");
    let old = program(&root.join("cache/a/chrome-headless-shell"));
    let new = program(&root.join("cache/b/deep/er/chrome-headless-shell"));
    let earlier = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    std::fs::File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(earlier)
        .unwrap();
    // A file with the right name that cannot be run is passed over.
    std::fs::write(root.join("cache/chrome"), "x").unwrap();
    let places = Places {
        caches: vec![root.join("cache")],
        ..Places::default()
    };
    assert_eq!(find(&places), Ok(new));
}
