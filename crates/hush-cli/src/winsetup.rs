//! Install and remove the test-signed HUSH virtual audio driver.
//!
//! pnputil stages the package. SetupAPI creates the root device node, which
//! pnputil alone does not do for a ROOT\ hardware id. newdev then points that
//! node at the inf. No GPL devcon binary is shipped.

use std::path::{Path, PathBuf};
use std::process::Command;

const HARDWARE_ID: &str = "ROOT\\VirtualAudioDriver";

#[repr(C)]
#[derive(Clone, Copy)]
struct Guid {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

#[repr(C)]
struct DevInfoData {
    cb_size: u32,
    class_guid: Guid,
    dev_inst: u32,
    reserved: usize,
}

const MEDIA: Guid = Guid {
    data1: 0x4d36_e96c,
    data2: 0xe325,
    data3: 0x11ce,
    data4: [0xbf, 0xc1, 0x08, 0x00, 0x2b, 0xe1, 0x03, 0x18],
};

const DIGCF_PRESENT: u32 = 0x02;
const DICD_GENERATE_ID: u32 = 0x01;
const DIF_REGISTERDEVICE: u32 = 0x19;
const SPDRP_HARDWAREID: u32 = 0x01;
const INSTALLFLAG_FORCE: u32 = 0x01;
const INVALID_HANDLE: isize = -1;

#[link(name = "shell32")]
unsafe extern "system" {
    fn IsUserAnAdmin() -> i32;
}

#[link(name = "setupapi")]
unsafe extern "system" {
    fn SetupDiCreateDeviceInfoList(class: *const Guid, hwnd: isize) -> isize;
    fn SetupDiCreateDeviceInfoW(
        set: isize,
        name: *const u16,
        class: *const Guid,
        description: *const u16,
        hwnd: isize,
        flags: u32,
        data: *mut DevInfoData,
    ) -> i32;
    fn SetupDiSetDeviceRegistryPropertyW(
        set: isize,
        data: *mut DevInfoData,
        property: u32,
        buffer: *const u8,
        size: u32,
    ) -> i32;
    fn SetupDiCallClassInstaller(code: u32, set: isize, data: *mut DevInfoData) -> i32;
    fn SetupDiDestroyDeviceInfoList(set: isize) -> i32;
    fn SetupDiGetClassDevsW(class: *const Guid, enumerator: *const u16, hwnd: isize, flags: u32) -> isize;
    fn SetupDiEnumDeviceInfo(set: isize, index: u32, data: *mut DevInfoData) -> i32;
    fn SetupDiGetDeviceRegistryPropertyW(
        set: isize,
        data: *mut DevInfoData,
        property: u32,
        reg_type: *mut u32,
        buffer: *mut u8,
        size: u32,
        required: *mut u32,
    ) -> i32;
    fn SetupDiRemoveDevice(set: isize, data: *mut DevInfoData) -> i32;
    fn SetupUninstallOEMInfW(file: *const u16, flags: u32, reserved: *mut core::ffi::c_void) -> i32;
}

#[link(name = "newdev")]
unsafe extern "system" {
    fn UpdateDriverForPlugAndPlayDevicesW(
        hwnd: isize,
        hardware_id: *const u16,
        inf: *const u16,
        flags: u32,
        reboot: *mut i32,
    ) -> i32;
}

pub fn install(package: Option<&Path>) -> Result<String, String> {
    if unsafe { IsUserAnAdmin() } == 0 {
        return Err("driver install needs an elevated installer. Re-run HUSH-setup.exe, or open PowerShell as administrator and run `hush driver install`.".into());
    }
    if !test_signing_on()? {
        let secure = secure_boot_on();
        let mut message = String::from(
            "test signing is off, so Windows will not load HushVad. From an elevated PowerShell run `bcdedit /set testsigning on`, reboot, and run this install again.",
        );
        if secure {
            message.push_str(" Secure Boot is on. Turn it off in the UEFI firmware first, or the test signature is rejected.");
        }
        return Err(message);
    }
    let dir = locate_package(package)?;
    let inf = find_named(&dir, "virtualaudiodriver.inf")
        .ok_or_else(|| format!("no VirtualAudioDriver.inf in {}", dir.display()))?;
    if find_named(&dir, "virtualaudiodriver.sys").is_none() {
        return Err(format!("no VirtualAudioDriver.sys in {}", dir.display()));
    }
    if find_named(&dir, "virtualaudiodriver.cat").is_none() {
        return Err(format!("no VirtualAudioDriver.cat in {}", dir.display()));
    }
    if let Some(cer) = find_named(&dir, "hushtest.cer") {
        trust_certificate(&cer)?;
    }
    run("pnputil", &["/add-driver", &inf.display().to_string(), "/install"])?;
    if !hardware_present() {
        create_device_node()?;
    }
    let mut reboot = 0i32;
    let inf_wide: Vec<u16> = wide(inf.to_string_lossy().as_ref());
    let id_wide = wide(HARDWARE_ID);
    let updated = unsafe {
        UpdateDriverForPlugAndPlayDevicesW(0, id_wide.as_ptr(), inf_wide.as_ptr(), INSTALLFLAG_FORCE, &mut reboot)
    };
    if updated == 0 {
        return Err("the driver package is staged, but Windows did not bind it to ROOT\\VirtualAudioDriver. Reboot and run `hush driver install` again.".into());
    }
    register_login()?;
    let mut message = format!(
        "driver: installed from {}\nlogin: HUSH and the user-session bridge start at sign-in",
        dir.display()
    );
    if reboot != 0 {
        message.push_str("\nreboot: Windows asked for a restart before the endpoints appear");
    }
    Ok(message)
}

pub fn uninstall() -> Result<String, String> {
    if unsafe { IsUserAnAdmin() } == 0 {
        return Err("driver uninstall needs an elevated PowerShell.".into());
    }
    let removed = remove_devices();
    let packages = remove_packages()?;
    let _ = Command::new("reg")
        .args(["delete", r"HKLM\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "HUSH", "/f"])
        .status();
    let _ = Command::new("reg")
        .args([
            "delete",
            r"HKLM\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "HUSH Bridge",
            "/f",
        ])
        .status();
    Ok(format!("removed {removed} device node(s) and {packages} driver package(s); login entries cleared"))
}

fn test_signing_on() -> Result<bool, String> {
    let output = Command::new("bcdedit")
        .args(["/enum", "{current}"])
        .output()
        .map_err(|err| format!("bcdedit failed to start: {err}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("bcdedit exited {:?}: {err}", output.status.code()));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(flag_yes(&text, "testsigning"))
}

fn flag_yes(text: &str, key: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim().to_ascii_lowercase();
        line.starts_with(key) && line.contains("yes")
    })
}

fn secure_boot_on() -> bool {
    let Ok(output) = Command::new("powershell")
        .args(["-NoProfile", "-Command", "try { Confirm-SecureBootUEFI } catch { $false }"])
        .output()
    else {
        return false;
    };
    String::from_utf8_lossy(&output.stdout).to_ascii_lowercase().contains("true")
}

fn trust_certificate(cer: &Path) -> Result<(), String> {
    run("certutil", &["-addstore", "-f", "Root", &cer.display().to_string()])?;
    run("certutil", &["-addstore", "-f", "TrustedPublisher", &cer.display().to_string()])?;
    Ok(())
}

fn locate_package(package: Option<&Path>) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Some(path) = package {
        candidates.push(path.to_path_buf());
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("driver"));
            candidates.push(dir.join("resources").join("driver"));
        }
    }
    for dir in &candidates {
        if find_named(dir, "virtualaudiodriver.inf").is_some() {
            return Ok(dir.clone());
        }
    }
    Err(format!(
        "no driver package found. Pass --package, or put VirtualAudioDriver.inf next to hush.exe. Looked in {}",
        candidates.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join(", ")
    ))
}

