#[cfg(windows)]
use std::process::{Command, Stdio};

#[cfg(windows)]
const TASK_NAME: &str = "Fork Lite Win";

#[cfg(windows)]
fn powershell(script: &str) -> Result<std::process::Output, String> {
    use std::os::windows::process::CommandExt;
    Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(0x08000000)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
pub fn is_enabled() -> bool {
    let script = format!(
        "$task = Get-ScheduledTask -TaskName '{TASK_NAME}' -ErrorAction SilentlyContinue; if ($task -and $task.Settings.Enabled) {{ exit 0 }} else {{ exit 1 }}"
    );
    powershell(&script).is_ok_and(|output| output.status.success())
}

#[cfg(not(windows))]
pub fn is_enabled() -> bool {
    false
}

#[cfg(windows)]
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let script = if enabled {
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        let account = "[System.Security.Principal.WindowsIdentity]::GetCurrent().Name";
        let script = format!(
            "$ErrorActionPreference = 'Stop'; \
             $user = {account}; \
             $action = New-ScheduledTaskAction -Execute $env:FORK_LITE_EXE; \
             $trigger = New-ScheduledTaskTrigger -AtLogOn -User $user; \
             $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Highest; \
             $settings = New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries; \
             Register-ScheduledTask -TaskName '{TASK_NAME}' -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null"
        );
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .env("FORK_LITE_EXE", exe)
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| error.to_string())?;
        return check_output(output);
    } else {
        format!(
            "$ErrorActionPreference = 'Stop'; Unregister-ScheduledTask -TaskName '{TASK_NAME}' -Confirm:$false"
        )
    };
    check_output(powershell(&script)?)
}

#[cfg(windows)]
fn check_output(output: std::process::Output) -> Result<(), String> {
    if output.status.success() {
        Ok(())
    } else {
        let details = String::from_utf8_lossy(&output.stderr);
        Err(if details.trim().is_empty() {
            format!(
                "Не удалось изменить автозапуск (код {:?})",
                output.status.code()
            )
        } else {
            details.trim().to_owned()
        })
    }
}

#[cfg(not(windows))]
pub fn set_enabled(_enabled: bool) -> Result<(), String> {
    Err("Автозапуск доступен только в Windows".into())
}
