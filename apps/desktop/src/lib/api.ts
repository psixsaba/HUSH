export type NsBackend = "off" | "dpdf_net" | "dpdf_net2" | "deep_filter_net3" | "rn_noise" | "target_speaker";

export interface DeviceLists {
  inputs: string[];
  outputs: string[];
}

export interface SessionStats {
  running: boolean;
  ready: boolean;
  frames: number;
  process_ns: number;
  cpu_percent: number;
  algorithmic_latency_samples: number;
  algorithmic_latency_ms: number;
  buffer_samples: number;
  buffer_ms: number;
  ring_underruns: number;
  output_underruns: number;
  input_drops: number;
  ns_backend: NsBackend;
  strength: number;
  aec_enabled: boolean;
  enabled: boolean;
  clean_incoming: boolean;
  incoming_latency_samples: number;
  incoming_latency_ms: number;
  input_level: number;
  output_level: number;
  error: string | null;
}

export interface Settings {
  input_device: string | null;
  output_device: string | null;
  far_device: string | null;
  enabled: boolean;
  muted: boolean;
  aec_enabled: boolean;
  ns_backend: NsBackend;
  strength: number;
  monitor_cleaned: boolean;
  clean_incoming: boolean;
}

const previewDevices: DeviceLists = {
  inputs: ["MacBook Pro Microphone", "External Interface"],
  outputs: ["MacBook Pro Speakers", "Headphones"],
};

function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function shellConnected(): boolean {
  return inTauri();
}

export async function listDevices(): Promise<DeviceLists> {
  if (!inTauri()) return previewDevices;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<DeviceLists>("list_devices");
}

export async function start(settings: Settings): Promise<void> {
  if (!inTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("start_session", { settings });
}

export async function stop(): Promise<void> {
  if (!inTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("stop_session");
}

export async function update(settings: Settings): Promise<void> {
  if (!inTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("update_settings", { settings });
}

export async function stats(): Promise<SessionStats | null> {
  if (!inTauri()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<SessionStats | null>("session_stats");
}

export interface DriverStatus {
  microphone: boolean;
  speaker: boolean;
}

export interface SavedUi {
  running: boolean;
  start_on_login: boolean;
  theme: "dark" | "light";
}

const STORAGE_KEY = "hush.ui.v2";

export function loadUi(): SavedUi {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { running: false, start_on_login: true, theme: "dark" };
    const parsed = JSON.parse(raw) as Partial<SavedUi>;
    return {
      running: Boolean(parsed.running),
      start_on_login: parsed.start_on_login !== false,
      theme: parsed.theme === "light" ? "light" : "dark",
    };
  } catch {
    return { running: false, start_on_login: true, theme: "dark" };
  }
}

export function saveUi(ui: SavedUi) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(ui));
}

export function loadSettings(): Settings | null {
  try {
    const raw = localStorage.getItem("hush.settings.v2");
    if (!raw) return null;
    return JSON.parse(raw) as Settings;
  } catch {
    return null;
  }
}

export function saveSettings(settings: Settings) {
  localStorage.setItem("hush.settings.v2", JSON.stringify(settings));
}

export async function driverStatus(): Promise<DriverStatus> {
  if (!inTauri()) return { microphone: false, speaker: false };
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<DriverStatus>("driver_status");
}

export interface AudioStatus {
  route: "vbcable" | "driver";
  cable_present: boolean;
  capture_label: string;
  platform: string;
}

export async function audioStatus(): Promise<AudioStatus> {
  if (!inTauri()) {
    return { route: "driver", cable_present: false, capture_label: "HUSH Microphone", platform: "browser" };
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<AudioStatus>("audio_status");
}

export async function installCable(): Promise<string> {
  if (!inTauri()) return "Open the HUSH app to install VB-CABLE.";
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<string>("install_cable");
}

export async function hostOs(): Promise<string> {
  if (!inTauri()) return "browser";
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<string>("host_os");
}

export async function setTrayRunning(running: boolean): Promise<void> {
  if (!inTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("set_tray_running", { running });
}

export async function setOpenAtLogin(enabled: boolean): Promise<void> {
  if (!inTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("set_open_at_login", { enabled });
}

export async function installDriver(): Promise<string> {
  if (!inTauri()) return "Open the HUSH app to install the driver.";
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<string>("install_driver");
}

export async function listenTray(handler: () => void): Promise<() => void> {
  if (!inTauri()) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen("tray-power", () => handler());
  return unlisten;
}
