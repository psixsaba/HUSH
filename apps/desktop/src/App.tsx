import { useCallback, useEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import {
  driverStatus,
  hostOs,
  installDriver,
  listenTray,
  listDevices,
  loadSettings,
  loadUi,
  saveSettings,
  saveUi,
  setOpenAtLogin,
  setTrayRunning,
  shellConnected,
  start,
  stats,
  stop,
  update,
  type DeviceLists,
  type DriverStatus,
  type NsBackend,
  type SessionStats,
  type Settings,
} from "@/lib/api";

function hardwareInput(names: string[]): string | null {
  return names.find((name) => !/hush/i.test(name)) ?? names[0] ?? null;
}

function hardwareOutput(names: string[]): string | null {
  return names.find((name) => !/hush speaker/i.test(name)) ?? names[0] ?? null;
}

const emptyStats: SessionStats = {
  running: false,
  ready: false,
  frames: 0,
  process_ns: 0,
  cpu_percent: 0,
  algorithmic_latency_samples: 0,
  algorithmic_latency_ms: 0,
  buffer_samples: 0,
  buffer_ms: 0,
  ring_underruns: 0,
  output_underruns: 0,
  input_drops: 0,
  ns_backend: "dpdf_net",
  strength: 1,
  aec_enabled: true,
  enabled: false,
  clean_incoming: true,
  incoming_latency_samples: 0,
  incoming_latency_ms: 0,
  input_level: 0,
  output_level: 0,
  error: null,
};

const defaultSettings: Settings = {
  input_device: null,
  output_device: null,
  far_device: null,
  enabled: true,
  muted: false,
  aec_enabled: true,
  ns_backend: "dpdf_net",
  strength: 1,
  monitor_cleaned: false,
  clean_incoming: true,
};

export function App() {
  const connected = shellConnected();
  const demo = new URLSearchParams(window.location.search).has("demo");
  const savedUi = useRef(loadUi());
  const [theme, setTheme] = useState<"dark" | "light">(savedUi.current.theme);
  const [devices, setDevices] = useState<DeviceLists>({ inputs: [], outputs: [] });
  const [running, setRunning] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [reading, setReading] = useState<SessionStats>(emptyStats);
  const [driver, setDriver] = useState<DriverStatus>({ microphone: false, speaker: false });
  const [os, setOs] = useState("browser");
  const [login, setLogin] = useState(savedUi.current.start_on_login);
  const [settings, setSettings] = useState<Settings>({ ...defaultSettings, ...loadSettings() });
  const settingsRef = useRef(settings);
  const runningRef = useRef(false);
  settingsRef.current = settings;
  runningRef.current = running;

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    saveUi({ running, start_on_login: login, theme });
  }, [theme, running, login]);

  useEffect(() => {
    saveSettings(settings);
  }, [settings]);

  const apply = useCallback(async (next: Settings) => {
    setSettings(next);
    if (runningRef.current && connected) {
      try {
        await update(next);
      } catch (err) {
        setError(String(err));
      }
    }
  }, [connected]);

  const setEngine = useCallback(async (on: boolean) => {
    if (!connected) {
      setRunning(on);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      if (on) {
        await start(settingsRef.current);
        setRunning(true);
        await setTrayRunning(true);
      } else {
        await stop();
        setRunning(false);
        setReading(emptyStats);
        await setTrayRunning(false);
      }
    } catch (err) {
      setError(String(err));
      setRunning(false);
      await setTrayRunning(false).catch(() => undefined);
    } finally {
      setBusy(false);
    }
  }, [connected]);

  useEffect(() => {
    let cancel = false;
    listDevices()
      .then((found) => {
        if (cancel) return;
        setDevices(found);
        setSettings((current) => ({
          ...current,
          input_device: current.input_device ?? hardwareInput(found.inputs),
          output_device: current.output_device ?? hardwareOutput(found.outputs),
        }));
      })
      .catch((err: unknown) => setError(String(err)));
    hostOs().then((name) => {
      if (!cancel) setOs(name);
    }).catch(() => undefined);
    driverStatus().then((status) => {
      if (!cancel) setDriver(status);
    }).catch(() => undefined);
    if (connected && savedUi.current.start_on_login) {
      setOpenAtLogin(true).catch((err: unknown) => setError(String(err)));
    }
    if (connected && savedUi.current.running) {
      void setEngine(true);
    }
    return () => {
      cancel = true;
    };
  }, [connected, setEngine]);

  useEffect(() => {
    if (!connected) return;
    let unlisten: (() => void) | undefined;
    let stopListen = false;
    listenTray(() => {
      void setEngine(!runningRef.current);
    }).then((done) => {
      if (stopListen) done();
      else unlisten = done;
    }).catch(() => undefined);
    return () => {
      stopListen = true;
      unlisten?.();
    };
  }, [connected, setEngine]);

  useEffect(() => {
    if (!connected || !running) return;
    const timer = window.setInterval(() => {
      stats()
        .then((next) => {
          if (next) setReading(next);
        })
        .catch((err: unknown) => setError(String(err)));
      driverStatus().then(setDriver).catch(() => undefined);
    }, 200);
    return () => window.clearInterval(timer);
  }, [connected, running]);

  async function onLogin(next: boolean) {
    setLogin(next);
    try {
      await setOpenAtLogin(next);
    } catch (err) {
      setError(String(err));
    }
  }

  async function onInstall() {
    setNotice(null);
    setError(null);
    try {
      setNotice(await installDriver());
      setDriver(await driverStatus());
    } catch (err) {
      setError(String(err));
    }
  }

  const active = demo || running;
  const meter = active ? reading : emptyStats;
  const inputLevel = demo ? 0.62 : Math.min(1, meter.input_level * 4);
  const outputLevel = demo ? 0.28 : Math.min(1, meter.output_level * 4);
  const driverReady = demo || (driver.microphone && driver.speaker);
  const modelDelay =
    settings.ns_backend === "dpdf_net" || settings.ns_backend === "dpdf_net2"
      ? 50
      : settings.ns_backend === "deep_filter_net3"
        ? 30
        : settings.ns_backend === "rn_noise"
          ? 10
          : 0;
  const latency = !settings.enabled ? 0 : active && meter.frames > 0 ? meter.algorithmic_latency_ms : modelDelay;

  return (
    <main className="app-shell">
      <header className="topbar">
        <div>
          <p className="mark">HUSH</p>
          <h1>Noise cancellation for the call</h1>
        </div>
        <button className="theme" type="button" onClick={() => setTheme(theme === "dark" ? "light" : "dark")}>
          {theme === "dark" ? "Light" : "Dark"}
        </button>
      </header>

      {!driverReady && (
        <section className="banner">
          <div>
            <strong>Virtual driver is not installed.</strong>
            <p>HUSH Microphone and HUSH Speaker have to exist before a call app can use them.</p>
          </div>
          <Button onClick={() => void onInstall()} disabled={!connected}>
            Install driver
          </Button>
        </section>
      )}

      {os === "windows" && (
        <section className="banner warn">
          <div>
            <strong>Windows Test Mode.</strong>
            <p>
              Turn Secure Boot off, then in an elevated PowerShell run <code>bcdedit /set testsigning on</code> and reboot.
              The desktop shows Test Mode. The installer certificate is a test certificate, not an EV signature.
            </p>
          </div>
        </section>
      )}

      <section className="hero">
        <button
          className={active ? "power on" : "power"}
          type="button"
          aria-pressed={active}
          disabled={busy}
          onClick={() => void setEngine(!running)}
        >
          <span>{active ? "ON" : "OFF"}</span>
        </button>
        <div className="hero-copy">
          <p className={active ? "state on" : "state"}>{active ? "Active" : "Off"}</p>
          <h2>{active ? "HUSH is cleaning audio." : "HUSH is standing by."}</h2>
          <p className="lede">
            In Zoom, Meet, Discord, or Teams, select <strong>HUSH Microphone</strong> and <strong>HUSH Speaker</strong>.
          </p>
          <div className="readouts">
            <Readout label="CPU" value={active ? `${meter.cpu_percent.toFixed(1)}%` : "—"} />
            <Readout label="Latency" value={active && settings.enabled ? `${latency.toFixed(0)} ms` : "—"} />
            <Readout label="Frames" value={active ? String(meter.frames) : "0"} />
          </div>
        </div>
      </section>

      <section className="grid">
        <article className="panel">
          <h3>What to clean</h3>
          <Toggle
            label="Clean my microphone"
            detail="Outgoing voice. Off is a straight wire, with no model delay."
            checked={settings.enabled}
            onChange={(enabled) => void apply({ ...settings, enabled })}
          />
          <Toggle
            label="Clean incoming audio"
            detail="HUSH Speaker is denoised before it reaches your headphones."
            checked={settings.clean_incoming}
            onChange={(clean_incoming) => void apply({ ...settings, clean_incoming })}
          />
          <Toggle
            label="Echo cancellation"
            detail="WebRTC AEC3, using HUSH Speaker as the reference."
            checked={settings.aec_enabled}
            onChange={(aec_enabled) => void apply({ ...settings, aec_enabled })}
          />
          <Toggle
            label="Start on login"
            detail="Open HUSH when you sign in, and restore this on/off state."
            checked={login}
            onChange={(next) => void onLogin(next)}
          />
        </article>

        <article className="panel">
          <h3>Devices</h3>
          <DeviceField
            label="Your microphone"
            value={settings.input_device}
            options={devices.inputs.filter((name) => !/hush/i.test(name))}
            onChange={(input_device) => void apply({ ...settings, input_device })}
          />
          <DeviceField
            label="Your speakers"
            value={settings.output_device}
            options={devices.outputs.filter((name) => !/hush speaker/i.test(name))}
            onChange={(output_device) => void apply({ ...settings, output_device })}
          />
          <div className="meters">
            <Meter label="Input, before" level={inputLevel} />
            <Meter label="Output, after" level={outputLevel} />
          </div>
          <div className="space-y-2">
            <Label>Model</Label>
            <Select
              value={settings.ns_backend}
              onValueChange={(ns_backend) => void apply({ ...settings, ns_backend: ns_backend as NsBackend })}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="dpdf_net">DPDFNet8, best quality</SelectItem>
                <SelectItem value="dpdf_net2">DPDFNet2, low CPU</SelectItem>
                <SelectItem value="rn_noise">RNNoise, lightest</SelectItem>
                <SelectItem value="deep_filter_net3">DeepFilterNet3</SelectItem>
              </SelectContent>
            </Select>
            <p className="hint">DPDFNet8 is the default. DPDFNet2 keeps the same 50 ms delay and uses less CPU.</p>
          </div>
        </article>
      </section>

      {!connected && !demo && (
        <p className="hint pad">Preview in the browser. Processing, the tray, and driver install run inside the HUSH app.</p>
      )}
      {error && <p className="error pad">{error}</p>}
      {notice && <p className="hint pad">{notice}</p>}
      {meter.error && <p className="error pad">{meter.error}</p>}
    </main>
  );
}

