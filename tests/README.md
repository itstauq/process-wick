# Integration Tests for process-wick

This directory contains integration tests for the process-wick binary. These tests verify that the actual binary works correctly in real scenarios.

## Test Files

### `basic_integration_tests.rs`

Contains basic integration tests that focus on:

- Command-line argument parsing
- Help and version output
- Error handling for missing/invalid arguments
- Log level validation
- Default value handling

These tests are safe to run and don't involve complex process management.

### `integration_tests.rs`

Contains comprehensive integration tests that test:

- Basic functionality with real process killing
- Logging functionality
- Multiple target handling
- Vengeance delay timing
- Process tree killing
- Default dog PID behavior

**Warning**: These tests involve creating and killing real processes. They should be run in a controlled environment.

## Running the Tests

### Run Basic Tests Only (Recommended)

```bash
cargo test --test basic_integration_tests
```

### Run All Integration Tests

```bash
cargo test --test integration_tests
```

### Run All Tests (Unit + Integration)

```bash
cargo test
```

## Test Requirements

- Unix-like system (Linux/macOS) - tests use Unix-specific commands like `sleep`
- Bash shell available
- Sufficient permissions to create and kill processes
- The process-wick binary must be buildable

## Test Environment

The tests create temporary processes using the `sleep` command and verify that process-wick correctly:

1. Monitors the specified "dog" process
2. Kills target processes when the dog dies
3. Respects the vengeance delay
4. Logs appropriate messages

## Troubleshooting

### Tests Fail with Permission Errors

- Ensure you have permission to create and kill processes
- Some systems may require elevated privileges for process management

### Tests Hang or Timeout

- Check if the `sleep` command is available on your system
- Verify that bash is available for parent-child process tests
- Some tests may take longer on slower systems

### Build Failures

- Ensure all dependencies are installed
- Run `cargo build --release` manually to check for build issues

## Test Coverage

The integration tests cover:

- ✅ Command-line argument parsing
- ✅ Help and version output
- ✅ Error handling
- ✅ Logging functionality
- ✅ Process monitoring
- ✅ Process killing
- ✅ Multiple target handling
- ✅ Timing behavior
- ✅ Default values

## Adding New Tests

When adding new integration tests:

1. Use the helper functions provided (`build_binary`, `create_test_process`)
2. Clean up any processes you create
3. Use appropriate timeouts for process operations
4. Test both success and failure scenarios
5. Document any special requirements or assumptions
