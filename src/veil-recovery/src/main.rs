#![windows_subsystem = "windows"]

use std::path::PathBuf;
use veil_engine::{RecoveryOptions, RecoverySession, Win32CcdApi, Win32Hotkey};

fn main() {
    let code = run(std::env::args().skip(1).collect());
    std::process::exit(code);
}

fn run(args: Vec<String>) -> i32 {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!("Veil.Recovery --directory <sessionDir> --parent-pid <pid>");
        return 0;
    }
    let mut directory = None;
    let mut parent_pid = 0;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--directory" && i + 1 < args.len() {
            directory = Some(args[i + 1].clone());
            i += 2;
            continue;
        }
        if args[i] == "--parent-pid" && i + 1 < args.len() {
            parent_pid = args[i + 1].parse().unwrap_or(0);
            i += 2;
            continue;
        }
        i += 1;
    }
    let Some(directory) = directory.filter(|s| !s.trim().is_empty()) else {
        return 2;
    };
    let dir = PathBuf::from(&directory);
    let _ = std::fs::create_dir_all(&dir);
    let mut options =
        RecoveryOptions::defaults(dir, Box::new(Win32CcdApi), Box::new(Win32Hotkey::default()));
    options.parent_pid = parent_pid;
    options.restore_only = args.iter().any(|a| a == "--restore-only");
    let mut session = RecoverySession::new(options);
    session.run_until_exit(None);
    if session.result.ok {
        0
    } else {
        1
    }
}
