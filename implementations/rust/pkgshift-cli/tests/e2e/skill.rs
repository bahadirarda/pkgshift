use super::*;

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("copy destination");
    let mut entries = fs::read_dir(source)
        .expect("copy source")
        .collect::<Result<Vec<_>, _>>()
        .expect("copy entries");
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let file_type = entry.file_type().expect("copy entry type");
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree(&entry.path(), &target);
        } else if file_type.is_file() {
            fs::copy(entry.path(), target).expect("copied file");
        } else {
            panic!("unexpected portable skill entry type");
        }
    }
}

#[test]
fn previews_installs_diagnoses_and_removes_a_managed_skill() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    fs::create_dir(&root).expect("project root");

    let preview = run(
        &root,
        &binaries,
        &[
            "skill", "install", "--scope", "project", "--client", "codex", "--mode", "copy",
        ],
    );
    assert_eq!(preview.status.code(), Some(7));
    let preview = json_output(&preview);
    let install_plan = preview["planId"].as_str().expect("skill installation plan");
    assert!(install_plan.starts_with("skill_plan_"));
    assert_eq!(preview["status"], "planned");
    assert_eq!(preview["summary"]["mutationPerformed"], false);
    assert_eq!(preview["nextActions"][0]["sideEffect"], "filesystem-write");
    assert_eq!(
        preview["nextActions"][0]["argv"],
        serde_json::json!([
            "pkgshift",
            "skill",
            "install",
            "--scope",
            "project",
            "--client",
            "codex",
            "--mode",
            "copy",
            "--approve",
            install_plan,
            "--json",
            "--no-color",
            "--non-interactive"
        ])
    );
    assert!(!root.join(".agents").exists());

    let dry_run = run(
        &root,
        &binaries,
        &[
            "skill",
            "install",
            "--scope",
            "project",
            "--client",
            "codex",
            "--mode",
            "copy",
            "--approve",
            install_plan,
            "--dry-run",
        ],
    );
    assert!(dry_run.status.success());
    assert_eq!(json_output(&dry_run)["status"], "planned");
    assert!(!root.join(".agents").exists());

    let installed = run(
        &root,
        &binaries,
        &[
            "skill",
            "install",
            "--scope",
            "project",
            "--client",
            "codex",
            "--mode",
            "copy",
            "--approve",
            install_plan,
        ],
    );
    assert!(
        installed.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&installed.stdout),
        String::from_utf8_lossy(&installed.stderr)
    );
    let installed = json_output(&installed);
    assert_eq!(installed["summary"]["installed"], true);
    assert_eq!(installed["summary"]["healthy"], true);
    assert_eq!(installed["summary"]["mutationPerformed"], true);
    let status = artifact_content(&installed, "skill-status");
    assert_eq!(status["sourceDigest"], status["installedDigest"]);
    let source = PathBuf::from(status["sourcePath"].as_str().expect("source path"));
    let target = root.join(".agents/skills/pkgshift");
    assert!(target.join("SKILL.md").is_file());

    let doctor = run(
        &root,
        &binaries,
        &["skill", "doctor", "--scope", "project", "--client", "codex"],
    );
    assert!(doctor.status.success());
    assert_eq!(json_output(&doctor)["summary"]["healthy"], true);

    write(&target.join("references/local.md"), "local modification\n");
    let uninstall_preview = run(
        &root,
        &binaries,
        &[
            "skill",
            "uninstall",
            "--scope",
            "project",
            "--client",
            "codex",
        ],
    );
    assert_eq!(uninstall_preview.status.code(), Some(7));
    let uninstall_preview = json_output(&uninstall_preview);
    let modified_uninstall_plan = uninstall_preview["planId"]
        .as_str()
        .expect("modified uninstall plan");
    let protected = run(
        &root,
        &binaries,
        &[
            "skill",
            "uninstall",
            "--scope",
            "project",
            "--client",
            "codex",
            "--approve",
            modified_uninstall_plan,
        ],
    );
    assert_eq!(protected.status.code(), Some(3));
    assert!(
        json_output(&protected)["diagnostics"]
            .as_array()
            .is_some_and(|entries| entries
                .iter()
                .any(|entry| entry["code"] == "SKILL_UNINSTALL_MODIFIED"))
    );
    assert!(target.exists());

    fs::remove_file(target.join("references/local.md")).expect("local modification");
    assert_eq!(
        fs::read_to_string(target.join("SKILL.md")).expect("installed skill"),
        fs::read_to_string(source.join("SKILL.md")).expect("source skill")
    );
    let stale_approval = run(
        &root,
        &binaries,
        &[
            "skill",
            "uninstall",
            "--scope",
            "project",
            "--client",
            "codex",
            "--approve",
            modified_uninstall_plan,
        ],
    );
    assert_eq!(stale_approval.status.code(), Some(7));
    assert!(target.exists());
    let current_uninstall_plan = json_output(&stale_approval)["planId"]
        .as_str()
        .expect("current uninstall plan")
        .to_owned();
    assert_ne!(current_uninstall_plan, modified_uninstall_plan);
    let removed = run(
        &root,
        &binaries,
        &[
            "skill",
            "uninstall",
            "--scope",
            "project",
            "--client",
            "codex",
            "--approve",
            &current_uninstall_plan,
        ],
    );
    assert!(removed.status.success());
    let removed = json_output(&removed);
    assert_eq!(removed["summary"]["installed"], false);
    assert_eq!(removed["summary"]["mutationPerformed"], true);
    assert!(!target.exists());
}

