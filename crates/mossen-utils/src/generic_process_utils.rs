//! Generic process utilities — platform-agnostic process inspection.

use std::process::Command;
use tokio::process::Command as AsyncCommand;

/// Check if a process with the given PID is running (signal 0 probe).
/// PID <= 1 returns false.
pub fn is_process_running(pid: u32) -> bool {
    if pid <= 1 {
        return false;
    }
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        // On non-Unix, try a different approach
        false
    }
}

/// Gets the ancestor process chain for a given process (up to max_depth levels).
/// Returns array of ancestor PIDs from immediate parent to furthest ancestor.
pub async fn get_ancestor_pids_async(pid: u32, max_depth: usize) -> Vec<u32> {
    if cfg!(target_os = "windows") {
        let script = format!(
            "$pid = {}; $ancestors = @(); for ($i = 0; $i -lt {}; $i++) {{ $proc = Get-CimInstance Win32_Process -Filter \"ProcessId=$pid\" -ErrorAction SilentlyContinue; if (-not $proc -or -not $proc.ParentProcessId -or $proc.ParentProcessId -eq 0) {{ break }}; $pid = $proc.ParentProcessId; $ancestors += $pid }}; $ancestors -join ','",
            pid, max_depth
        );

        let result = AsyncCommand::new("powershell.exe")
            .args(["-NoProfile", "-Command", &script])
            .output()
            .await;

        match result {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if stdout.is_empty() {
                    return Vec::new();
                }
                stdout
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.parse::<u32>().ok())
                    .collect()
            }
            _ => Vec::new(),
        }
    } else {
        let script = format!(
            "pid={}; for i in $(seq 1 {}); do ppid=$(ps -o ppid= -p $pid 2>/dev/null | tr -d ' '); if [ -z \"$ppid\" ] || [ \"$ppid\" = \"0\" ] || [ \"$ppid\" = \"1\" ]; then break; fi; echo $ppid; pid=$ppid; done",
            pid, max_depth
        );

        let result = AsyncCommand::new("sh").args(["-c", &script]).output().await;

        match result {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if stdout.is_empty() {
                    return Vec::new();
                }
                stdout
                    .lines()
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.parse::<u32>().ok())
                    .collect()
            }
            _ => Vec::new(),
        }
    }
}

/// Gets the command line for a given process.
pub fn get_process_command(pid: u32) -> Option<String> {
    let command = if cfg!(target_os = "windows") {
        format!(
            "powershell.exe -NoProfile -Command \"(Get-CimInstance Win32_Process -Filter \\\"ProcessId={}\\\").CommandLine\"",
            pid
        )
    } else {
        format!("ps -o command= -p {}", pid)
    };

    let output = Command::new("sh").args(["-c", &command]).output().ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() {
            None
        } else {
            Some(stdout)
        }
    } else {
        None
    }
}

/// Gets the command lines for a process and its ancestors in a single call.
pub async fn get_ancestor_commands_async(pid: u32, max_depth: usize) -> Vec<String> {
    if cfg!(target_os = "windows") {
        let script = format!(
            "$currentPid = {}; $commands = @(); for ($i = 0; $i -lt {}; $i++) {{ $proc = Get-CimInstance Win32_Process -Filter \"ProcessId=$currentPid\" -ErrorAction SilentlyContinue; if (-not $proc) {{ break }}; if ($proc.CommandLine) {{ $commands += $proc.CommandLine }}; if (-not $proc.ParentProcessId -or $proc.ParentProcessId -eq 0) {{ break }}; $currentPid = $proc.ParentProcessId }}; $commands -join [char]0",
            pid, max_depth
        );

        let result = AsyncCommand::new("powershell.exe")
            .args(["-NoProfile", "-Command", &script])
            .output()
            .await;

        match result {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if stdout.is_empty() {
                    return Vec::new();
                }
                stdout
                    .split('\0')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect()
            }
            _ => Vec::new(),
        }
    } else {
        let script = format!(
            "currentpid={}; for i in $(seq 1 {}); do cmd=$(ps -o command= -p $currentpid 2>/dev/null); if [ -n \"$cmd\" ]; then printf '%s\\0' \"$cmd\"; fi; ppid=$(ps -o ppid= -p $currentpid 2>/dev/null | tr -d ' '); if [ -z \"$ppid\" ] || [ \"$ppid\" = \"0\" ] || [ \"$ppid\" = \"1\" ]; then break; fi; currentpid=$ppid; done",
            pid, max_depth
        );

        let result = AsyncCommand::new("sh").args(["-c", &script]).output().await;

        match result {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if stdout.is_empty() {
                    return Vec::new();
                }
                stdout
                    .split('\0')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect()
            }
            _ => Vec::new(),
        }
    }
}

/// Gets the child process IDs for a given process.
pub fn get_child_pids(pid: u32) -> Vec<u32> {
    let command = if cfg!(target_os = "windows") {
        format!(
            "powershell.exe -NoProfile -Command \"(Get-CimInstance Win32_Process -Filter \\\"ParentProcessId={}\\\").ProcessId\"",
            pid
        )
    } else {
        format!("pgrep -P {}", pid)
    };

    let output = match Command::new("sh").args(["-c", &command]).output() {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    if !output.status.success() {
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    stdout
        .lines()
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u32>().ok())
        .collect()
}
