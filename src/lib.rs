use log::{info, warn};
use std::collections::{HashMap, HashSet, VecDeque};

#[cfg(unix)]
use nix::{
    sys::signal::{kill, Signal},
    unistd::Pid,
};

#[cfg(windows)]
use {
    std::mem::{size_of, zeroed},
    winapi::um::{
        handleapi::CloseHandle,
        processthreadsapi::GetCurrentProcessId,
        tlhelp32::{
            CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32,
            TH32CS_SNAPPROCESS,
        },
        winnt::HANDLE,
    },
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

/// Builds a process tree starting from the given root PID
pub fn build_process_tree(root_pid: u32) -> HashMap<u32, ProcessNode> {
    let mut process_tree: HashMap<u32, ProcessNode> = HashMap::new();
    let mut to_visit: VecDeque<u32> = VecDeque::new();

    // Start with the root process
    to_visit.push_back(root_pid);
    process_tree.insert(root_pid, ProcessNode::new(root_pid, 0));

    while let Some(current_pid) = to_visit.pop_front() {
        let current_depth = process_tree[&current_pid].depth;

        // Get all processes and find children of current_pid
        let all_processes = get_all_processes();

        for (pid, parent_pid) in all_processes {
            if parent_pid == current_pid && pid != current_pid {
                // This is a child of current_pid
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

/// Gets all processes with their parent PIDs
#[cfg(unix)]
pub fn get_all_processes() -> Vec<(u32, u32)> {
    use std::fs;
    use std::io::{BufRead, BufReader};

    let mut processes = Vec::new();

    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            if let Ok(entry_name) = entry.file_name().into_string() {
                if let Ok(pid) = entry_name.parse::<u32>() {
                    // Read /proc/{pid}/stat to get parent PID
                    let stat_path = format!("/proc/{}/stat", pid);
                    if let Ok(file) = fs::File::open(stat_path) {
                        let reader = BufReader::new(file);
                        if let Some(Ok(line)) = reader.lines().next() {
                            let parts: Vec<&str> = line.split_whitespace().collect();
                            if parts.len() >= 4 {
                                if let Ok(ppid) = parts[3].parse::<u32>() {
                                    processes.push((pid, ppid));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    processes
}

#[cfg(windows)]
pub fn get_all_processes() -> Vec<(u32, u32)> {
    let mut processes = Vec::new();

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() {
            return processes;
        }

        let mut entry: PROCESSENTRY32 = zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32>() as u32;

        if Process32First(snapshot, &mut entry as *mut PROCESSENTRY32) != 0 {
            loop {
                processes.push((entry.th32ProcessID, entry.th32ParentProcessID));

                if Process32Next(snapshot, &mut entry as *mut PROCESSENTRY32) == 0 {
                    break;
                }
            }
        }

        CloseHandle(snapshot);
    }

    processes
}

pub fn get_dog_pid(dog_arg: Option<u32>) -> u32 {
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
            if Process32First(snapshot, &mut entry as *mut PROCESSENTRY32) != 0 {
                loop {
                    if entry.th32ProcessID == current_pid {
                        found = true;
                        break;
                    }
                    if Process32Next(snapshot, &mut entry as *mut PROCESSENTRY32) == 0 {
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

pub fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        kill(Pid::from_raw(pid as i32), None).is_ok()
    }

    #[cfg(windows)]
    {
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot.is_null() {
                warn!(
                    "Failed to create process snapshot: {:?}",
                    std::io::Error::last_os_error()
                );
                return false;
            }

            let mut entry: PROCESSENTRY32 = zeroed();
            entry.dwSize = size_of::<PROCESSENTRY32>() as u32;

            let mut found = false;
            if Process32First(snapshot, &mut entry as *mut PROCESSENTRY32) != 0 {
                loop {
                    if entry.th32ProcessID == pid {
                        found = true;
                        break;
                    }
                    if Process32Next(snapshot, &mut entry as *mut PROCESSENTRY32) == 0 {
                        break;
                    }
                }
            }

            CloseHandle(snapshot);
            found
        }
    }
}

#[cfg(unix)]
pub fn send_signal(pid: u32, force: bool) {
    let sig = if force {
        Signal::SIGKILL
    } else {
        Signal::SIGTERM
    };

    // First, try to kill the process group
    let pgid = -(pid as i32);
    info!(
        "Attempting to send {} to process group {} (original PID: {})",
        sig, pgid, pid
    );

    match kill(Pid::from_raw(pgid), sig) {
        Ok(_) => {
            info!("Successfully sent {} to process group {}.", sig, pgid);
            return; // If process group kill succeeds, we're done
        }
        Err(e_pgid) => {
            warn!(
                "Failed to send {} to process group {}: {:?}. Falling back to individual process tree termination.",
                sig, pgid, e_pgid
            );
        }
    }

    // If process group kill fails, use DFS approach for individual process tree
    info!(
        "Building process tree for PID {} and terminating with DFS approach",
        pid
    );

    let process_tree = build_process_tree(pid);
    let processes_in_order = get_processes_in_dfs_order(&process_tree, pid);

    info!(
        "Process tree for PID {} contains {} processes. Terminating in DFS order: {:?}",
        pid,
        processes_in_order.len(),
        processes_in_order
    );

    for &process_pid in &processes_in_order {
        if is_process_alive(process_pid) {
            info!(
                "Sending {} to PID {} (depth: {})",
                sig,
                process_pid,
                process_tree.get(&process_pid).map(|n| n.depth).unwrap_or(0)
            );

            match kill(Pid::from_raw(process_pid as i32), sig) {
                Ok(_) => {
                    info!("Successfully sent {} to PID {}.", sig, process_pid);
                }
                Err(e) => {
                    warn!("Failed to send {} to PID {}: {:?}.", sig, process_pid, e);
                }
            }
        }
    }
}

#[cfg(windows)]
pub fn send_signal(pid: u32, force: bool) {
    // First, try to use taskkill with /T flag for process tree
    let pid_str = pid.to_string();
    let mut cmd = std::process::Command::new("taskkill");
    cmd.arg("/PID").arg(&pid_str);
    cmd.arg("/T"); // /T kills child processes

    if force {
        cmd.arg("/F"); // Force kill
    }

    info!(
        "Attempting taskkill for PID {} and its children (force: {}). Command: {:?}",
        pid, force, cmd
    );

    match cmd.output() {
        Ok(output) => {
            if output.status.success() {
                info!(
                    "taskkill for PID {} and its children (force: {}) successful.",
                    pid, force
                );
                return; // If taskkill succeeds, we're done
            } else {
                warn!(
                    "taskkill for PID {} and its children (force: {}) failed. Status: {}. Stdout: [{}], Stderr: [{}]. Falling back to manual DFS approach.",
                    pid,
                    force,
                    output.status,
                    String::from_utf8_lossy(&output.stdout).trim(),
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }
        }
        Err(e) => {
            warn!(
                "Windows: Failed to execute taskkill for PID {} and its children (force: {}): {:?}. Falling back to manual DFS approach.",
                pid, force, e
            );
        }
    }

    // If taskkill fails, use DFS approach for manual process tree termination
    info!(
        "Building process tree for PID {} and terminating with DFS approach",
        pid
    );

    let process_tree = build_process_tree(pid);
    let processes_in_order = get_processes_in_dfs_order(&process_tree, pid);

    info!(
        "Process tree for PID {} contains {} processes. Terminating in DFS order: {:?}",
        pid,
        processes_in_order.len(),
        processes_in_order
    );

    for &process_pid in &processes_in_order {
        if is_process_alive(process_pid) {
            info!(
                "Sending termination signal to PID {} (depth: {})",
                process_pid,
                process_tree.get(&process_pid).map(|n| n.depth).unwrap_or(0)
            );

            let mut cmd = std::process::Command::new("taskkill");
            cmd.arg("/PID").arg(&process_pid.to_string());

            if force {
                cmd.arg("/F"); // Force kill
            }

            match cmd.output() {
                Ok(output) => {
                    if output.status.success() {
                        info!("Successfully terminated PID {}.", process_pid);
                    } else {
                        warn!(
                            "Failed to terminate PID {}. Status: {}. Stdout: [{}], Stderr: [{}]",
                            process_pid,
                            output.status,
                            String::from_utf8_lossy(&output.stdout).trim(),
                            String::from_utf8_lossy(&output.stderr).trim()
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to execute taskkill for PID {}: {:?}",
                        process_pid, e
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod process_node {
        use super::*;

        #[test]
        fn test_process_node_creation() {
            let node = ProcessNode::new(123, 456);
            assert_eq!(node.pid, 123);
            assert_eq!(node.parent_pid, 456);
            assert_eq!(node.children, Vec::<u32>::new());
            assert_eq!(node.depth, 0);
        }

        #[test]
        fn test_process_node_equality() {
            let node1 = ProcessNode::new(123, 456);
            let node2 = ProcessNode::new(123, 456);
            let node3 = ProcessNode::new(789, 456);
            assert_eq!(node1, node2);
            assert_ne!(node1, node3);
        }

        #[test]
        fn test_process_node_clone() {
            let original = ProcessNode::new(123, 456);
            let cloned = original.clone();
            assert_eq!(original, cloned);
            assert_eq!(original.pid, cloned.pid);
            assert_eq!(original.parent_pid, cloned.parent_pid);
        }
    }

    mod process_tree {
        use super::*;

        pub(crate) fn create_test_tree() -> HashMap<u32, ProcessNode> {
            let mut tree = HashMap::new();
            // Create a simple tree: 1 -> 2,3 -> 4,5,6
            tree.insert(1, ProcessNode::new(1, 0));
            tree.insert(2, ProcessNode::new(2, 1));
            tree.insert(3, ProcessNode::new(3, 1));
            tree.insert(4, ProcessNode::new(4, 2));
            tree.insert(5, ProcessNode::new(5, 2));
            tree.insert(6, ProcessNode::new(6, 3));
            // Set up parent-child relationships
            tree.get_mut(&1).unwrap().children = vec![2, 3];
            tree.get_mut(&2).unwrap().children = vec![4, 5];
            tree.get_mut(&3).unwrap().children = vec![6];
            // Set depths
            tree.get_mut(&1).unwrap().depth = 0;
            tree.get_mut(&2).unwrap().depth = 1;
            tree.get_mut(&3).unwrap().depth = 1;
            tree.get_mut(&4).unwrap().depth = 2;
            tree.get_mut(&5).unwrap().depth = 2;
            tree.get_mut(&6).unwrap().depth = 2;
            tree
        }

        #[test]
        fn test_dfs_ordering_simple_tree() {
            let tree = create_test_tree();
            let processes_in_order = get_processes_in_dfs_order(&tree, 1);
            assert_eq!(processes_in_order, vec![4, 5, 2, 6, 3, 1]);
        }

        #[test]
        fn test_dfs_ordering_single_node() {
            let mut tree = HashMap::new();
            tree.insert(1, ProcessNode::new(1, 0));
            let processes_in_order = get_processes_in_dfs_order(&tree, 1);
            assert_eq!(processes_in_order, vec![1]);
        }

        #[test]
        fn test_dfs_ordering_linear_chain() {
            let mut tree = HashMap::new();
            tree.insert(1, ProcessNode::new(1, 0));
            tree.insert(2, ProcessNode::new(2, 1));
            tree.insert(3, ProcessNode::new(3, 2));
            tree.get_mut(&1).unwrap().children = vec![2];
            tree.get_mut(&2).unwrap().children = vec![3];
            tree.get_mut(&1).unwrap().depth = 0;
            tree.get_mut(&2).unwrap().depth = 1;
            tree.get_mut(&3).unwrap().depth = 2;
            let processes_in_order = get_processes_in_dfs_order(&tree, 1);
            assert_eq!(processes_in_order, vec![3, 2, 1]);
        }

        #[test]
        fn test_dfs_ordering_with_cycles() {
            let mut tree = HashMap::new();
            tree.insert(1, ProcessNode::new(1, 0));
            tree.insert(2, ProcessNode::new(2, 1));
            // Create a cycle: 1 -> 2 -> 1
            tree.get_mut(&1).unwrap().children = vec![2];
            tree.get_mut(&2).unwrap().children = vec![1];
            tree.get_mut(&1).unwrap().depth = 0;
            tree.get_mut(&2).unwrap().depth = 1;
            let processes_in_order = get_processes_in_dfs_order(&tree, 1);
            assert_eq!(processes_in_order.len(), 2);
            assert!(processes_in_order.contains(&1));
            assert!(processes_in_order.contains(&2));
        }

        #[test]
        fn test_dfs_ordering_nonexistent_root() {
            let tree = HashMap::new();
            let processes_in_order = get_processes_in_dfs_order(&tree, 999);
            assert_eq!(processes_in_order, vec![]);
        }

        #[test]
        fn test_dfs_ordering_subtree() {
            let tree = create_test_tree();
            let processes_in_order = get_processes_in_dfs_order(&tree, 2);
            assert_eq!(processes_in_order, vec![4, 5, 2]);
        }
    }

    mod process_management {
        use super::*;

        #[test]
        fn test_get_dog_pid_with_provided_value() {
            let provided_pid = 12345;
            let result = get_dog_pid(Some(provided_pid));
            assert_eq!(result, provided_pid);
        }

        #[test]
        fn test_get_dog_pid_with_none() {
            let result = get_dog_pid(None);
            assert!(result > 0);
        }

        #[test]
        fn test_is_process_alive_with_invalid_pid() {
            let result = is_process_alive(999999);
            assert!(!result);
        }
    }

    mod edge_cases {
        use super::*;

        #[test]
        fn test_empty_process_tree() {
            let tree = HashMap::new();
            let processes_in_order = get_processes_in_dfs_order(&tree, 1);
            assert_eq!(processes_in_order, vec![]);
        }

        #[test]
        fn test_process_tree_with_orphaned_nodes() {
            let mut tree = HashMap::new();
            tree.insert(1, ProcessNode::new(1, 0));
            tree.insert(2, ProcessNode::new(2, 999));
            let processes_in_order = get_processes_in_dfs_order(&tree, 1);
            assert_eq!(processes_in_order, vec![1]);
        }

        #[test]
        fn test_process_tree_with_duplicate_children() {
            let mut tree = HashMap::new();
            tree.insert(1, ProcessNode::new(1, 0));
            tree.insert(2, ProcessNode::new(2, 1));
            tree.get_mut(&1).unwrap().children = vec![2, 2, 2];
            let processes_in_order = get_processes_in_dfs_order(&tree, 1);
            assert_eq!(processes_in_order, vec![2, 1]);
        }
    }

    mod property_tests {
        use super::*;

        #[test]
        fn test_dfs_ordering_properties() {
            let tree = process_tree::create_test_tree();
            let processes_in_order = get_processes_in_dfs_order(&tree, 1);
            assert_eq!(processes_in_order.len(), tree.len());
            let unique_pids: HashSet<u32> = processes_in_order.iter().cloned().collect();
            assert_eq!(unique_pids.len(), processes_in_order.len());
            assert_eq!(*processes_in_order.last().unwrap(), 1);
        }

        #[test]
        fn test_process_node_immutability() {
            let node = ProcessNode::new(123, 456);
            let original_children = node.children.clone();
            assert_eq!(node.children, original_children);
        }
    }
}
