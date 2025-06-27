use clap::Parser;
use log::{info, warn};
use std::fs::File;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use process_wick::{
    build_process_tree, get_dog_pid, get_pids_by_depth, is_process_alive, kill_process_group,
    parse_target_pids, send_signal_to_pids,
};

#[derive(Parser, Debug)]
#[command(name = "process-wick")]
#[command(about = "The John Wick of processes — Kill dangling processes when the parent dies 🔫💥", long_about = None)]
struct Args {
    /// PID of the process to watch (the dog). If not provided, defaults to parent PID.
    #[arg(long)]
    dog: Option<u32>,

    /// PIDs of the processes to kill when the dog dies (comma-separated).
    #[arg(long, required = true)]
    targets: String,

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

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Initialize logging
    let mut logger = env_logger::Builder::from_default_env();
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
    if let Some(log_path) = &args.log_file {
        let log_file = File::create(log_path).expect("Failed to create log file");
        let log_file = Box::new(log_file);
        logger.target(env_logger::Target::Pipe(log_file));
    }
    logger.init();

    let dog_pid = get_dog_pid(args.dog);
    let targets = match parse_target_pids(&args.targets) {
        Ok(pids) => pids,
        Err(e) => {
            eprintln!("Error parsing targets: {}", e);
            std::process::exit(1);
        }
    };
    info!("🐶 Watching dog PID: {}", dog_pid);
    info!("🎯 Targets: {:?}", targets);
    info!(
        "⏳ Tick every {}s, vengeance in {}s",
        args.tick, args.vengeance_delay
    );
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    tokio::spawn(async move {
        loop {
            if !is_process_alive(dog_pid) {
                warn!("💀 Dog died. Unleashing vengeance.");

                // Step 1: Try group killing first for all targets
                let mut targets_needing_individual_kill: Vec<u32> = Vec::new();

                for &pid in &targets {
                    info!("⚠️ Attempting group kill for PID {}", pid);
                    let group_kill_successful = kill_process_group(pid, false);

                    if !group_kill_successful {
                        info!("⚠️ Group kill failed for PID {}, will use individual process termination", pid);
                        targets_needing_individual_kill.push(pid);
                    }
                }

                // Step 2: If group killing failed for any targets, build fresh process trees and kill individually
                let mut all_pids_to_kill: Vec<u32> = Vec::new();

                if !targets_needing_individual_kill.is_empty() {
                    info!("🔍 Building fresh process trees for individual termination");

                    for &pid in &targets_needing_individual_kill {
                        info!("⚠️ Building fresh process tree for PID {}", pid);
                        let process_tree = build_process_tree(pid);
                        let pids_in_order = get_pids_by_depth(&process_tree);

                        info!(
                            "📋 PID {} has {} child processes: {:?}",
                            pid,
                            pids_in_order.len(),
                            pids_in_order
                        );

                        // Add all PIDs from this tree to our kill list
                        for &tree_pid in &pids_in_order {
                            if !all_pids_to_kill.contains(&tree_pid) {
                                all_pids_to_kill.push(tree_pid);
                            }
                        }
                    }

                    info!("🎯 Total PIDs to kill individually: {:?}", all_pids_to_kill);

                    // Step 3: Send SIGTERM to individual PIDs
                    info!("⚠️ Sending SIGTERM to individual processes");
                    send_signal_to_pids(&all_pids_to_kill, false);
                }

                // Step 4: Wait for graceful termination
                info!(
                    "⏳ Waiting {} seconds for graceful termination...",
                    args.vengeance_delay
                );
                tokio::time::sleep(Duration::from_secs(args.vengeance_delay)).await;

                // Step 5: Refresh the kill list, adding new PIDs but preserving original ones
                let mut final_kill_list: Vec<u32> = all_pids_to_kill.clone();
                let mut targets_needing_individual_force_kill: Vec<u32> = Vec::new();

                for &pid in &targets {
                    info!("⚠️ Forcefully attempting group kill for PID {}", pid);
                    let group_kill_successful = kill_process_group(pid, true);

                    if !group_kill_successful {
                        info!("⚠️ Group kill failed for PID {}, will use individual process force termination", pid);
                        targets_needing_individual_force_kill.push(pid);
                    }
                }
                if !targets_needing_individual_force_kill.is_empty() {
                    info!("🔍 Refreshing process trees to catch any new processes");

                    for &pid in &targets_needing_individual_force_kill {
                        let process_tree = build_process_tree(pid);
                        let pids_in_order = get_pids_by_depth(&process_tree);

                        // Add any new PIDs that weren't in the original list
                        for &tree_pid in &pids_in_order {
                            if !final_kill_list.contains(&tree_pid) {
                                info!("➕ Adding new PID {} to kill list", tree_pid);
                                final_kill_list.push(tree_pid);
                            }
                        }
                    }

                    info!(
                        "🎯 Final kill list (preserving original + new PIDs): {:?}",
                        final_kill_list
                    );
                }

                // Step 6: Force kill all processes in the final list
                info!("🔪 Force killing all processes in final list");
                send_signal_to_pids(&final_kill_list, true);

                info!("🧘 Process-wick retires in peace.");
                r.store(false, Ordering::SeqCst);
                break;
            }
            tokio::time::sleep(Duration::from_secs(args.tick)).await;
        }
    });
    while running.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
