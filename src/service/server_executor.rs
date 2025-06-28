use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

pub struct ServerExecutionConfig<'a> {
    pub java_path: Option<&'a Path>,
    pub jar_file_name: String,
    pub port: u16,
    pub cwd: PathBuf,
}

pub trait ServerExecutor {
    fn start(&mut self, config: ServerExecutionConfig) -> Result<(), String>;
    fn stop(&mut self) -> Result<(), String>;
    fn send_command(&mut self, command: &str) -> Result<(), String>;
}

pub struct ChildProcessServerExecutor {
    child: Option<Child>,
    stdin: Option<BufWriter<std::process::ChildStdin>>,
}

impl ChildProcessServerExecutor {
    pub fn new() -> Self {
        Self {
            child: None,
            stdin: None,
        }
    }
}

impl ServerExecutor for ChildProcessServerExecutor {
    fn start(&mut self, config: ServerExecutionConfig) -> Result<(), String> {
        if self.child.is_some() {
            return Err("Server is already running".to_string());
        }

        let java_cmd = config
            .java_path
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "java".to_string());

        let mut command = Command::new(java_cmd);
        command
            .arg("-jar")
            .arg(&config.jar_file_name)
            .arg("--port")
            .arg(config.port.to_string())
            .arg("nogui")
            .current_dir(&config.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(|e| format!("Failed to start server: {}", e))?;

        let stdin = child.stdin.take().ok_or("Failed to get stdin handle")?;

        self.stdin = Some(BufWriter::new(stdin));
        self.child = Some(child);

        Ok(())
    }

    fn stop(&mut self) -> Result<(), String> {
        if let Some(ref mut stdin) = self.stdin {
            stdin
                .write_all(b"stop\n")
                .map_err(|e| format!("Failed to send stop command: {}", e))?;
            stdin
                .flush()
                .map_err(|e| format!("Failed to flush stdin: {}", e))?;
        }

        if let Some(mut child) = self.child.take() {
            child
                .wait()
                .map_err(|e| format!("Failed to wait for server process: {}", e))?;
        }

        self.stdin = None;
        Ok(())
    }

    fn send_command(&mut self, command: &str) -> Result<(), String> {
        if let Some(ref mut stdin) = self.stdin {
            stdin
                .write_all(command.as_bytes())
                .map_err(|e| format!("Failed to send command: {}", e))?;
            stdin
                .write_all(b"\n")
                .map_err(|e| format!("Failed to send newline: {}", e))?;
            stdin
                .flush()
                .map_err(|e| format!("Failed to flush stdin: {}", e))?;
            Ok(())
        } else {
            Err("Server is not running".to_string())
        }
    }
}

#[derive(Debug)]
pub struct MockServerExecutor {
    is_running: bool,
    commands_sent: Vec<String>,
    should_fail_start: bool,
    should_fail_stop: bool,
    should_fail_command: bool,
}

impl MockServerExecutor {
    pub fn new() -> Self {
        Self {
            is_running: false,
            commands_sent: Vec::new(),
            should_fail_start: false,
            should_fail_stop: false,
            should_fail_command: false,
        }
    }

    pub fn with_start_failure() -> Self {
        Self {
            should_fail_start: true,
            ..Self::new()
        }
    }

    pub fn with_stop_failure() -> Self {
        Self {
            should_fail_stop: true,
            ..Self::new()
        }
    }

    pub fn with_command_failure() -> Self {
        Self {
            should_fail_command: true,
            ..Self::new()
        }
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }

    pub fn commands_sent(&self) -> &[String] {
        &self.commands_sent
    }

    pub fn clear_commands(&mut self) {
        self.commands_sent.clear();
    }
}

impl Default for MockServerExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerExecutor for MockServerExecutor {
    fn start(&mut self, _config: ServerExecutionConfig) -> Result<(), String> {
        if self.should_fail_start {
            return Err("Mock start failure".to_string());
        }

        if self.is_running {
            return Err("Server is already running".to_string());
        }

        self.is_running = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), String> {
        if self.should_fail_stop {
            return Err("Mock stop failure".to_string());
        }

        self.is_running = false;
        Ok(())
    }

    fn send_command(&mut self, command: &str) -> Result<(), String> {
        if self.should_fail_command {
            return Err("Mock command failure".to_string());
        }

        if !self.is_running {
            return Err("Server is not running".to_string());
        }

        self.commands_sent.push(command.to_string());
        Ok(())
    }
}
