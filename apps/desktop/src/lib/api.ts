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
