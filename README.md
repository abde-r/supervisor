<h1>supervisor</h1>
<p>
Supervisor is a lightweight, asynchronous process supervisor written in Rust. It reads a YAML configuration file to manage and monitor multiple subprocesses, making it ideal for orchestrating long-running services, daemons, or background jobs.
</p>
<br/>
<br/>
<div align="center">
<img alt="img" src="https://64.media.tumblr.com/f6bfd226824ab67f564ead715105c115/342b800b723aa237-52/s500x750/1a5a378fd34399015e3c8f71f1197fe2898d207e.png"/>
</div>
<br/>
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
<strong>Interactive Shell</strong><br/>
Supervisor includes an interactive command-line interface (CLI) that allows users to manage and monitor subprocesses in real-time. This interactive shell provides the following capabilities:

- View Program Status: Use the status command to display the current status of all managed programs.

- Control Programs: Start, stop, or restart individual programs or all programs collectively with commands like start, stop, and restart.

- Reload Configuration: Apply changes from the configuration file at runtime without restarting the supervisor using the reload command.

- Graceful Shutdown: Terminate the supervisor and all managed programs cleanly with the quit command.

- Help Command: Type help to view a list of available commands and their descriptions.
<br/>

<strong>YAML-based Configuration:</strong> Define multiple programs with customizable settings.

<br/>

<strong>Concurrent Process Management:</strong> Spawn and manage multiple processes asynchronously using Tokio.

<br/>

<strong>Dynamic Configuration Reloading:</strong> Apply new configurations at runtime without restarting the supervisor.

<br/>

<strong>Graceful Shutdown:</strong> Handle termination signals to stop processes cleanly.

<br/>

<strong>Environment Variable Support:</strong> Set custom environment variables for each managed process.

<br/>

