use std::{fs, path::PathBuf, process::Command};

pub fn handle_service_command(args: &[String]) {
    if args.is_empty() {
        print_service_usage();
    }

    let action = args[0].as_str();
    match action {
        "install" => {
            let bin_path = parse_bin_path_arg(&args[1..]);
            install_service(bin_path);
        }
        "uninstall" => uninstall_service(),
        "start" => start_service(),
        "stop" => stop_service(),
        "restart" => restart_service(),
        "status" => status_service(),
        _ => print_service_usage(),
    }
}

fn print_service_usage() -> ! {
    eprintln!(
        "usage: kittctl service <install|uninstall|start|stop|restart|status> [--bin-path <path>]"
    );
    std::process::exit(2);
}

fn parse_bin_path_arg(args: &[String]) -> Option<PathBuf> {
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--bin-path" && i + 1 < args.len() {
            return Some(PathBuf::from(&args[i + 1]));
        }
        i += 1;
    }
    None
}

fn resolve_kittd_binary(explicit: Option<PathBuf>) -> PathBuf {
    if let Some(path) = explicit {
        if path.is_file() {
            return fs::canonicalize(&path).unwrap_or(path);
        }
        eprintln!("error: specified binary does not exist: {}", path.display());
        std::process::exit(1);
    }

    // Try sibling of current executable
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            let candidate = if cfg!(windows) {
                parent.join("kittd.exe")
            } else {
                parent.join("kittd")
            };
            if candidate.is_file() {
                return fs::canonicalize(&candidate).unwrap_or(candidate);
            }
        }
    }

    // Try in standard PATH
    let binary_name = if cfg!(windows) { "kittd.exe" } else { "kittd" };
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(binary_name);
            if candidate.is_file() {
                return fs::canonicalize(&candidate).unwrap_or(candidate);
            }
        }
    }

    // Try ~/.local/bin/kittd
    if let Some(home) = dirs::home_dir() {
        let candidate = home.join(".local").join("bin").join(binary_name);
        if candidate.is_file() {
            return fs::canonicalize(&candidate).unwrap_or(candidate);
        }
    }

    eprintln!("error: could not locate kittd binary. Please build it or pass --bin-path <path>");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
fn install_service(explicit_bin: Option<PathBuf>) {
    let bin = resolve_kittd_binary(explicit_bin);
    let home = dirs::home_dir().expect("home dir required");
    let systemd_dir = home.join(".config").join("systemd").join("user");
    fs::create_dir_all(&systemd_dir).unwrap_or_else(|e| {
        eprintln!("error: failed to create systemd user directory: {e}");
        std::process::exit(1);
    });

    let service_content = format!(
        r#"[Unit]
Description=K.I.T.T. Assistant daemon
After=network.target

[Service]
Type=simple
ExecStart={}
Restart=on-failure
RestartSec=2
NoNewPrivileges=true
PrivateTmp=false

[Install]
WantedBy=default.target
"#,
        bin.display()
    );

    let service_file = systemd_dir.join("kitt-assistant.service");
    fs::write(&service_file, service_content).unwrap_or_else(|e| {
        eprintln!("error: failed to write service file: {e}");
        std::process::exit(1);
    });

    println!("Service file written to: {}", service_file.display());

    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();

    let status = Command::new("systemctl")
        .args(["--user", "enable", "kitt-assistant.service"])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("K.I.T.T. systemd service enabled successfully.");
        }
        _ => {
            eprintln!("warning: could not enable systemd user service via systemctl.");
        }
    }
}

#[cfg(target_os = "macos")]
fn install_service(explicit_bin: Option<PathBuf>) {
    let bin = resolve_kittd_binary(explicit_bin);
    let home = dirs::home_dir().expect("home dir required");
    let launch_agents = home.join("Library").join("LaunchAgents");
    fs::create_dir_all(&launch_agents).unwrap_or_else(|e| {
        eprintln!("error: failed to create LaunchAgents directory: {e}");
        std::process::exit(1);
    });

    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.kitt.assistant</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/kitt-assistant.out.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/kitt-assistant.err.log</string>
</dict>
</plist>
"#,
        bin.display()
    );

    let plist_file = launch_agents.join("com.kitt.assistant.plist");
    fs::write(&plist_file, plist_content).unwrap_or_else(|e| {
        eprintln!("error: failed to write plist file: {e}");
        std::process::exit(1);
    });

    println!("LaunchAgent plist written to: {}", plist_file.display());

    let _ = Command::new("launchctl")
        .args(["unload", plist_file.to_str().unwrap()])
        .status();

    let status = Command::new("launchctl")
        .args(["load", "-w", plist_file.to_str().unwrap()])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("K.I.T.T. LaunchAgent loaded successfully.");
        }
        _ => {
            eprintln!("warning: could not load LaunchAgent via launchctl.");
        }
    }
}

