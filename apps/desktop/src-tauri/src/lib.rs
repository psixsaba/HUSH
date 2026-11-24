use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread::JoinHandle;

use hush_core::{NsBackend, PipelineConfig, RealtimeSession, SessionConfig, SessionStats};
use serde::{Deserialize, Serialize};

struct AppState {
    worker: Mutex<Option<Worker>>,
    power_item: Mutex<Option<tauri::menu::MenuItem<tauri::Wry>>>,
}

struct Worker {
    tx: Sender<Cmd>,
    handle: Option<JoinHandle<()>>,
}

enum Cmd {
    Update(PipelineConfig),
    Stats(Sender<SessionStats>),
    Stop,
}

#[derive(Deserialize)]
struct UiSettings {
    input_device: Option<String>,
    output_device: Option<String>,
    far_device: Option<String>,
    enabled: bool,
    muted: bool,
    aec_enabled: bool,
    ns_backend: NsBackend,
    strength: f32,
    monitor_cleaned: bool,
    clean_incoming: bool,
}

fn blank_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn session_config(settings: &UiSettings) -> Result<SessionConfig, String> {
    let mut config = SessionConfig {
        input_device: blank_to_none(settings.input_device.clone()),
        output_device: blank_to_none(settings.output_device.clone()),
        far_device: blank_to_none(settings.far_device.clone()),
        pipeline: PipelineConfig {
            enabled: settings.enabled,
            muted: settings.muted,
            aec_enabled: settings.aec_enabled,
            ns_backend: settings.ns_backend,
            strength: settings.strength,
            clean_incoming: settings.clean_incoming,
        },
        publish_ring: true,
        bridge_speaker: true,
        monitor_cleaned: settings.monitor_cleaned,
        loopback_device: None,
    };
    #[cfg(windows)]
    {
        let lists = hush_core::list_devices().map_err(|err| err.to_string())?;
        let plan = hush_core::engine_plan(
            hush_core::hush_vad_loaded(),
            &lists.outputs,
            config.output_device.as_deref(),
        );
        config.publish_ring = plan.publish_ring;
        config.bridge_speaker = plan.bridge_speaker;
        if plan.route == hush_core::AudioRoute::VbCable {
            config.pipeline.clean_incoming = false;
            if !plan.cable_present {
                return Err("VB-CABLE is not installed. Use Install VB-CABLE in this window.".into());
            }
            config.output_device = plan.playback_device;
            config.loopback_device = plan.loopback_device;
        }
    }
    Ok(config)
}

fn owner(config: SessionConfig, commands: Receiver<Cmd>, ready: Sender<Result<(), String>>) {
    let mut session = match RealtimeSession::start(config) {
        Ok(session) => session,
        Err(err) => {
            let _ = ready.send(Err(err.to_string()));
            return;
        }
    };
    let _ = ready.send(Ok(()));
    while let Ok(command) = commands.recv() {
        match command {
            Cmd::Update(pipeline) => session.set_config(pipeline),
            Cmd::Stats(reply) => {
                let _ = reply.send(session.stats());
            }
            Cmd::Stop => {
                session.stop();
                break;
            }
        }
    }
}

#[tauri::command]
fn list_devices() -> Result<hush_core::DeviceLists, String> {
    hush_core::list_devices().map_err(|err| err.to_string())
}

