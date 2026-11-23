import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import {
  listDevices,
  shellConnected,
  start,
  stats,
  stop,
  update,
  type DeviceLists,
  type NsBackend,
  type SessionStats,
  type Settings,
} from "@/lib/api";

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
  ns_backend: "deep_filter_net3",
  strength: 1,
  aec_enabled: true,
  enabled: false,
  error: null,
};

export function App() {
  const connected = shellConnected();
  const [devices, setDevices] = useState<DeviceLists>({ inputs: [], outputs: [] });
  const [live, setLive] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reading, setReading] = useState<SessionStats>(emptyStats);
  const [settings, setSettings] = useState<Settings>({
    input_device: null,
    output_device: null,
    far_device: null,
    enabled: true,
    aec_enabled: true,
    ns_backend: "deep_filter_net3",
    strength: 1,
    monitor_cleaned: false,
  });

  useEffect(() => {
    listDevices()
      .then((found) => {
        setDevices(found);
        setSettings((current) => ({
          ...current,
          input_device: current.input_device ?? found.inputs[0] ?? null,
          output_device: current.output_device ?? found.outputs[0] ?? null,
        }));
      })
      .catch((err: unknown) => setError(String(err)));
  }, []);

  useEffect(() => {
    if (!connected || !live) return;
    const timer = window.setInterval(() => {
      stats()
        .then((next) => {
          if (next) setReading(next);
        })
        .catch((err: unknown) => setError(String(err)));
    }, 200);
    return () => window.clearInterval(timer);
  }, [connected, live]);

  async function apply(next: Settings) {
    setSettings(next);
    if (live && connected) {
      try {
        await update(next);
      } catch (err) {
        setError(String(err));
      }
    }
  }

  async function toggleRun() {
    setError(null);
    try {
      if (live) {
        await stop();
        setLive(false);
        return;
      }
      await start(settings);
      setLive(true);
    } catch (err) {
      setError(String(err));
      setLive(false);
    }
  }

  const meter = live ? reading : emptyStats;

  return (
    <main className="mx-auto grid max-w-5xl gap-6 px-4 py-8 md:grid-cols-[1.4fr_0.8fr] md:px-8">
      <section className="space-y-6">
        <header>
          <p className="text-xs uppercase tracking-[0.22em] text-[#c47a3a]">Hush</p>
          <h1 className="mt-2 font-serif text-4xl text-[#efe6da]">The mic your call actually hears.</h1>
          <p className="mt-3 max-w-xl text-[#efe6da]/70">
            Echo cancellation, then DeepFilterNet3. Point Zoom, Meet, Discord, or Teams at Krisp Alt Microphone.
            Krisp Alt Speaker is the far end, played back on the device you pick below.
          </p>
          {!connected && (
            <p className="mt-3 text-sm text-[#e2c16b]">
              Preview in the browser. Processing starts when this page is opened inside the Hush app.
            </p>
          )}
        </header>

        <Card className="space-y-5">
          <DeviceField
            label="Microphone"
            value={settings.input_device}
            options={devices.inputs}
            onChange={(input_device) => void apply({ ...settings, input_device })}
          />
          <DeviceField
            label="Play the call on"
            value={settings.output_device}
            options={devices.outputs}
            onChange={(output_device) => void apply({ ...settings, output_device })}
          />
          <DeviceField
            label="Far-end override"
            value={settings.far_device}
            options={["", ...devices.inputs]}
            allowEmpty
            onChange={(far_device) => void apply({ ...settings, far_device: far_device || null })}
          />
          <div className="flex items-center justify-between gap-4">
            <div>
              <Label>Processing</Label>
              <p className="text-xs text-[#efe6da]/50">Off is a straight wire. No model delay.</p>
            </div>
            <Switch checked={settings.enabled} onCheckedChange={(enabled) => void apply({ ...settings, enabled })} />
          </div>
          <div className="flex items-center justify-between gap-4">
            <div>
              <Label>Echo cancellation</Label>
              <p className="text-xs text-[#efe6da]/50">WebRTC AEC3, using Krisp Alt Speaker as the reference.</p>
            </div>
            <Switch
              checked={settings.aec_enabled}
              onCheckedChange={(aec_enabled) => void apply({ ...settings, aec_enabled })}
            />
          </div>
          <div className="flex items-center justify-between gap-4">
            <div>
              <Label>Sidetone</Label>
              <p className="text-xs text-[#efe6da]/50">A little of your cleaned voice in the speakers.</p>
            </div>
            <Switch
              checked={settings.monitor_cleaned}
              onCheckedChange={(monitor_cleaned) => void apply({ ...settings, monitor_cleaned })}
            />
          </div>
          <div className="space-y-2">
            <Label>Suppressor</Label>
            <Select
              value={settings.ns_backend}
              onValueChange={(ns_backend) => void apply({ ...settings, ns_backend: ns_backend as NsBackend })}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="deep_filter_net3">DeepFilterNet3</SelectItem>
                <SelectItem value="rn_noise">RNNoise, low CPU</SelectItem>
                <SelectItem value="off">Off</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <Label>Suppression strength</Label>
              <span className="text-sm tabular-nums text-[#efe6da]/70">{Math.round(settings.strength * 100)}</span>
            </div>
            <Slider
              min={0}
              max={1}
              step={0.01}
              value={[settings.strength]}
              onValueChange={([strength]) => void apply({ ...settings, strength })}
            />
          </div>
          <Button size="lg" onClick={() => void toggleRun()} disabled={!connected && false}>
            {live ? "Stop" : "Start"}
          </Button>
          {error && <p className="text-sm text-[#e07a5f]">{error}</p>}
        </Card>
      </section>

      <aside className="space-y-4">
        <Card>
          <p className="text-xs uppercase tracking-[0.18em] text-[#efe6da]/50">This frame</p>
          <p className="mt-3 font-serif text-5xl tabular-nums">{meter.cpu_percent.toFixed(1)}%</p>
          <p className="text-sm text-[#efe6da]/60">of one core, {meter.process_ns / 1e6} ms of processing</p>
          <dl className="mt-6 space-y-3 text-sm">
            <Row label="Model delay" value={`${meter.algorithmic_latency_ms.toFixed(1)} ms`} />
            <Row label="Buffer" value={`${meter.buffer_ms.toFixed(1)} ms`} />
            <Row label="Frames" value={String(meter.frames)} />
            <Row label="Mic underruns" value={String(meter.ring_underruns)} />
            <Row label="Output underruns" value={String(meter.output_underruns)} />
          </dl>
        </Card>
        <Card>
          <p className="text-sm leading-6 text-[#efe6da]/75">
            Install the audio driver, then choose Krisp Alt Microphone and Krisp Alt Speaker in the call app. Hush has
            to be running for the speaker to reach your headphones. That bridge is also the echo reference.
          </p>
        </Card>
      </aside>
    </main>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <dt className="text-[#efe6da]/50">{label}</dt>
      <dd className="tabular-nums">{value}</dd>
    </div>
  );
}

function DeviceField({
  label,
  value,
  options,
  onChange,
  allowEmpty = false,
}: {
  label: string;
  value: string | null;
  options: string[];
  onChange: (value: string) => void;
  allowEmpty?: boolean;
}) {
  const current = value ?? (allowEmpty ? "" : options[0] ?? "");
  return (
    <div className="space-y-2">
      <Label>{label}</Label>
      <Select value={current || "none"} onValueChange={(next) => onChange(next === "none" ? "" : next)}>
        <SelectTrigger>
          <SelectValue placeholder="No device" />
        </SelectTrigger>
        <SelectContent>
          {allowEmpty && <SelectItem value="none">Virtual speaker</SelectItem>}
          {options.filter(Boolean).map((name) => (
            <SelectItem key={name} value={name}>
              {name}
            </SelectItem>
          ))}
          {options.filter(Boolean).length === 0 && <SelectItem value="none">No device</SelectItem>}
        </SelectContent>
      </Select>
    </div>
  );
}
