# process-wick — The John Wick of Processes 💼🔫

![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)
![Built with Rust](https://img.shields.io/badge/Rust-🦀-orange)

_"This is the **Baba Yaga** of processes — mess with one, and the rest die."_

## What is **process-wick**?

**process-wick** is a lightweight, cross-platform process killer with a personal vendetta. It's primarily used to keep dangling child processes in check if the parent process dies.

Inspired by the iconic John Wick character, this tool keeps your system tidy by ensuring that when your "dog" process dies, the "targets" don’t survive for long. It's all about retribution🖤💥

## CLI Arguments

| Flag                | Description                                                                 |
|---------------------|-----------------------------------------------------------------------------|
| `--dog`             | PID to watch. When this process dies, the killing begins.                   |
| `--targets`         | List of PIDs to kill when the dog dies.                                     |
| `--vengeance-delay` | Time (in seconds) to wait before force-killing.                             |
| `--tick`            | How often (in seconds) to check if the dog is still alive.                  |

## Example Usage

```bash
process-wick \
  --dog 12345 \
  --targets 2222 3333 \
  --vengeance-delay 5 \
  --tick 1
```

Here’s what it does:

- 🔁 **Checks every 1 second** if PID `12345` (the dog) is alive.
- 💀 If the dog dies, it sends a warning to PIDs `2222` and `3333`.
- 🕰️ Waits **5 seconds** for a graceful exit (SIGTERM).
- 🔪 If any targets are still alive, it forcefully kills them (SIGKILL).
- 🧘 After all targets are dead, **process-wick** gracefully retires (exits).

## Features

- **Rust Powered:** Blazing-fast startup, minimal memory usage, and small binary size with zero runtime dependencies.
- **Safe and Predictable:** Memory-safe, thread-safe, and panic-free thanks to Rust’s strict guarantees.
- **Cross-platform**: Works on **Linux**, **macOS**, and **Windows**.
- **Target Killing**: Terminates all target processes if the monitored process exits.
- **Tick-Based Monitoring:** Checks the watched "dog" PID at regular intervals (--tick) for minimal overhead.
- **Graceful to Brutal**: Waits for processes to exit gracefully, then forcibly kills any remaining ones.
- **Self-Terminating:** Once the contract is fulfilled and all targets are gone, process-wick exits too. No footprints.

## Installation

1. Clone this repository.
2. Build using `cargo build --release`.
3. Enjoy cleaning up your system like a true process killer. 💼🔫

## Why this tool?

This started when I was building a [Tauri](https://tauri.app/) app with sidecar processes (i.e., auxiliary processes running alongside the app). These sidecar processes would get orphaned when the main app exited, becoming inherited by PID 1 and continuing to run aimlessly.

**process-wick** was made to solve this. It monitors your main process (the "dog"), and when it dies, it ensures the sidecar processes (the "targets") are properly terminated—either gracefully or forcefully after a timeout.

It soon evolved into a generic tool to keep orphaned processes in check. No mess. No surprises.

## Tauri Example

Since **process-wick** was originally built to manage Tauri sidecars, here’s a quick example of how to use it within a Tauri app:

1. Add `process-wick` as a [sidecar](https://v2.tauri.app/develop/sidecar/) in your `tauri.conf.json`, just like any other binary.
2. Add any additional sidecars your app depends on (e.g., long-running daemons or background services).
3. From your `setup()` function, spawn the sidecars like this:

   ```rs
   let (_rx, my_sidecar_process) = app
       .shell()
       .sidecar("my-sidecar")
       .unwrap()
       .spawn()
       .expect("Failed to spawn sidecar");

   app.shell()
       .sidecar("process-wick")
       .unwrap()
       .args(&["--targets", &my_sidecar_process.pid().to_string()])
       .spawn()
       .expect("Failed to spawn sidecar");
   ```

## License

MIT License. See [LICENSE](LICENSE) for more details.
