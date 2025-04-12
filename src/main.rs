use clap::Parser;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[cfg(unix)]
use nix::sys::signal::{kill, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

#[cfg(windows)]
use winapi::um::handleapi::CloseHandle;
#[cfg(windows)]
use winapi::um::processthreadsapi::OpenProcess;
#[cfg(windows)]
use winapi::um::winnt::PROCESS_QUERY_LIMITED_INFORMATION;

#[derive(Parser, Debug)]
#[command(name = "process-wick")]
#[command(about = "Revenge-driven process watcher", long_about = None)]
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
}

fn get_dog_pid(dog_arg: Option<u32>) -> u32 {
    dog_arg.unwrap_or_else(|| {
        #[cfg(unix)]
        {
            unsafe { libc::getppid() as u32 }
        }
        #[cfg(windows)]
        {
            // Windows fallback to get parent PID
            unsafe { process::id() }
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
    let dog_pid = get_dog_pid(args.dog);
    let targets: HashSet<u32> = args.targets.into_iter().collect();

    println!("🐶 Watching dog PID: {}", dog_pid);
    println!("🎯 Targets: {:?}", targets);
    println!(
        "⏳ Tick every {}s, vengeance in {}s",
        args.tick, args.vengeance_delay
    );

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    // Watch loop
    tokio::spawn(async move {
        loop {
            if !is_process_alive(dog_pid) {
                println!("💀 Dog died. Unleashing vengeance.");
                for &pid in &targets {
                    println!("⚠️ Sending SIGTERM to PID {}", pid);
                    send_signal(pid, false); // graceful termination
                }

                tokio::time::sleep(Duration::from_secs(args.vengeance_delay)).await;

                for &pid in &targets {
                    if is_process_alive(pid) {
                        println!("🔪 Forcing kill on PID {}", pid);
                        send_signal(pid, true); // force termination
                    }
                }

                println!("🧘 Process-wick retires in peace.");
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
