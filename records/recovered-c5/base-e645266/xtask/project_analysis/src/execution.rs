use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunRequest {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub cwd: PathBuf,
    pub environment: BTreeMap<String, String>,
    pub timeout_ms: u64,
    pub max_output_bytes: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessObservation {
    pub request: RunRequest,
    pub executable_sha256: String,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: Option<i32>,
    pub termination: String,
    pub elapsed_ms: u128,
}

#[cfg(not(unix))]
pub fn run_bounded(_request: RunRequest) -> anyhow::Result<ProcessObservation> {
    anyhow::bail!("bounded process-tree execution is currently supported only on Unix")
}

#[cfg(unix)]
pub fn run_bounded(request: RunRequest) -> anyhow::Result<ProcessObservation> {
    use anyhow::{Context, ensure};
    use std::{
        os::unix::process::CommandExt,
        process::{Command, Stdio},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };
    ensure!(
        request.executable.is_absolute(),
        "executable must be absolute"
    );
    ensure!(
        request.cwd.is_absolute(),
        "working directory must be absolute"
    );
    ensure!(request.timeout_ms > 0, "timeout must be positive");
    let executable_sha256 = crate::artifact::sha256(&std::fs::read(&request.executable)?);
    let started = Instant::now();
    let child = Command::new(&request.executable)
        .args(&request.arguments)
        .current_dir(&request.cwd)
        .env_clear()
        .envs(&request.environment)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .context("start bounded process")?;
    let mut child = ProcessGroup {
        child,
        active: true,
    };
    let stdout = child.child.stdout.take().context("capture stdout")?;
    let stderr = child.child.stderr.take().context("capture stderr")?;
    let exceeded = Arc::new(AtomicBool::new(false));
    let expired = Arc::new(AtomicBool::new(false));
    let timeout = Duration::from_millis(request.timeout_ms);
    let output_limit = request.max_output_bytes;
    let stdout_exceeded = exceeded.clone();
    let stdout_expired = expired.clone();
    let stdout = thread::Builder::new()
        .name("project-report-stdout".into())
        .spawn(move || {
            read_bounded(
                stdout,
                output_limit,
                &stdout_exceeded,
                started,
                timeout,
                &stdout_expired,
            )
        })?;
    let stderr_exceeded = exceeded.clone();
    let stderr_expired = expired.clone();
    let stderr = thread::Builder::new()
        .name("project-report-stderr".into())
        .spawn(move || {
            read_bounded(
                stderr,
                output_limit,
                &stderr_exceeded,
                started,
                timeout,
                &stderr_expired,
            )
        })?;
    let mut termination = "completed";
    let status = loop {
        if exceeded.load(Ordering::Acquire) {
            termination = "output-limit";
            break child.stop()?;
        }
        if started.elapsed() >= Duration::from_millis(request.timeout_ms) {
            termination = "timeout";
            break child.stop()?;
        }
        if let Some(status) = child.child.try_wait()? {
            child.stop()?;
            break status;
        }
        thread::sleep(Duration::from_millis(5));
    };
    let stdout = stdout
        .join()
        .map_err(|_| anyhow::anyhow!("stdout reader panicked"))??;
    let stderr = stderr
        .join()
        .map_err(|_| anyhow::anyhow!("stderr reader panicked"))??;
    if termination == "completed" && exceeded.load(Ordering::Acquire) {
        termination = "output-limit";
    }
    if termination == "completed" && expired.load(Ordering::Acquire) {
        termination = "timeout";
    }
    Ok(ProcessObservation {
        request,
        executable_sha256,
        stdout,
        stderr,
        exit_code: status.code(),
        termination: termination.into(),
        elapsed_ms: started.elapsed().as_millis(),
    })
}

#[cfg(unix)]
fn read_bounded(
    mut stream: impl std::io::Read + std::os::fd::AsRawFd,
    limit: usize,
    exceeded: &std::sync::atomic::AtomicBool,
    started: std::time::Instant,
    timeout: std::time::Duration,
    expired: &std::sync::atomic::AtomicBool,
) -> std::io::Result<Vec<u8>> {
    use std::sync::atomic::Ordering;
    // A descendant can retain a pipe after leaving the child's process group.
    // Nonblocking reads keep the same deadline even after the direct child exits.
    let descriptor = stream.as_raw_fd();
    let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(std::io::Error::last_os_error());
    }
    let mut result = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        if started.elapsed() >= timeout {
            expired.store(true, Ordering::Release);
            return Ok(result);
        }
        let count = match stream.read(&mut buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(2));
                continue;
            }
            other => other?,
        };
        if count == 0 {
            return Ok(result);
        }
        let remaining = limit.saturating_sub(result.len());
        result.extend_from_slice(&buffer[..count.min(remaining)]);
        if count > remaining {
            exceeded.store(true, Ordering::Release);
        }
    }
}

#[cfg(unix)]
struct ProcessGroup {
    child: std::process::Child,
    active: bool,
}

#[cfg(unix)]
impl ProcessGroup {
    fn stop(&mut self) -> std::io::Result<std::process::ExitStatus> {
        let pid = i32::try_from(self.child.id()).map_err(std::io::Error::other)?;
        // process_group(0) gives this child a group containing only its descendants.
        let result = unsafe { libc::kill(-pid, libc::SIGKILL) };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error);
            }
        }
        let status = self.child.wait()?;
        self.active = false;
        Ok(status)
    }
}

#[cfg(unix)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        if self.active {
            let _ = self.stop();
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn command(script: &str) -> RunRequest {
        RunRequest {
            executable: "/bin/sh".into(),
            arguments: vec!["-c".into(), script.into()],
            cwd: std::env::temp_dir(),
            environment: BTreeMap::new(),
            timeout_ms: 1000,
            max_output_bytes: 64,
        }
    }

    #[test]
    fn compare_bounded_execution_retains_streams_and_numeric_failure() {
        let output = run_bounded(command("printf output; printf error >&2; exit 7")).unwrap();
        assert_eq!(output.stdout, b"output");
        assert_eq!(output.stderr, b"error");
        assert_eq!(output.exit_code, Some(7));
        assert_eq!(output.termination, "completed");
    }

    #[test]
    fn compare_bounded_execution_limits_time_and_output() {
        let mut request = command("sleep 10");
        request.timeout_ms = 30;
        let output = run_bounded(request).unwrap();
        assert_eq!(output.termination, "timeout");
        let output = run_bounded(command("while true; do printf 0123456789; done")).unwrap();
        assert_eq!(output.termination, "output-limit");
        assert_eq!(output.stdout.len(), 64);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn c4_boundary_inherited_pipe_still_has_a_deadline() {
        let mut request = command("/usr/bin/setsid /bin/sh -c 'printf held; sleep 1' & sleep 0.05");
        request.timeout_ms = 150;
        let output = run_bounded(request).unwrap();
        assert_eq!(output.termination, "timeout");
        assert_eq!(output.exit_code, Some(0));
        assert!(output.elapsed_ms < 750, "{}ms", output.elapsed_ms);
    }
}