fn find_named(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let file = entry.file_name();
        if file.to_string_lossy().eq_ignore_ascii_case(name) {
            return Some(entry.path());
        }
    }
    None
}

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| format!("{program} failed to start: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(format!("{program} exited {:?}: {stderr}{stdout}", output.status.code()))
}

fn create_device_node() -> Result<(), String> {
    unsafe {
        let set = SetupDiCreateDeviceInfoList(&MEDIA, 0);
        if set == 0 || set == INVALID_HANDLE {
            return Err("SetupDiCreateDeviceInfoList failed".into());
        }
        let mut data = DevInfoData {
            cb_size: std::mem::size_of::<DevInfoData>() as u32,
            class_guid: MEDIA,
            dev_inst: 0,
            reserved: 0,
        };
        let name = wide("HUSH");
        let description = wide("HUSH");
        let created = SetupDiCreateDeviceInfoW(set, name.as_ptr(), &MEDIA, description.as_ptr(), 0, DICD_GENERATE_ID, &mut data);
        if created == 0 {
            SetupDiDestroyDeviceInfoList(set);
            return Err("SetupDiCreateDeviceInfo failed".into());
        }
        let mut hardware: Vec<u16> = wide(HARDWARE_ID);
        hardware.push(0);
        let bytes = std::slice::from_raw_parts(hardware.as_ptr() as *const u8, hardware.len() * 2);
        let property = SetupDiSetDeviceRegistryPropertyW(
            set,
            &mut data,
            SPDRP_HARDWAREID,
            bytes.as_ptr(),
            bytes.len() as u32,
        );
        if property == 0 {
            SetupDiDestroyDeviceInfoList(set);
            return Err("could not set ROOT\\VirtualAudioDriver".into());
        }
        let registered = SetupDiCallClassInstaller(DIF_REGISTERDEVICE, set, &mut data);
        SetupDiDestroyDeviceInfoList(set);
        if registered == 0 {
            return Err("SetupDiCallClassInstaller did not register the HUSH device".into());
        }
    }
    Ok(())
}