#[cfg(target_os = "windows")]
fn install_service(explicit_bin: Option<PathBuf>) {
    let bin = resolve_kittd_binary(explicit_bin);
    let bin_str = bin.to_string_lossy().to_string();

    let script = format!(
        r#"$action = New-ScheduledTaskAction -Execute "{}"
$trigger = New-ScheduledTaskTrigger -AtLogOn
$settings = New-ScheduledTaskSettingsSet -StartWhenAvailable -ExecutionTimeLimit (New-TimeSpan -Days 3650)
Register-ScheduledTask -TaskName "KITT Assistant" -Action $action -Trigger $trigger -Settings $settings -Description "K.I.T.T. background service" -Force
"#,
        bin_str.replace('"', "`\"")
    );

    let status = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("K.I.T.T. Windows Scheduled Task registered successfully.");
        }
        _ => {
            eprintln!("warning: could not register Windows scheduled task.");
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn install_service(_explicit_bin: Option<PathBuf>) {
    eprintln!("error: native service management is not supported on this platform.");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
fn uninstall_service() {
    let _ = Command::new("systemctl")
        .args(["--user", "stop", "kitt-assistant.service"])
        .status();
    let _ = Command::new("systemctl")
        .args(["--user", "disable", "kitt-assistant.service"])
        .status();

    if let Some(home) = dirs::home_dir() {
        let service_file = home
            .join(".config")
            .join("systemd")
            .join("user")
            .join("kitt-assistant.service");
        if service_file.exists() {
            let _ = fs::remove_file(&service_file);
            println!("Removed {}", service_file.display());
        }
    }

    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();
    println!("K.I.T.T. systemd service uninstalled.");
}

#[cfg(target_os = "macos")]
fn uninstall_service() {
    if let Some(home) = dirs::home_dir() {
        let plist_file = home
            .join("Library")
            .join("LaunchAgents")
            .join("com.kitt.assistant.plist");
        if plist_file.exists() {
            let _ = Command::new("launchctl")
                .args(["unload", plist_file.to_str().unwrap()])
                .status();
            let _ = fs::remove_file(&plist_file);
            println!("Removed {}", plist_file.display());
        }
    }
    println!("K.I.T.T. LaunchAgent uninstalled.");
}

#[cfg(target_os = "windows")]
fn uninstall_service() {
    let script = r#"Unregister-ScheduledTask -TaskName "KITT Assistant" -Confirm:$false -ErrorAction SilentlyContinue"#;
    let _ = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .status();
    println!("K.I.T.T. Windows Scheduled Task uninstalled.");
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn uninstall_service() {
    eprintln!("error: unsupported platform.");
}

pub fn start_service() {
    #[cfg(target_os = "linux")]
    {
        let status = Command::new("systemctl")
            .args(["--user", "start", "kitt-assistant.service"])
            .status();
        if status.is_ok_and(|s| s.success()) {
            println!("K.I.T.T. background service started.");
        } else {
            eprintln!("error: failed to start service. Have you run 'kittctl service install'?");
            std::process::exit(1);
        }
    }
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("launchctl")
            .args(["start", "com.kitt.assistant"])
            .status();
        if status.is_ok_and(|s| s.success()) {
            println!("K.I.T.T. background service started.");
        } else {
            eprintln!("error: failed to start service.");
            std::process::exit(1);
        }
    }
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                r#"Start-ScheduledTask -TaskName "KITT Assistant""#,
            ])
            .status();
        println!("K.I.T.T. scheduled task started.");
    }
}

pub fn stop_service() {
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("systemctl")
            .args(["--user", "stop", "kitt-assistant.service"])
            .status();
        println!("K.I.T.T. background service stopped.");
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("launchctl")
            .args(["stop", "com.kitt.assistant"])
            .status();
        println!("K.I.T.T. background service stopped.");
    }
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                r#"Stop-ScheduledTask -TaskName "KITT Assistant""#,
            ])
            .status();
        println!("K.I.T.T. scheduled task stopped.");
    }
}

pub fn restart_service() {
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("systemctl")
            .args(["--user", "restart", "kitt-assistant.service"])
            .status();
        println!("K.I.T.T. background service restarted.");
    }
    #[cfg(target_os = "macos")]
    {
        stop_service();
        start_service();
    }
    #[cfg(target_os = "windows")]
    {
        stop_service();
        start_service();
    }
}

pub fn status_service() {
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("systemctl")
            .args(["--user", "status", "kitt-assistant.service", "--no-pager"])
            .status();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("launchctl")
            .args(["list", "com.kitt.assistant"])
            .status();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                r#"Get-ScheduledTask -TaskName "KITT Assistant""#,
            ])
            .status();
    }
}
