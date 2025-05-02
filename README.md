<h1>supervisor</h1>

Supervisor is a lightweight, asynchronous process supervisor written in Rust. It reads a YAML configuration file to manage and monitor multiple subprocesses, making it ideal for orchestrating long-running services, daemons, or background jobs.

<h3>Features</h3>
YAML-based Configuration: Define multiple programs with customizable settings.

Concurrent Process Management: Spawn and manage multiple processes asynchronously using Tokio.

Dynamic Configuration Reloading: Apply new configurations at runtime without restarting the supervisor.

Graceful Shutdown: Handle termination signals to stop processes cleanly.

Environment Variable Support: Set custom environment variables for each managed process.

<h3>Getting Started</h3>
<h4>Prerequisites</h4>
Rust (edition 2021)

Cargo package manager

<h4>Build the project:</h4>
<p><code>cargo build --release</code></p>
Create a configuration file in YAML format:

```
programs:
  example_program:
    cmd: "echo Hello, World!"
    numprocs: 1
    autostart: true
    autorestart: "always"
    startretries: 3
    starttime: 1
    stopsignal: "TERM"
    stoptime: 5
    stdout: "logs/example_program.out"
    stderr: "logs/example_program.err"
    env:
      ENV_VAR: "value"
```

<h3>License</h3>
This project is licensed under the MIT License. See the LICENSE file for details.