fn hardware_present() -> bool {
    !matching_devices().is_empty()
}

fn remove_devices() -> usize {
    let set = unsafe { SetupDiGetClassDevsW(&MEDIA, std::ptr::null(), 0, DIGCF_PRESENT) };
    if set == 0 || set == INVALID_HANDLE {
        return 0;
    }
    let mut removed = 0usize;
    let mut index = 0u32;
    loop {
        let mut data = DevInfoData {
            cb_size: std::mem::size_of::<DevInfoData>() as u32,
            class_guid: MEDIA,
            dev_inst: 0,
            reserved: 0,
        };
        if unsafe { SetupDiEnumDeviceInfo(set, index, &mut data) } == 0 {
            break;
        }
        if hardware_id_matches(set, &mut data) && unsafe { SetupDiRemoveDevice(set, &mut data) } != 0 {
            removed += 1;
        }
        index += 1;
    }
    unsafe { SetupDiDestroyDeviceInfoList(set) };
    removed
}

fn matching_devices() -> Vec<u32> {
    let set = unsafe { SetupDiGetClassDevsW(&MEDIA, std::ptr::null(), 0, DIGCF_PRESENT) };
    if set == 0 || set == INVALID_HANDLE {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut index = 0u32;
    loop {
        let mut data = DevInfoData {
            cb_size: std::mem::size_of::<DevInfoData>() as u32,
            class_guid: MEDIA,
            dev_inst: 0,
            reserved: 0,
        };
        if unsafe { SetupDiEnumDeviceInfo(set, index, &mut data) } == 0 {
            break;
        }
        if hardware_id_matches(set, &mut data) {
            found.push(index);
        }
        index += 1;
    }
    unsafe { SetupDiDestroyDeviceInfoList(set) };
    found
}

fn hardware_id_matches(set: isize, data: &mut DevInfoData) -> bool {
    let mut buffer = vec![0u8; 1024];
    let mut required = 0u32;
    let ok = unsafe {
        SetupDiGetDeviceRegistryPropertyW(
            set,
            data,
            SPDRP_HARDWAREID,
            std::ptr::null_mut(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            &mut required,
        )
    };
    if ok == 0 {
        return false;
    }
    let wide = buffer
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect::<Vec<_>>();
    String::from_utf16_lossy(&wide)
        .to_ascii_lowercase()
        .contains("virtualaudiodriver")
}

fn remove_packages() -> Result<usize, String> {
    let mut removed = 0usize;
    let inf_dir = PathBuf::from(r"C:\Windows\INF");
    let entries = std::fs::read_dir(&inf_dir).map_err(|err| format!("cannot read {}: {err}", inf_dir.display()))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.to_ascii_lowercase().starts_with("oem") || !name.to_ascii_lowercase().ends_with(".inf") {
            continue;
        }
        if !inf_mentions_driver(&entry.path()) {
            continue;
        }
        let wide_name = wide(&name);
        let ok = unsafe { SetupUninstallOEMInfW(wide_name.as_ptr(), 1, std::ptr::null_mut()) };
        if ok == 0 {
            let _ = Command::new("pnputil")
                .args(["/delete-driver", name.as_ref(), "/uninstall", "/force"])
                .status();
        }
        removed += 1;
    }
    Ok(removed)
}

fn inf_mentions_driver(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let text = if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    };
    let text = text.to_ascii_lowercase();
    text.contains("virtualaudiodriver") || text.contains("hushvad")
}

fn register_login() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    let dir = exe.parent().ok_or("hush.exe has no directory")?;
    let gui = dir.join("HUSH.exe");
    let gui_cmd = if gui.is_file() {
        format!("\"{}\"", gui.display())
    } else {
        format!("\"{}\"", exe.display())
    };
    let bridge = format!("\"{}\" service", exe.display());
    reg_add("HUSH", &gui_cmd)?;
    reg_add("HUSH Bridge", &bridge)?;
    Ok(())
}

fn reg_add(name: &str, value: &str) -> Result<(), String> {
    run(
        "reg",
        &[
            "add",
            r"HKLM\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            name,
            "/t",
            "REG_SZ",
            "/d",
            value,
            "/f",
        ],
    )
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
