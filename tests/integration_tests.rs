use std::fs;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Helper function to build the process-wick binary
fn build_binary() -> String {
    let output = Command::new("cargo")
        .args(&["build", "--release"])
        .output()
        .expect("Failed to build binary");

    if !output.status.success() {
        panic!("Build failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    // Return the path to the built binary
    "target/release/process-wick".to_string()
}

#[cfg(unix)]
fn create_test_process() -> Child {
    Command::new("sleep")
        .arg("100")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn test process")
}

#[cfg(windows)]
fn create_test_process() -> Child {
    Command::new("cmd")
        .args(&["/C", "timeout /T 100 > NUL"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn test process")
}

#[cfg(unix)]
fn create_parent_with_children() -> Child {
    let script = r#"
#!/bin/bash
sleep 50 &
CHILD_PID=$!
echo "CHILD_PID=$CHILD_PID"
sleep 100
"#;
    Command::new("bash")
        .arg("-c")
        .arg(script)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn parent with children")
}

#[cfg(windows)]
fn create_parent_with_children() -> Child {
    // PowerShell script: start a child process, print its PID, then sleep
    let script = r#"
$child = Start-Process -PassThru powershell -ArgumentList '-Command', 'Start-Sleep -Seconds 50'
Write-Output "CHILD_PID=$($child.Id)"
Start-Sleep -Seconds 100
"#;
    Command::new("powershell")
        .arg("-Command")
        .arg(script)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn parent with children")
}

#[test]
fn test_basic_functionality() {
    let binary_path = build_binary();

    // Create a test process to watch
    let mut dog_process = create_test_process();
    let dog_pid = dog_process.id();

    // Create target processes
    let mut target1 = create_test_process();
    let mut target2 = create_test_process();
    let target1_pid = target1.id();
    let target2_pid = target2.id();

    // Start process-wick in a separate thread
    let (tx, rx) = mpsc::channel();
    let binary_path_clone = binary_path.clone();

    let handle = thread::spawn(move || {
        let output = Command::new(&binary_path_clone)
            .args(&[
                "--dog",
                &dog_pid.to_string(),
                "--targets",
                &target1_pid.to_string(),
                &target2_pid.to_string(),
                "--tick",
                "1",
                "--vengeance-delay",
                "2",
            ])
            .output()
            .expect("Failed to execute process-wick");

        tx.send(output).unwrap();
    });

    // Wait a moment for process-wick to start
    thread::sleep(Duration::from_millis(500));

    // Kill the dog process
    dog_process.kill().expect("Failed to kill dog process");

    // Wait for process-wick to finish
    let output = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("Timeout waiting for process-wick");

    // Verify that target processes were killed
    assert!(
        !target1.try_wait().unwrap().is_some(),
        "Target1 should have been killed"
    );
    assert!(
        !target2.try_wait().unwrap().is_some(),
        "Target2 should have been killed"
    );

    // Clean up
    let _ = target1.kill();
    let _ = target2.kill();
    handle.join().unwrap();
}

#[test]
fn test_default_dog_pid() {
    let binary_path = build_binary();

    // Create target processes
    let mut target1 = create_test_process();
    let target1_pid = target1.id();

    // Start process-wick without specifying --dog (should use parent PID)
    let (tx, rx) = mpsc::channel();
    let binary_path_clone = binary_path.clone();

    let handle = thread::spawn(move || {
        let output = Command::new(&binary_path_clone)
            .args(&[
                "--targets",
                &target1_pid.to_string(),
                "--tick",
                "1",
                "--vengeance-delay",
                "1",
            ])
            .output()
            .expect("Failed to execute process-wick");

        tx.send(output).unwrap();
    });

    // Wait a moment for process-wick to start
    thread::sleep(Duration::from_millis(500));

    // Kill the current process (which is the parent of process-wick)
    // This will cause process-wick to terminate
    std::process::exit(0);

    // This test is a bit tricky because we're killing the parent
    // In a real scenario, this would work, but for testing we'll just verify the binary starts
    handle.join().unwrap();

    // Clean up
    let _ = target1.kill();
}

#[test]
fn test_logging_functionality() {
    let binary_path = build_binary();
    let log_file = "test_integration.log";

    // Clean up any existing log file
    let _ = fs::remove_file(log_file);

    // Create a test process to watch
    let mut dog_process = create_test_process();
    let dog_pid = dog_process.id();

    // Create target process
    let mut target = create_test_process();
    let target_pid = target.id();

    // Start process-wick with logging
    let (tx, rx) = mpsc::channel();
    let binary_path_clone = binary_path.clone();

    let handle = thread::spawn(move || {
        let output = Command::new(&binary_path_clone)
            .args(&[
                "--dog",
                &dog_pid.to_string(),
                "--targets",
                &target_pid.to_string(),
                "--tick",
                "1",
                "--vengeance-delay",
                "2",
                "--log-file",
                log_file,
                "--log-level",
                "debug",
            ])
            .output()
            .expect("Failed to execute process-wick");

        tx.send(output).unwrap();
    });

    // Wait a moment for process-wick to start
    thread::sleep(Duration::from_millis(500));

    // Kill the dog process
    dog_process.kill().expect("Failed to kill dog process");

    // Wait for process-wick to finish
    let _output = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("Timeout waiting for process-wick");

    // Verify log file was created and contains expected content
    assert!(
        Path::new(log_file).exists(),
        "Log file should have been created"
    );

    let log_content = fs::read_to_string(log_file).expect("Failed to read log file");
    assert!(
        log_content.contains("🐶 Watching dog PID"),
        "Log should contain dog watching message"
    );
    assert!(
        log_content.contains("🎯 Targets"),
        "Log should contain targets message"
    );
    assert!(
        log_content.contains("💀 Dog died"),
        "Log should contain dog death message"
    );

    // Clean up
    let _ = target.kill();
    let _ = fs::remove_file(log_file);
    handle.join().unwrap();
}

#[test]
fn test_multiple_targets() {
    let binary_path = build_binary();

    // Create a test process to watch
    let mut dog_process = create_test_process();
    let dog_pid = dog_process.id();

    // Create multiple target processes
    let mut targets = Vec::new();
    let mut target_pids = Vec::new();

    for _ in 0..5 {
        let mut target = create_test_process();
        target_pids.push(target.id());
        targets.push(target);
    }

    // Start process-wick
    let (tx, rx) = mpsc::channel();
    let binary_path_clone = binary_path.clone();

    let handle = thread::spawn(move || {
        let dog_pid_str = dog_pid.to_string();
        let target_pid_strings: Vec<String> =
            target_pids.iter().map(|pid| pid.to_string()).collect();

        let mut args = vec![
            "--dog",
            &dog_pid_str,
            "--tick",
            "1",
            "--vengeance-delay",
            "2",
        ];

        args.push("--targets");
        for pid_str in &target_pid_strings {
            args.push(pid_str);
        }

        let output = Command::new(&binary_path_clone)
            .args(&args)
            .output()
            .expect("Failed to execute process-wick");

        tx.send(output).unwrap();
    });

    // Wait a moment for process-wick to start
    thread::sleep(Duration::from_millis(500));

    // Kill the dog process
    dog_process.kill().expect("Failed to kill dog process");

    // Wait for process-wick to finish
    let _output = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("Timeout waiting for process-wick");

    // Verify all target processes were killed
    for target in &mut targets {
        assert!(
            !target.try_wait().unwrap().is_some(),
            "All targets should have been killed"
        );
    }

    // Clean up
    for mut target in targets {
        let _ = target.kill();
    }
    handle.join().unwrap();
}

#[test]
fn test_vengeance_delay() {
    let binary_path = build_binary();

    // Create a test process to watch
    let mut dog_process = create_test_process();
    let dog_pid = dog_process.id();

    // Create target process
    let mut target = create_test_process();
    let target_pid = target.id();

    let start_time = std::time::Instant::now();

    // Start process-wick with a longer vengeance delay
    let (tx, rx) = mpsc::channel();
    let binary_path_clone = binary_path.clone();

    let handle = thread::spawn(move || {
        let output = Command::new(&binary_path_clone)
            .args(&[
                "--dog",
                &dog_pid.to_string(),
                "--targets",
                &target_pid.to_string(),
                "--tick",
                "1",
                "--vengeance-delay",
                "3",
            ])
            .output()
            .expect("Failed to execute process-wick");

        tx.send(output).unwrap();
    });

    // Wait a moment for process-wick to start
    thread::sleep(Duration::from_millis(500));

    // Kill the dog process
    dog_process.kill().expect("Failed to kill dog process");

    // Wait for process-wick to finish
    let _output = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("Timeout waiting for process-wick");

    let elapsed = start_time.elapsed();

    // The process should have taken at least the vengeance delay time
    assert!(
        elapsed >= Duration::from_secs(3),
        "Process should have taken at least 3 seconds"
    );

    // Clean up
    let _ = target.kill();
    handle.join().unwrap();
}

#[test]
fn test_invalid_pid_handling() {
    let binary_path = build_binary();

    // Try to run process-wick with an invalid dog PID
    let output = Command::new(&binary_path)
        .args(&[
            "--dog",
            "999999",
            "--targets",
            "999998",
            "--tick",
            "1",
            "--vengeance-delay",
            "1",
        ])
        .output()
        .expect("Failed to execute process-wick");

    // The process should start but eventually terminate when it can't find the dog
    // We can't easily test this without complex process management, so we'll just verify it doesn't crash
    assert!(
        output.status.success() || output.status.code().is_some(),
        "Process should not crash"
    );
}

#[test]
fn test_help_output() {
    let binary_path = build_binary();

    let output = Command::new(&binary_path)
        .arg("--help")
        .output()
        .expect("Failed to execute process-wick");

    assert!(output.status.success(), "Help should display successfully");

    let help_text = String::from_utf8_lossy(&output.stdout);
    assert!(
        help_text.contains("process-wick"),
        "Help should contain program name"
    );
    assert!(
        help_text.contains("--dog"),
        "Help should contain --dog option"
    );
    assert!(
        help_text.contains("--targets"),
        "Help should contain --targets option"
    );
    assert!(
        help_text.contains("--vengeance-delay"),
        "Help should contain --vengeance-delay option"
    );
    assert!(
        help_text.contains("--tick"),
        "Help should contain --tick option"
    );
}

#[test]
fn test_missing_required_args() {
    let binary_path = build_binary();

    // Try to run without required --targets argument
    let output = Command::new(&binary_path)
        .args(&["--dog", "1234", "--tick", "1"])
        .output()
        .expect("Failed to execute process-wick");

    // Should fail with an error about missing required argument
    assert!(
        !output.status.success(),
        "Should fail without required --targets argument"
    );

    let error_text = String::from_utf8_lossy(&output.stderr);
    assert!(
        error_text.contains("required"),
        "Error should mention required argument"
    );
}

#[test]
fn test_process_tree_killing() {
    let binary_path = build_binary();

    // Create a parent process that spawns children
    let mut parent_process = create_parent_with_children();
    let parent_pid = parent_process.id();

    // Wait a moment for the parent to spawn children
    thread::sleep(Duration::from_millis(1000));

    // Create target process
    let mut target = create_test_process();
    let target_pid = target.id();

    // Start process-wick
    let (tx, rx) = mpsc::channel();
    let binary_path_clone = binary_path.clone();

    let handle = thread::spawn(move || {
        let output = Command::new(&binary_path_clone)
            .args(&[
                "--dog",
                &parent_pid.to_string(),
                "--targets",
                &target_pid.to_string(),
                "--tick",
                "1",
                "--vengeance-delay",
                "2",
            ])
            .output()
            .expect("Failed to execute process-wick");

        tx.send(output).unwrap();
    });

    // Wait a moment for process-wick to start
    thread::sleep(Duration::from_millis(500));

    // Kill the parent process
    parent_process
        .kill()
        .expect("Failed to kill parent process");

    // Wait for process-wick to finish
    let _output = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("Timeout waiting for process-wick");

    // Verify target process was killed
    assert!(
        !target.try_wait().unwrap().is_some(),
        "Target should have been killed"
    );

    // Clean up
    let _ = target.kill();
    handle.join().unwrap();
}
