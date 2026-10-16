#![forbid(unsafe_code)]
use photocraft_depth_studio_native::{Result, engine, error, protocol};
use std::{
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--job")) {
        return Err(error("Expected --job <request.json>"));
    }
    let path = args.next().ok_or_else(|| error("Missing job request."))?;
    if args.next().is_some() {
        return Err(error("Unexpected worker argument."));
    }
    let job = protocol::parse_job(std::fs::File::open(path)?)?;
    let executable = std::env::current_exe()?;
    let runtime = executable
        .parent()
        .ok_or_else(|| error("Cannot locate plugin installation."))?
        .join("runtime");
    let cancel = Arc::new(AtomicBool::new(false));
    let lifetime = Arc::clone(&cancel);
    std::thread::Builder::new()
        .name("depth-host-lifetime".into())
        .spawn(move || {
            let mut byte = [0u8; 1];
            let _ = std::io::stdin().read(&mut byte);
            lifetime.store(true, Ordering::Release);
        })?;
    engine::process(&job, &runtime, &cancel, &|progress, message| {
        eprintln!(
            "{}",
            serde_json::json!({"progress":progress,"message":message})
        );
    })
}

fn main() {
    if let Err(failure) = run() {
        let message: String = failure.to_string().chars().take(2048).collect();
        eprintln!("{}", serde_json::json!({"error":message}));
        std::process::exit(1);
    }
}
