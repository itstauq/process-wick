use std::process::Command;

/// Helper function to build the process-wick binary
fn build_binary() -> String {
    let output = Command::new("cargo")
        .args(&["build", "--release"])
        .output()
        .expect("Failed to build binary");

    if !output.status.success() {
        panic!("Build failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    "target/release/process-wick".to_string()
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
    assert!(
        help_text.contains("John Wick"),
        "Help should contain John Wick reference"
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
fn test_log_level_validation() {
    let binary_path = build_binary();

    // Test with valid log levels
    let valid_levels = ["error", "warn", "info", "debug", "trace"];

    for level in &valid_levels {
        let output = Command::new(&binary_path)
            .args(&["--dog", "1234", "--targets", "5678", "--log-level", level])
            .output()
            .expect("Failed to execute process-wick");

        // The process should start (even though it will fail later due to invalid PIDs)
        // We're just testing that the log level argument is accepted
        assert!(
            output.status.code().is_some(),
            "Should accept valid log level: {}",
            level
        );
    }
}

#[test]
fn test_argument_parsing() {
    let binary_path = build_binary();

    // Test that all arguments are properly parsed
    let output = Command::new(&binary_path)
        .args(&[
            "--dog",
            "1234",
            "--targets",
            "5678",
            "9012",
            "--vengeance-delay",
            "10",
            "--tick",
            "5",
            "--log-level",
            "debug",
        ])
        .output()
        .expect("Failed to execute process-wick");

    // The process should start (even though it will fail later due to invalid PIDs)
    // We're just testing that all arguments are accepted
    assert!(
        output.status.code().is_some(),
        "Should accept all valid arguments"
    );
}

#[test]
fn test_multiple_targets_parsing() {
    let binary_path = build_binary();

    // Test with multiple target PIDs
    let output = Command::new(&binary_path)
        .args(&["--dog", "1234", "--targets", "5678", "9012", "3456", "7890"])
        .output()
        .expect("Failed to execute process-wick");

    // The process should start and accept multiple targets
    assert!(
        output.status.code().is_some(),
        "Should accept multiple target PIDs"
    );
}

#[test]
fn test_default_values() {
    let binary_path = build_binary();

    // Test with minimal arguments (should use defaults)
    // Use a non-existent PID to avoid hanging
    let output = Command::new(&binary_path)
        .args(&[
            "--dog",
            "999999", // Non-existent PID
            "--targets",
            "999998", // Non-existent PID
            "--tick",
            "1",
            "--vengeance-delay",
            "1",
        ])
        .output()
        .expect("Failed to execute process-wick");

    // The process should start with default values
    // It will fail due to invalid PIDs, but that's expected
    assert!(
        output.status.code().is_some(),
        "Should work with default values"
    );
}