#[test]
fn owns_and_removes_only_an_exact_claude_skill_link() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let binaries = fake_package_managers(&directory);
    let root = directory.path().join("project");
    fs::create_dir(&root).expect("project root");
    let preview = run(
        &root,
        &binaries,
        &[
            "skill", "install", "--scope", "project", "--client", "claude", "--mode", "link",
        ],
    );
    assert_eq!(preview.status.code(), Some(7));
    let install_plan = json_output(&preview)["planId"]
        .as_str()
        .expect("link install plan")
        .to_owned();
    let installed = run(
        &root,
        &binaries,
        &[
            "skill",
            "install",
            "--scope",
            "project",
            "--client",
            "claude",
            "--mode",
            "link",
            "--approve",
            &install_plan,
        ],
    );
    assert!(installed.status.success());
    let target = root.join(".claude/skills/pkgshift");
    assert!(
        fs::symlink_metadata(&target)
            .expect("skill link")
            .file_type()
            .is_symlink()
    );

    let preview = run(
        &root,
        &binaries,
        &[
            "skill",
            "uninstall",
            "--scope",
            "project",
            "--client",
            "claude",
        ],
    );
    assert_eq!(preview.status.code(), Some(7));
    let uninstall_plan = json_output(&preview)["planId"]
        .as_str()
        .expect("link uninstall plan")
        .to_owned();
    let removed = run(
        &root,
        &binaries,
        &[
            "skill",
            "uninstall",
            "--scope",
            "project",
            "--client",
            "claude",
            "--approve",
            &uninstall_plan,
        ],
    );
    assert!(removed.status.success());
    assert!(!target.exists());
}

#[test]
fn resolves_the_portable_skill_from_the_release_shared_data_layout() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let distribution = directory.path().join("distribution");
    let binary = distribution.join("bin/pkgshift");
    fs::create_dir_all(binary.parent().expect("binary parent")).expect("binary directory");
    fs::copy(env!("CARGO_BIN_EXE_pkgshift"), &binary).expect("release binary");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../skills/pkgshift");
    let shared = distribution.join("share/pkgshift/skills/pkgshift");
    copy_tree(&source, &shared);
    let project = directory.path().join("project");
    fs::create_dir(&project).expect("project directory");

    let output = Command::new(&binary)
        .args([
            "skill", "status", "--scope", "project", "--client", "codex", "--cwd",
        ])
        .arg(&project)
        .args(["--json", "--non-interactive"])
        .output()
        .expect("release-layout status");
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result = json_output(&output);
    let status = artifact_content(&result, "skill-status");
    assert_eq!(
        Path::new(status["sourcePath"].as_str().expect("source path")),
        fs::canonicalize(shared).expect("shared source")
    );
}