function Readout({ label, value }: { label: string; value: string }) {
  return (
    <div className="readout">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function Meter({ label, level }: { label: string; level: number }) {
  return (
    <div className="meter">
      <div className="meter-label">
        <span>{label}</span>
        <span>{Math.round(level * 100)}</span>
      </div>
      <div className="meter-track">
        <div className="meter-fill" style={{ width: `${Math.round(level * 100)}%` }} />
      </div>
    </div>
  );
}

function Toggle({
  label,
  detail,
  checked,
  onChange,
}: {
  label: string;
  detail: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <div className="toggle">
      <div>
        <Label>{label}</Label>
        <p className="hint">{detail}</p>
      </div>
      <Switch checked={checked} onCheckedChange={onChange} />
    </div>
  );
}

function DeviceField({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string | null;
  options: string[];
  onChange: (value: string) => void;
}) {
  const current = value && options.includes(value) ? value : options[0] ?? "none";
  return (
    <div className="space-y-2">
      <Label>{label}</Label>
      <Select value={current} onValueChange={onChange}>
        <SelectTrigger>
          <SelectValue placeholder="No device" />
        </SelectTrigger>
        <SelectContent>
          {options.length === 0 && <SelectItem value="none">No device</SelectItem>}
          {options.map((name) => (
            <SelectItem key={name} value={name}>
              {name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}
