//! Download VB-CABLE from VB-Audio. The zip is not stored in this repository.

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::thread;
use std::time::{Duration, Instant};

use hush_core::{sha256_hex, PACK_SHA256, PACK_URL};

pub fn install(noninteractive: bool) -> ExitCode {
    println!("VB-CABLE is donationware by VB-Audio Software.");
    println!("https://vb-audio.com/Cable/");
    println!("Donate: https://vb-audio.com/Services/donate.htm");
    println!("HUSH does not redistribute the driver. It downloads the official Pack45 zip and checks SHA-256 {PACK_SHA256}.");

    if cable_render_present() {
        println!("VB-CABLE is already installed. HUSH will share that one cable with OBS and any other app that uses CABLE Input.");
        let _ = rename_capture();
        return ExitCode::SUCCESS;
    }

    let dir = std::env::temp_dir().join("hush-vbcable");
    let _ = fs::create_dir_all(&dir);
    let _ = save_defaults(&dir);
    let setup = match ensure_setup() {
        Ok(setup) => setup,
        Err(err) => {
            eprintln!("hush: {err}");
            return ExitCode::from(1);
        }
    };

    println!("Running VBCABLE_Setup_x64.exe -i -h");
    let silent = run_setup(&setup, &["-i", "-h"], Duration::from_secs(if noninteractive { 25 } else { 90 }));
    match silent {
        SetupResult::Finished(code) if code == 0 => {
            println!("VB-Audio's setup exited 0 with -i -h.");
        }
        SetupResult::Finished(code) => {
            println!("VB-Audio's setup exited {code} with -i -h.");
            if !noninteractive {
                println!("Opening VB-Audio's setup window.");
                let _ = Command::new(&setup).status();
            }
        }
        SetupResult::TimedOut => {
            println!("-i -h did not finish. Those flags are not a documented silent install.");
            if noninteractive {
                println!("Noninteractive mode will not open the VB-Audio window.");
            } else {
                println!("Opening VB-Audio's setup window.");
                let _ = Command::new(&setup).status();
            }
        }
        SetupResult::Failed(err) => {
            eprintln!("hush: could not start the setup: {err}");
            return ExitCode::from(1);
        }
    }

    thread::sleep(Duration::from_secs(2));
    let _ = rename_capture();
    let _ = restore_defaults(&dir);
    if cable_render_present() {
        println!("CABLE Input is available. In Zoom, Meet, Discord, or Teams select HUSH Microphone.");
        println!("One cable is shared with OBS and similar apps. Only one program should play into CABLE Input.");
        ExitCode::SUCCESS
    } else {
        println!("VB-CABLE is not visible yet. Reboot once. Secure Boot can stay on. Test Mode is not required.");
        ExitCode::from(10)
    }
}

pub fn uninstall() -> ExitCode {
    println!("Removing VB-CABLE uses VB-Audio's own setup. Other apps such as OBS may be using the cable.");
    let setup = match ensure_setup() {
        Ok(setup) => setup,
        Err(err) => {
            eprintln!("hush: {err}");
            return ExitCode::from(1);
        }
    };
    println!("Running VBCABLE_Setup_x64.exe -u -h");
    match run_setup(&setup, &["-u", "-h"], Duration::from_secs(45)) {
        SetupResult::Finished(0) => ExitCode::SUCCESS,
        _ => {
            println!("Opening VB-Audio's setup window. Choose Remove, then reboot.");
            let _ = Command::new(&setup).status();
            ExitCode::SUCCESS
        }
    }
}

enum SetupResult {
    Finished(i32),
    TimedOut,
    Failed(String),
}

fn run_setup(setup: &std::path::Path, args: &[&str], limit: Duration) -> SetupResult {
    let mut child = match Command::new(setup).args(args).spawn() {
        Ok(child) => child,
        Err(err) => return SetupResult::Failed(err.to_string()),
    };
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return SetupResult::Finished(status.code().unwrap_or(1)),
            Ok(None) if started.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                return SetupResult::TimedOut;
            }
            Ok(None) => thread::sleep(Duration::from_millis(400)),
            Err(err) => return SetupResult::Failed(err.to_string()),
        }
    }
}

fn cable_render_present() -> bool {
    hush_core::list_devices()
        .map(|lists| lists.outputs.iter().any(|name| hush_core::is_cable_render(name)))
        .unwrap_or(false)
}

fn rename_capture() -> ExitCode {
    powershell(&["-Action", "Rename"])
}

fn save_defaults(dir: &std::path::Path) -> ExitCode {
    let path = dir.join("defaults.txt");
    let text = path.display().to_string();
    powershell(&["-Action", "Save", "-Path", &text])
}

fn restore_defaults(dir: &std::path::Path) -> ExitCode {
    let path = dir.join("defaults.txt");
    if !path.is_file() {
        return ExitCode::SUCCESS;
    }
    let text = path.display().to_string();
    powershell(&["-Action", "Restore", "-Path", &text])
}

fn ensure_setup() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("hush-vbcable");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let zip = dir.join("VBCABLE_Driver_Pack45.zip");
    let status = Command::new("curl.exe")
        .args(["-fL", "--retry", "3", "-o"])
        .arg(&zip)
        .arg(PACK_URL)
        .status()
        .map_err(|err| format!("curl.exe: {err}"))?;
    if !status.success() {
        return Err(format!("download failed ({status})"));
    }
    let mut file = fs::File::open(&zip).map_err(|err| err.to_string())?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|err| err.to_string())?;
    let digest = sha256_hex(&bytes);
    if digest != PACK_SHA256 {
        let _ = fs::remove_file(&zip);
        return Err(format!("VB-CABLE hash mismatch\n  expected {PACK_SHA256}\n  got      {digest}"));
    }
    println!("VB-CABLE SHA-256 matches the pinned Pack45 hash.");
    let unpacked = dir.join("unpacked");
    let _ = fs::remove_dir_all(&unpacked);
    fs::create_dir_all(&unpacked).map_err(|err| err.to_string())?;
    // Windows PowerShell 7's Expand-Archive fails on this zip. tar.exe reads it.
    let expanded = Command::new("tar")
        .args(["-xf"])
        .arg(&zip)
        .arg("-C")
        .arg(&unpacked)
        .status()
        .map_err(|err| format!("tar: {err}"))?;
    if !expanded.success() {
        return Err("could not unzip VB-CABLE".into());
    }
    find_setup(&unpacked).ok_or_else(|| "VBCABLE_Setup_x64.exe was not in the zip".to_string())
}

fn find_setup(dir: &std::path::Path) -> Option<PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = fs::read_dir(&current).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.file_name().and_then(|name| name.to_str()) == Some("VBCABLE_Setup_x64.exe") {
                return Some(path);
            }
        }
    }
    None
}

fn powershell(args: &[&str]) -> ExitCode {
    let script = std::env::temp_dir().join("hush-vbcable-endpoint.ps1");
    if fs::write(&script, include_str!("../../../scripts/windows/vbcable-endpoint.ps1")).is_err() {
        eprintln!("hush: could not write the endpoint script");
        return ExitCode::from(1);
    }
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .args(args)
        .status();
    match status {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => {
            eprintln!("hush: endpoint script exited {status}");
            ExitCode::from(1)
        }
        Err(err) => {
            eprintln!("hush: powershell: {err}");
            ExitCode::from(1)
        }
    }
}