#[tauri::command]
fn start_session(state: tauri::State<AppState>, settings: UiSettings) -> Result<(), String> {
    let mut slot = state.worker.lock().map_err(|err| err.to_string())?;
    if let Some(mut running) = slot.take() {
        let _ = running.tx.send(Cmd::Stop);
        if let Some(handle) = running.handle.take() {
            let _ = handle.join();
        }
    }
    let (tx, rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let config = session_config(&settings)?;
    let handle = std::thread::Builder::new()
        .name("hush-session".into())
        .spawn(move || owner(config, rx, ready_tx))
        .map_err(|err| err.to_string())?;
    match ready_rx.recv().map_err(|err| err.to_string())? {
        Ok(()) => {
            *slot = Some(Worker {
                tx,
                handle: Some(handle),
            });
            Ok(())
        }
        Err(err) => {
            let _ = handle.join();
            Err(err)
        }
    }
}

#[tauri::command]
fn stop_session(state: tauri::State<AppState>) -> Result<(), String> {
    let mut slot = state.worker.lock().map_err(|err| err.to_string())?;
    if let Some(mut worker) = slot.take() {
        let _ = worker.tx.send(Cmd::Stop);
        if let Some(handle) = worker.handle.take() {
            let _ = handle.join();
        }
    }
    Ok(())
}

#[tauri::command]
fn update_settings(state: tauri::State<AppState>, settings: UiSettings) -> Result<(), String> {
    let slot = state.worker.lock().map_err(|err| err.to_string())?;
    if let Some(worker) = slot.as_ref() {
        worker
            .tx
            .send(Cmd::Update(session_config(&settings)?.pipeline))
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn session_stats(state: tauri::State<AppState>) -> Result<Option<SessionStats>, String> {
    let slot = state.worker.lock().map_err(|err| err.to_string())?;
    let Some(worker) = slot.as_ref() else {
        return Ok(None);
    };
    let (tx, rx) = mpsc::channel();
    worker.tx.send(Cmd::Stats(tx)).map_err(|err| err.to_string())?;
    rx.recv().map(Some).map_err(|err| err.to_string())
}

#[derive(Serialize)]
struct DriverStatus {
    microphone: bool,
    speaker: bool,
}

#[tauri::command]
fn driver_status() -> Result<DriverStatus, String> {
    let lists = hush_core::list_devices().map_err(|err| err.to_string())?;
    Ok(DriverStatus {
        microphone: lists.inputs.iter().any(|name| hush_core::is_hush_microphone(name)),
        speaker: lists.outputs.iter().any(|name| hush_core::is_hush_speaker(name)),
    })
}

#[tauri::command]
fn host_os() -> &'static str {
    std::env::consts::OS
}

#[derive(Serialize)]
struct AudioStatus {
    route: String,
    cable_present: bool,
    capture_label: String,
    platform: String,
}

#[tauri::command]
fn audio_status() -> Result<AudioStatus, String> {
    let lists = hush_core::list_devices().map_err(|err| err.to_string())?;
    #[cfg(windows)]
    let loaded = hush_core::hush_vad_loaded();
    #[cfg(not(windows))]
    let loaded = true;
    let plan = hush_core::engine_plan(loaded, &lists.outputs, None);
    let route = match plan.route {
        hush_core::AudioRoute::VbCable => "vbcable",
        hush_core::AudioRoute::VirtualDriver => "driver",
    };
    Ok(AudioStatus {
        route: route.into(),
        cable_present: plan.cable_present,
        capture_label: plan.capture_label.into(),
        platform: std::env::consts::OS.into(),
    })
}

#[tauri::command]
fn install_cable() -> Result<String, String> {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe().map_err(|err| err.to_string())?;
        let sibling = exe.parent().map(|dir| dir.join("hush.exe"));
        let target = sibling.filter(|path| path.is_file()).unwrap_or(exe);
        let command = format!(
            "Start-Process -FilePath '{}' -ArgumentList 'vbcable','install' -Verb RunAs -Wait",
            target.display()
        );
        let status = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &command])
            .status()
            .map_err(|err| err.to_string())?;
        if status.success() {
            Ok("Windows asked for approval to install VB-CABLE. Reboot once if HUSH Microphone is still missing. Secure Boot can stay on.".into())
        } else {
            Err("Windows did not start the VB-CABLE install.".into())
        }
    }
    #[cfg(not(windows))]
    {
        Err("VB-CABLE is the Windows microphone. On macOS, use Install driver.".into())
    }
}

