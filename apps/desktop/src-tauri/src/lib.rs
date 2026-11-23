use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread::JoinHandle;

use hush_core::{NsBackend, PipelineConfig, RealtimeSession, SessionConfig, SessionStats};
use serde::Deserialize;

struct AppState {
    worker: Mutex<Option<Worker>>,
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

fn session_config(settings: &UiSettings) -> SessionConfig {
    SessionConfig {
        input_device: blank_to_none(settings.input_device.clone()),
        output_device: blank_to_none(settings.output_device.clone()),
        far_device: blank_to_none(settings.far_device.clone()),
        pipeline: PipelineConfig {
            enabled: settings.enabled,
            muted: settings.muted,
            aec_enabled: settings.aec_enabled,
            ns_backend: settings.ns_backend,
            strength: settings.strength,
        },
        publish_ring: true,
        bridge_speaker: true,
        monitor_cleaned: settings.monitor_cleaned,
    }
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
    let config = session_config(&settings);
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
            .send(Cmd::Update(session_config(&settings).pipeline))
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

pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            worker: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            list_devices,
            start_session,
            stop_session,
            update_settings,
            session_stats
        ])
        .run(tauri::generate_context!())
        .expect("Hush failed to start");
}
