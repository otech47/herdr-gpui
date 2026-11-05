#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn preview_paths_only_include_renderable_documents() {
    for path in [
        "/home/user/.agent-scratch/run/files/reply-draft.md",
        "./report.HTML",
        "~/notes.markdown",
    ] {
        assert!(preview_path(path));
    }
    for path in ["./run.sh", "./script.py", "./Tool.app", "./report.md.exe"] {
        assert!(!preview_path(path));
    }
}

#[test]
fn local_documents_use_the_bounded_folder_preview() {
    let root = tempfile::Builder::new()
        .prefix("herdr-document-")
        .tempdir()
        .unwrap();
    let page = root.path().join("notes.md");
    std::fs::write(&page, "# Notes").unwrap();
    let location = locate(
        FileLink {
            path: page.to_string_lossy().into_owned(),
            cwd: None,
        },
        ConnectTarget::Local,
    )
    .unwrap();
    let Location::Local { file } = location else {
        panic!("local document was not previewed")
    };
    assert_eq!(file.path(), super::super::real_path(&page).unwrap());
}

#[cfg(unix)]
#[test]
fn remote_paths_remain_literal_shell_arguments() {
    let root = tempfile::tempdir().unwrap();
    let helper = root.path().join(".local/bin");
    std::fs::create_dir_all(&helper).unwrap();
    let tool = helper.join("tailopen");
    std::fs::write(&tool, "#!/bin/sh\nprintf '%s' \"$2\"\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    for name in ["a 'quote'.md", "$(touch injected).md", "a `id`.md"] {
        std::fs::write(root.path().join(name), "# Notes").unwrap();
        for path in [name.to_owned(), format!("~/{name}")] {
            let link = FileLink {
                path,
                cwd: Some(root.path().to_string_lossy().into_owned()),
            };
            let result = std::process::Command::new("/bin/sh")
                .args(["-c", &reveal_script(&link).unwrap()])
                .env("HOME", root.path())
                .current_dir(root.path())
                .output()
                .unwrap();
            assert!(result.status.success());
            assert_eq!(
                String::from_utf8(result.stdout).unwrap(),
                root.path().join(name).to_string_lossy()
            );
        }
    }
    assert!(!root.path().join("injected").exists());
}

#[test]
#[ignore = "requires an explicitly selected host and document"]
fn selected_host_document_resolves_to_its_file_server() {
    let path = std::env::var("HERDR_TEST_DOCUMENT_PATH").unwrap();
    let target = std::env::var("HERDR_TEST_DOCUMENT_HOST").unwrap();
    let Location::Web { url } = locate(
        FileLink { path, cwd: None },
        ConnectTarget::Ssh {
            target,
            session: "default".into(),
        },
    )
    .unwrap() else {
        panic!("remote document was resolved through local storage")
    };
    assert!(url.report_title().is_some());
    let response = ureq::get(url.as_str()).call().unwrap();
    assert_eq!(response.status().as_u16(), 200);
}
