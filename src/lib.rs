use log::{info, warn};
use std::collections::{HashMap, HashSet, VecDeque};
use sysinfo::{Pid, System};

#[cfg(unix)]
use nix::{
    sys::signal::{kill, Signal},
    unistd::Pid as NixPid,
};

// Process tree node structure
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessNode {
    pub pid: u32,
    pub parent_pid: u32,
    pub children: Vec<u32>,
    pub depth: usize,
}

impl ProcessNode {
    pub fn new(pid: u32, parent_pid: u32) -> Self {
        Self {
            pid,
            parent_pid,
            children: Vec::new(),
            depth: 0,
        }
    }
}

/// Gets all processes with their parent PIDs using sysinfo (cross-platform)
pub fn get_all_processes() -> Vec<(u32, u32)> {
    let mut sys = System::new_all();
    sys.refresh_all();

    let mut processes = Vec::new();
    for (pid, process) in sys.processes() {
        let parent_pid = process.parent().unwrap_or(Pid::from(0));
        processes.push((pid.as_u32(), parent_pid.as_u32()));
    }

    processes
}

/// Builds a process tree s ting from the given root PID
pub fn build_process_tree(root_pid: u32) -> HashMap<u32, ProcessNode> {
    let mut process_tree: HashMap<u32, ProcessNode> = HashMap::new();
    let mut to_visit: VecDeque<u32> = VecDeque::new();
    let mut visited_pids: HashSet<u32> = HashSet::new();

    // Start with the root process
    to_visit.push_back(root_pid);
    process_tree.insert(root_pid, ProcessNode::new(root_pid, 0));

    while let Some(current_pid) = to_visit.pop_front() {
        if visited_pids.contains(&current_pid) {
            continue; // Skip cycles
        }
        visited_pids.insert(current_pid);

        let current_depth = process_tree[&current_pid].depth;
        let all_processes = get_all_processes();

        for (pid, parent_pid) in all_processes {
            if parent_pid == current_pid && pid != current_pid {
                if !process_tree.contains_key(&pid) {
                    let mut child_node = ProcessNode::new(pid, parent_pid);
                    child_node.depth = current_depth + 1;
                    process_tree.insert(pid, child_node);
                    to_visit.push_back(pid);
                }

                // Add this child to the current node's children list
                if let Some(current_node) = process_tree.get_mut(&current_pid) {
                    current_node.children.push(pid);
                }
            }
        }
    }

    process_tree
}

/// Performs DFS traversal to get processes in order (deepest first)
pub fn get_processes_in_dfs_order(
    process_tree: &HashMap<u32, ProcessNode>,
    root_pid: u32,
) -> Vec<u32> {
    let mut result: Vec<u32> = Vec::new();
    let mut visited: HashSet<u32> = HashSet::new();

    fn dfs(
        pid: u32,
        process_tree: &HashMap<u32, ProcessNode>,
        visited: &mut HashSet<u32>,
        result: &mut Vec<u32>,
    ) {
        if visited.contains(&pid) {
            return;
        }

        visited.insert(pid);

        if let Some(node) = process_tree.get(&pid) {
            // First, recursively visit all children
            for &child_pid in &node.children {
                dfs(child_pid, process_tree, visited, result);
            }

            // Then add this process (children are already added, so this comes after)
            result.push(pid);
        }
    }

    dfs(root_pid, process_tree, &mut visited, &mut result);
    result
}

pub fn get_dog_pid(dog_arg: Option<u32>) -> u32 {
    dog_arg.unwrap_or_else(|| {
        #[cfg(unix)]
        {
            unsafe { libc::getppid() as u32 }
        }
        #[cfg(windows)]
        {
            // Use sysinfo to get current process info
            let mut sys = System::new_all();
            sys.refresh_all();
            let current_pid = std::process::id();

            // Find current process and get its parent
            if let Some(process) = sys.process(Pid::from(current_pid as usize)) {
                process.parent().unwrap_or(Pid::from(0)).as_u32()
            } else {
                0
            }
        }
    })
}

pub fn is_process_alive(pid: u32) -> bool {
    let mut sys = System::new_all();
    sys.refresh_all();
    sys.process(Pid::from(pid as usize)).is_some()
}

/// Gets all PIDs in the process tree sorted by depth (shallowest first)
pub fn get_pids_by_depth(process_tree: &HashMap<u32, ProcessNode>) -> Vec<u32> {
    let mut all_pids: Vec<u32> = process_tree.keys().cloned().collect();
    all_pids.sort_by_key(|&pid| process_tree.get(&pid).map(|n| n.depth).unwrap_or(0));
    all_pids
}

