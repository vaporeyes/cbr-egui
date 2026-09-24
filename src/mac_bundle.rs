// ABOUTME: macOS development launcher that reruns `cargo run` builds through a tiny .app bundle.
// ABOUTME: Gives LaunchServices a foreground app identity so the window can receive keyboard input.

#[cfg(target_os = "macos")]
pub fn try_relaunch_via_bundle() -> Option<i32> {
    use std::env;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    const RELAUNCH_ENV: &str = "CBR_EGUI_MAC_APP_BUNDLE";
    const BUNDLE_NAME: &str = "cbr-egui.app";
    const BUNDLE_ID: &str = "dev.jsh.cbr-egui";

    if env::var(RELAUNCH_ENV).ok().as_deref() == Some("1") {
        return None;
    }

    let exe = env::current_exe().ok()?;
    let exe_str = exe.to_string_lossy();
    if exe_str.contains(".app/Contents/MacOS/") {
        return None;
    }

    let output_dir = exe.parent()?.to_path_buf();
    let bundle_path = output_dir.join(BUNDLE_NAME);
    let macos_dir = bundle_path.join("Contents").join("MacOS");
    let plist_path = bundle_path.join("Contents").join("Info.plist");
    let launcher_path = macos_dir.join("cbr-egui");

    fs::create_dir_all(&macos_dir).ok()?;

    let plist = info_plist(BUNDLE_ID);
    write_if_changed(&plist_path, &plist)?;

    let real_binary = exe.to_string_lossy().to_string();
    let launcher = launcher_script(RELAUNCH_ENV, &real_binary);
    write_if_changed(&launcher_path, &launcher)?;
    fs::set_permissions(&launcher_path, fs::Permissions::from_mode(0o755)).ok()?;

    let args: Vec<String> = env::args().skip(1).collect();
    let mut cmd = Command::new("open");
    cmd.arg("-n").arg("-W").arg(&bundle_path);
    if !args.is_empty() {
        cmd.arg("--args");
        for a in &args {
            cmd.arg(a);
        }
    }

    match cmd.status() {
        Ok(status) => Some(status.code().unwrap_or(0)),
        Err(_) => None,
    }
}

#[cfg(target_os = "macos")]
fn write_if_changed(path: &std::path::Path, contents: &str) -> Option<()> {
    use std::fs;
    if let Ok(existing) = fs::read_to_string(path)
        && existing == contents
    {
        return Some(());
    }
    fs::write(path, contents).ok()?;
    Some(())
}

#[cfg(target_os = "macos")]
fn info_plist(bundle_id: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>cbr-egui</string>
    <key>CFBundleIdentifier</key>
    <string>{bundle_id}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>cbr-egui</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSPrincipalClass</key>
    <string>NSApplication</string>
</dict>
</plist>
"#
    )
}

#[cfg(target_os = "macos")]
fn launcher_script(env_var: &str, real_binary: &str) -> String {
    let quoted_binary = real_binary.replace('\'', "'\"'\"'");
    format!("#!/bin/sh\nexport {env_var}=1\nexec '{quoted_binary}' \"$@\"\n")
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn launcher_preserves_literal_executable_path_and_arguments() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let executable = dir
            .path()
            .join("reader 'quoted' $HOME `literal` $(literal)");
        std::fs::write(&executable, "#!/bin/sh\nprintf '%s\\n' \"$1\"\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let launcher = dir.path().join("launcher.sh");
        std::fs::write(
            &launcher,
            super::launcher_script("CBR_TEST_LAUNCHER", executable.to_str().unwrap()),
        )
        .unwrap();
        let output = std::process::Command::new("/bin/sh")
            .arg(&launcher)
            .arg("argument with $literal")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"argument with $literal\n");
    }
}

#[cfg(not(target_os = "macos"))]
pub fn try_relaunch_via_bundle() -> Option<i32> {
    None
}
