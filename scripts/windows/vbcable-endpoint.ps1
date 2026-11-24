# Rename the VB-CABLE capture endpoint and keep the user's default devices.
# The cable installer likes to become the default. HUSH puts the previous
# defaults back. It does not edit the driver files.
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Save", "Restore", "Rename")]
    [string]$Action,
    [string]$Path
)

$ErrorActionPreference = "Stop"

$source = @"
using System;
using System.Runtime.InteropServices;

public static class HushEndpoint {
    public static string DefaultId(int dataFlow) {
        object enumerator = new MMDeviceEnumeratorComObject();
        var face = (IMMDeviceEnumerator)enumerator;
        IMMDevice device;
        int hr = face.GetDefaultAudioEndpoint(dataFlow, 0, out device);
        if (hr != 0 || device == null) return "";
        string id;
        device.GetId(out id);
        return id ?? "";
    }

    public static void SetDefault(string id) {
        if (string.IsNullOrEmpty(id)) return;
        var policy = (IPolicyConfig)new PolicyConfigClient();
        for (int role = 0; role < 3; role++) {
            policy.SetDefaultEndpoint(id, role);
        }
    }

    public static int RenameCapture(string fromPart, string toName) {
        object enumerator = new MMDeviceEnumeratorComObject();
        var face = (IMMDeviceEnumerator)enumerator;
        IMMDeviceCollection devices;
        face.EnumAudioEndpoints(1, 1, out devices);
        int count;
        devices.GetCount(out count);
        int renamed = 0;
        var key = new PropertyKey();
        key.fmtid = new Guid("a45c254e-df1c-4efd-8020-67d146a850e0");
        key.pid = 14;
        for (int i = 0; i < count; i++) {
            IMMDevice device;
            devices.Item(i, out device);
            IPropertyStore store;
            device.OpenPropertyStore(0, out store);
            PropVariant value;
            store.GetValue(ref key, out value);
            string name = value.GetString();
            if (name == null || name.IndexOf(fromPart, StringComparison.OrdinalIgnoreCase) < 0) continue;
            device.OpenPropertyStore(2, out store);
            var next = PropVariant.FromString(toName);
            store.SetValue(ref key, ref next);
            store.Commit();
            renamed++;
        }
        return renamed;
    }
}

[ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")]
class MMDeviceEnumeratorComObject { }

[Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IMMDeviceEnumerator {
    int EnumAudioEndpoints(int dataFlow, int dwStateMask, out IMMDeviceCollection ppDevices);
    int GetDefaultAudioEndpoint(int dataFlow, int role, out IMMDevice ppEndpoint);
    int GetDevice([MarshalAs(UnmanagedType.LPWStr)] string pwstrId, out IMMDevice ppDevice);
}

[Guid("0BD7A1BE-7A1A-44DB-8397-CC5392387B5E"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IMMDeviceCollection {
    int GetCount(out int pcDevices);
    int Item(int nDevice, out IMMDevice ppDevice);
}

[Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IMMDevice {
    int Activate(ref Guid iid, int dwClsCtx, IntPtr pActivationParams, [MarshalAs(UnmanagedType.IUnknown)] out object ppInterface);
    int OpenPropertyStore(int stgmAccess, out IPropertyStore ppProperties);
    int GetId([MarshalAs(UnmanagedType.LPWStr)] out string ppstrId);
    int GetState(out int pdwState);
}

[Guid("886d8eeb-8cf2-4446-8d02-cdba1dbdcf99"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IPropertyStore {
    int GetCount(out int cProps);
    int GetAt(int iProp, out PropertyKey pkey);
    int GetValue(ref PropertyKey key, out PropVariant pv);
    int SetValue(ref PropertyKey key, ref PropVariant pv);
    int Commit();
}

[StructLayout(LayoutKind.Sequential)]
public struct PropertyKey {
    public Guid fmtid;
    public uint pid;
}

[StructLayout(LayoutKind.Explicit)]
public struct PropVariant {
    [FieldOffset(0)] public ushort vt;
    [FieldOffset(8)] public IntPtr pointer;

    public static PropVariant FromString(string value) {
        var variant = new PropVariant();
        variant.vt = 31;
        variant.pointer = Marshal.StringToCoTaskMemUni(value);
        return variant;
    }

    public string GetString() {
        if (vt != 31 || pointer == IntPtr.Zero) return null;
        return Marshal.PtrToStringUni(pointer);
    }
}

[Guid("f8679f50-850a-41cf-9c72-430f290290c8"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IPolicyConfig {
    int Unused1(); int Unused2(); int Unused3(); int Unused4();
    int Unused5(); int Unused6(); int Unused7(); int Unused8();
    int Unused9(); int Unused10();
    int SetDefaultEndpoint([MarshalAs(UnmanagedType.LPWStr)] string deviceId, int role);
}

[ComImport, Guid("870af99c-171d-4f9e-af0d-e63df40c2bc9")]
class PolicyConfigClient { }
"@

Add-Type -TypeDefinition $source -Language CSharp | Out-Null

switch ($Action) {
    "Save" {
        $render = [HushEndpoint]::DefaultId(0)
        $capture = [HushEndpoint]::DefaultId(1)
        Set-Content -Path $Path -Value @($render, $capture) -Encoding ascii
        Write-Host "saved default playback $render"
        Write-Host "saved default recording $capture"
    }
    "Restore" {
        $lines = @(Get-Content -Path $Path)
        if ($lines.Count -ge 1 -and $lines[0]) { [HushEndpoint]::SetDefault($lines[0]) }
        if ($lines.Count -ge 2 -and $lines[1]) { [HushEndpoint]::SetDefault($lines[1]) }
        Write-Host "restored the previous default playback and recording devices"
    }
    "Rename" {
        $count = [HushEndpoint]::RenameCapture("CABLE Output", "HUSH Microphone")
        Write-Host "renamed $count VB-CABLE capture endpoint(s) to HUSH Microphone"
    }
}
