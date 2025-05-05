<h1>supervisor</h1>
<p>
Supervisor is a lightweight, asynchronous process supervisor written in Rust. It reads a YAML configuration file to manage and monitor multiple subprocesses, making it ideal for orchestrating long-running services, daemons, or background jobs.
</p>
<br/>
<br/>
<div align="center">
<img alt="img" src="https://64.media.tumblr.com/f6bfd226824ab67f564ead715105c115/342b800b723aa237-52/s500x750/1a5a378fd34399015e3c8f71f1197fe2898d207e.png"/>
</div>
<p>
In Unix and Unix-like operating systems, job control refers to control of jobs by a shell,
especially interactively, where a "job" is a shell’s representation for a process group. Basic
job control features are the suspending, resuming, or terminating of all processes in the
job/process group; more advanced features can be performed by sending signals to the
job. Job control is of particular interest in Unix due to its multiprocessing, and should
be distinguished from job control generally, which is frequently applied to sequential
execution (batch processing).
</p>

<br/>
<h3>Features</h3>
YAML-based Configuration: Define multiple programs with customizable settings.

Concurrent Process Management: Spawn and manage multiple processes asynchronously using Tokio.

Dynamic Configuration Reloading: Apply new configurations at runtime without restarting the supervisor.

Graceful Shutdown: Handle termination signals to stop processes cleanly.

Environment Variable Support: Set custom environment variables for each managed process.

<br/>
<h3>Getting Started</h3>
<h4>Prerequisites</h4>
<p>- Rust (edition 2021)</p>
<p>- Cargo package manager</p>

<br/>
<h4>Build the project:</h4>
<p><code>cargo build --release</code></p>
create a configuration file in YAML format, i.e:

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
