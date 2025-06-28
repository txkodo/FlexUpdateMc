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
