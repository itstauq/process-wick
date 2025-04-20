use clap::Parser;
use log::{info, warn};
use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[cfg(unix)]
use nix::sys::signal::{kill, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

#[cfg(windows)]
use std::mem::{size_of, zeroed};

#[cfg(windows)]
use winapi::um::{
    handleapi::CloseHandle,
    processthreadsapi::{GetCurrentProcessId, OpenProcess},
    tlhelp32::{
        CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
    },
    winnt::{HANDLE, PROCESS_QUERY_LIMITED_INFORMATION},
};

#[derive(Parser, Debug)]
#[command(name = "process-wick")]
#[command(about = "The John Wick of processes — Kill dangling processes when the parent dies 🔫💥", long_about = None)]
struct Args {
    /// PID of the process to watch (the dog). If not provided, defaults to parent PID.
    #[arg(long)]
    dog: Option<u32>,

    /// PIDs of the processes to kill when the dog dies.
    #[arg(long, required = true)]
    targets: Vec<u32>,

    /// Time in seconds to wait after SIGTERM before force-killing.
    #[arg(long, default_value = "5")]
    vengeance_delay: u64,

    /// Time in seconds between each check on the dog.
    #[arg(long, default_value = "3")]
    tick: u64,

    /// Path to the log file. If not provided, logs will only be printed to console.
    #[arg(long)]
    log_file: Option<String>,

    /// Log level (error, warn, info, debug, trace). Default: info
    #[arg(long, default_value = "info")]
    log_level: String,
}

fn get_dog_pid(dog_arg: Option<u32>) -> u32 {
    dog_arg.unwrap_or_else(|| {
        #[cfg(unix)]
        {
            unsafe { libc::getppid() as u32 }
        }
        #[cfg(windows)]
        unsafe {
            let current_pid = GetCurrentProcessId();
            let snapshot: HANDLE = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot.is_null() {
                return 0;
            }

            let mut entry: PROCESSENTRY32 = zeroed();
            entry.dwSize = size_of::<PROCESSENTRY32>() as u32;

            let mut found = false;
            if Process32First(snapshot, &mut entry) != 0 {
                loop {
                    if entry.th32ProcessID == current_pid {
                        found = true;
                        break;
                    }
                    if Process32Next(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snapshot);

            if found {
                entry.th32ParentProcessID
            } else {
                0
            }
        }
    })
}

fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        kill(Pid::from_raw(pid as i32), None).is_ok()
    }

    #[cfg(windows)]
    {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return false;
            }
            CloseHandle(handle);
            true
        }
    }
}

#[cfg(unix)]
fn send_signal(pid: u32, force: bool) {
    let sig = if force {
        Signal::SIGKILL
    } else {
        Signal::SIGTERM
    };
    let _ = kill(Pid::from_raw(pid as i32), sig);
}

#[cfg(windows)]
fn send_signal(pid: u32, force: bool) {
    use std::process::Command;

    let mut cmd = Command::new("taskkill");
    cmd.args(&["/PID", &pid.to_string(), "/T"]); // /T kills child processes

    if force {
        cmd.arg("/F"); // Force kill
    }

    let _ = cmd.output(); // Ignore result for now
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Initialize logging
    let mut logger = env_logger::Builder::from_default_env();

    // Set the default log level
    logger.filter_level(match args.log_level.to_lowercase().as_str() {
        "error" => log::LevelFilter::Error,
        "warn" => log::LevelFilter::Warn,
        "info" => log::LevelFilter::Info,
        "debug" => log::LevelFilter::Debug,
        "trace" => log::LevelFilter::Trace,
        _ => log::LevelFilter::Info,
    });

    logger.format(|buf, record| {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        writeln!(
            buf,
            "[{}] {} - {}",
            timestamp,
            record.level(),
            record.args()
        )
    });

    // If log file is specified, write to both file and console
    if let Some(log_path) = &args.log_file {
        let log_file = File::create(log_path).expect("Failed to create log file");
        let log_file = Box::new(log_file);
        logger.target(env_logger::Target::Pipe(log_file));
    }

    logger.init();

    let dog_pid = get_dog_pid(args.dog);
    let targets: HashSet<u32> = args.targets.into_iter().collect();

    info!("🐶 Watching dog PID: {}", dog_pid);
    info!("🎯 Targets: {:?}", targets);
    info!(
        "⏳ Tick every {}s, vengeance in {}s",
        args.tick, args.vengeance_delay
    );

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    // Watch loop
    tokio::spawn(async move {
        loop {
            if !is_process_alive(dog_pid) {
                warn!("💀 Dog died. Unleashing vengeance.");
                for &pid in &targets {
                    info!("⚠️ Sending SIGTERM to PID {}", pid);
                    send_signal(pid, false); // graceful termination
                }

                tokio::time::sleep(Duration::from_secs(args.vengeance_delay)).await;

                for &pid in &targets {
                    if is_process_alive(pid) {
                        warn!("🔪 Forcing kill on PID {}", pid);
                        send_signal(pid, true); // force termination
                    }
                }

                info!("🧘 Process-wick retires in peace.");
                r.store(false, Ordering::SeqCst);
                break;
            }

            tokio::time::sleep(Duration::from_secs(args.tick)).await;
        }
    });

    // Wait until done
    while running.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