#[tauri::command]
fn set_tray_running(state: tauri::State<AppState>, running: bool) -> Result<(), String> {
    let slot = state.power_item.lock().map_err(|err| err.to_string())?;
    if let Some(item) = slot.as_ref() {
        item.set_text(if running { "Turn off" } else { "Turn on" })
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn set_open_at_login(enabled: bool) -> Result<(), String> {
    login::set(enabled)
}

#[tauri::command]
fn open_at_login() -> Result<bool, String> {
    login::enabled()
}

#[tauri::command]
fn install_driver(app: tauri::AppHandle) -> Result<String, String> {
    driver_install::run(&app)
}

fn show_main(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;
    use tauri::{Emitter, Manager};

    let power = MenuItem::with_id(app, "power", "Turn on", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "Open HUSH", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&power, &show, &quit])?;
    if let Ok(mut slot) = app.state::<AppState>().power_item.lock() {
        *slot = Some(power);
    }
    let mut builder = TrayIconBuilder::with_id("hush")
        .tooltip("HUSH is off")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "quit" => app.exit(0),
            "show" => show_main(app),
            "power" => {
                show_main(app);
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("tray-power", ());
                }
            }
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

pub fn run() {
    #[cfg(windows)]
    std::thread::spawn(|| {
        // The login task starts `hush service` in this session. Give it the
        // bridge mutex. If it is not there, the window owns the loop.
        std::thread::sleep(std::time::Duration::from_secs(2));
        let Some(_guard) = hush_core::acquire_bridge() else {
            return;
        };
        let stop = std::sync::atomic::AtomicBool::new(false);
        let _ = hush_core::run_windows_bridge(&stop);
    });

    tauri::Builder::default()
        .manage(AppState {
            worker: Mutex::new(None),
            power_item: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            list_devices,
            start_session,
            stop_session,
            update_settings,
            session_stats,
            driver_status,
            host_os,
            audio_status,
            install_cable,
            set_tray_running,
            set_open_at_login,
            open_at_login,
            install_driver
        ])
        .setup(|app| {
            tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("Hush failed to start");
}

mod login {
    use std::path::PathBuf;
    use std::process::Command;

    pub fn set(enabled: bool) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            return macos(enabled);
        }
        #[cfg(target_os = "windows")]
        {
            return windows(enabled);
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = enabled;
            Ok(())
        }
    }

    pub fn enabled() -> Result<bool, String> {
        #[cfg(target_os = "macos")]
        {
            return Ok(agent_path()?.is_file());
        }
        #[cfg(target_os = "windows")]
        {
            let output = Command::new("reg")
                .args(["query", run_key(), "/v", "HUSH"])
                .output()
                .map_err(|err| err.to_string())?;
            return Ok(output.status.success());
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            Ok(false)
        }
    }

    #[cfg(target_os = "macos")]
    fn macos(enabled: bool) -> Result<(), String> {
        let path = agent_path()?;
        let label = format!("gui/{}/com.hush.desktop", user_id()?);
        let _ = Command::new("launchctl").args(["bootout", &label]).status();
        if !enabled {
            let _ = std::fs::remove_file(&path);
            return Ok(());
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
        }
        let exe = std::env::current_exe().map_err(|err| err.to_string())?;
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>com.hush.desktop</string>
<key>ProgramArguments</key><array><string>{}</string></array>
<key>RunAtLoad</key><true/>
<key>ProcessType</key><string>Interactive</string>
</dict></plist>
"#,
            xml_escape(&exe.display().to_string())
        );
        std::fs::write(&path, xml).map_err(|err| err.to_string())?;
        let status = Command::new("launchctl")
            .args(["bootstrap", &format!("gui/{}", user_id()?), &path.display().to_string()])
            .status()
            .map_err(|err| err.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err("launchctl could not install the login item".into())
        }
    }

    #[cfg(target_os = "windows")]
    fn windows(enabled: bool) -> Result<(), String> {
        if !enabled {
            let _ = Command::new("reg").args(["delete", run_key(), "/v", "HUSH", "/f"]).status();
            return Ok(());
        }
        let exe = std::env::current_exe().map_err(|err| err.to_string())?;
        let value = format!("\"{}\"", exe.display());
        let status = Command::new("reg")
            .args(["add", run_key(), "/v", "HUSH", "/t", "REG_SZ", "/d", &value, "/f"])
            .status()
            .map_err(|err| err.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err("could not write the Windows login entry".into())
        }
    }

    #[cfg(target_os = "windows")]
    fn run_key() -> &'static str {
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run"
    }

    #[cfg(target_os = "macos")]
    fn agent_path() -> Result<PathBuf, String> {
        let home = std::env::var("HOME").map_err(|_| "HOME is not set".to_string())?;
        Ok(PathBuf::from(home).join("Library/LaunchAgents/com.hush.desktop.plist"))
    }

    #[cfg(target_os = "macos")]
    fn user_id() -> Result<String, String> {
        let output = Command::new("id").arg("-u").output().map_err(|err| err.to_string())?;
        if !output.status.success() {
            return Err("id -u failed".into());
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    #[cfg(target_os = "macos")]
    fn xml_escape(text: &str) -> String {
        text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }
}

mod driver_install {
    use std::path::Path;
    use std::process::Command;

    use tauri::Manager;

    pub fn run(app: &tauri::AppHandle) -> Result<String, String> {
        let resources = app.path().resource_dir().map_err(|err| err.to_string())?;
        #[cfg(target_os = "macos")]
        {
            return macos(&resources);
        }
        #[cfg(target_os = "windows")]
        {
            return windows(&resources);
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = resources;
            Err("Install the driver on macOS or Windows.".into())
        }
    }

    #[cfg(target_os = "macos")]
    fn macos(resources: &Path) -> Result<String, String> {
        let src = resources.join("HUSH.driver");
        if !src.join("Contents/MacOS").is_dir() && !src.is_dir() {
            return Err("This app does not contain HUSH.driver. Open the disk image and run the package.".into());
        }
        if !src.is_dir() {
            return Err(format!("missing {}", src.display()));
        }
        let dest = "/Library/Audio/Plug-Ins/HAL/HUSH.driver";
        let inner = format!(
            "rm -rf \"{dest}\" && cp -R \"{}\" \"{dest}\" && (killall coreaudiod || true) && echo installed",
            src.display()
        );
        let apple = format!("do shell script \"{}\" with administrator privileges", osa(&inner));
        let output = Command::new("osascript")
            .arg("-e")
            .arg(&apple)
            .output()
            .map_err(|err| err.to_string())?;
        if output.status.success() {
            Ok("Installed HUSH.driver and restarted coreaudiod. Choose HUSH Microphone and HUSH Speaker in the call app.".into())
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            Err(format!("macOS did not install the driver. {err}"))
        }
    }

    #[cfg(target_os = "windows")]
    fn windows(resources: &Path) -> Result<String, String> {
        let script = resources.join("install-driver.ps1");
        if !script.is_file() {
            return Err("install-driver.ps1 is not in this app. Reinstall HUSH from the setup exe.".into());
        }
        let command = format!(
            "Start-Process -FilePath powershell.exe -Verb RunAs -Wait -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File','{}')",
            script.display().to_string().replace('\'', "''")
        );
        let status = Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &command])
            .status()
            .map_err(|err| err.to_string())?;
        if status.success() {
            Ok("Windows asked for administrator approval. If Test Mode is on, HUSH Microphone and HUSH Speaker appear after the driver binds.".into())
        } else {
            Err("Windows did not start the driver installer.".into())
        }
    }

    #[cfg(target_os = "macos")]
    fn osa(script: &str) -> String {
        script.replace('\\', "\\\\").replace('"', "\\\"")
    }
}