/// Sends signal to a list of PIDs in the specified order
pub fn send_signal_to_pids(pids: &[u32], force: bool) {
    for &pid in pids {
        if is_process_alive(pid) {
            #[cfg(unix)]
            {
                let sig = if force {
                    Signal::SIGKILL
                } else {
                    Signal::SIGTERM
                };

                info!("Sending {} to PID {}", sig, pid);

                match kill(NixPid::from_raw(pid as i32), sig) {
                    Ok(_) => {
                        info!("Successfully sent {} to PID {}.", sig, pid);
                    }
                    Err(e) => {
                        warn!("Failed to send {} to PID {}: {:?}.", sig, pid, e);
                    }
                }
            }

            #[cfg(windows)]
            {
                let sig_name = if force { "SIGKILL" } else { "SIGTERM" };
                info!("Sending {} to PID {}", sig_name, pid);

                let mut cmd = std::process::Command::new("taskkill");
                cmd.arg("/PID").arg(&pid.to_string());

                if force {
                    cmd.arg("/F"); // Force kill
                }

                match cmd.output() {
                    Ok(output) => {
                        if output.status.success() {
                            info!("Successfully terminated PID {}.", pid);
                        } else {
                            warn!(
                                "Failed to terminate PID {}. Status: {}.",
                                pid, output.status
                            );
                        }
                    }
                    Err(e) => {
                        warn!("Failed to execute taskkill for PID {}: {:?}", pid, e);
                    }
                }
            }
        }
    }
}

/// Attempts to kill a process group, returns true if successful, false if it needs fallback
pub fn kill_process_group(root_pid: u32, force: bool) -> bool {
    #[cfg(unix)]
    {
        let sig = if force {
            Signal::SIGKILL
        } else {
            Signal::SIGTERM
        };

        // Try to kill the process group
        let pgid = -(root_pid as i32);
        info!(
            "Attempting to send {} to process group {} (original PID: {})",
            sig, pgid, root_pid
        );

        match kill(NixPid::from_raw(pgid), sig) {
            Ok(_) => {
                info!("Successfully sent {} to process group {}.", sig, pgid);
                return true;
            }
            Err(e_pgid) => {
                warn!("Failed to send {} to process group {}: {:?}. Will use individual process termination.", sig, pgid, e_pgid);
            }
        }
    }

    #[cfg(windows)]
    {
        // Try to use taskkill with /T flag for process tree
        let pid_str = root_pid.to_string();
        let mut cmd = std::process::Command::new("taskkill");
        cmd.arg("/PID").arg(&pid_str);
        cmd.arg("/T"); // /T kills child processes

        if force {
            cmd.arg("/F"); // Force kill
        }

        info!(
            "Attempting taskkill for PID {} and its children (force: {}).",
            root_pid, force
        );

        match cmd.output() {
            Ok(output) => {
                if output.status.success() {
                    info!(
                        "taskkill for PID {} and its children (force: {}) successful.",
                        root_pid, force
                    );
                    return true;
                } else {
                    warn!("taskkill for PID {} and its children (force: {}) failed. Status: {}. Will use individual process termination.", 
                          root_pid, force, output.status);
                }
            }
            Err(e) => {
                warn!("Failed to execute taskkill for PID {} and its children (force: {}): {:?}. Will use individual process termination.", 
                      root_pid, force, e);
            }
        }
    }

    false
}

/// Parses a comma-separated string of PIDs into a HashSet of u32 values
///
/// # Arguments
/// * `targets_str` - A comma-separated string of PIDs (e.g., "1234,5678,9012")
///
/// # Returns
/// * `Result<HashSet<u32>, String>` - A HashSet of PIDs or an error message
///
/// # Examples
/// ```
/// use process_wick::parse_target_pids;
///
/// let result = parse_target_pids("1234,5678,9012").unwrap();
/// assert_eq!(result.len(), 3);
/// assert!(result.contains(&1234));
/// assert!(result.contains(&5678));
/// assert!(result.contains(&9012));
/// ```
pub fn parse_target_pids(targets_str: &str) -> Result<HashSet<u32>, String> {
    let mut pids = HashSet::new();

    for s in targets_str.split(',') {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            match trimmed.parse::<u32>() {
                Ok(pid) => {
                    pids.insert(pid);
                }
                Err(_) => {
                    return Err(format!("Invalid PID: {}", trimmed));
                }
            }
        }
    }

    if pids.is_empty() {
        return Err("No valid PIDs provided".to_string());
    }

    Ok(pids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_target_pids_basic() {
        let result = parse_target_pids("1234,5678,9012").unwrap();
        assert_eq!(result.len(), 3);
        assert!(result.contains(&1234));
        assert!(result.contains(&5678));
        assert!(result.contains(&9012));
    }

    #[test]
    fn test_parse_target_pids_with_spaces() {
        let result = parse_target_pids(" 1234 , 5678 , 9012 ").unwrap();
        assert_eq!(result.len(), 3);
        assert!(result.contains(&1234));
        assert!(result.contains(&5678));
        assert!(result.contains(&9012));
    }

    #[test]
    fn test_parse_target_pids_empty_and_duplicates() {
        let result = parse_target_pids(",1234,,5678,1234,").unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&1234));
        assert!(result.contains(&5678));
    }

    #[test]
    fn test_parse_target_pids_invalid_pid() {
        let result = parse_target_pids("1234,abc,5678");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid PID: abc");
    }

    #[test]
    fn test_parse_target_pids_all_invalid() {
        let result = parse_target_pids(",,,");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "No valid PIDs provided");
    }
}
