// SPDX-License-Identifier: GPL-3.0-only

use std::ffi::OsString;
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use sysinfo::{ProcessesToUpdate, System};

use crate::cursor::CursorInstallation;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExistingCursorProcess {
    pub process_id: u32,
    pub executable: PathBuf,
}

#[derive(Debug)]
pub struct LaunchedCursor {
    pub child: Child,
    pub renderer_debug_port: Option<u16>,
    pub main_inspector_port: Option<u16>,
    pub command: Vec<OsString>,
}

pub fn select_loopback_port() -> Result<u16> {
    let listener =
        TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).context("failed to reserve a loopback port")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

#[must_use]
pub fn find_existing_cursor_processes(
    installation: &CursorInstallation,
) -> Vec<ExistingCursorProcess> {
    let mut system = System::new_all();
    system.refresh_processes(ProcessesToUpdate::All, true);
    let expected = comparable_path(&installation.executable);
    system
        .processes()
        .iter()
        .filter_map(|(pid, process)| {
            if !process
                .name()
                .to_string_lossy()
                .eq_ignore_ascii_case("Cursor.exe")
            {
                return None;
            }
            let executable = process.exe()?.to_path_buf();
            (comparable_path(&executable) == expected).then(|| ExistingCursorProcess {
                process_id: pid.as_u32(),
                executable,
            })
        })
        .collect()
}

pub fn launch_cursor(
    installation: &CursorInstallation,
    cursor_args: &[OsString],
    enable_renderer_debugging: bool,
    enable_main_inspector: bool,
) -> Result<LaunchedCursor> {
    let existing = find_existing_cursor_processes(installation);
    if !existing.is_empty() {
        let process_ids = existing
            .iter()
            .map(|process| process.process_id.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        bail!("Cursor is already running (PID: {process_ids}); fully exit Cursor and retry");
    }
    let renderer_debug_port = enable_renderer_debugging
        .then(select_loopback_port)
        .transpose()?;
    let main_inspector_port = enable_main_inspector
        .then(select_loopback_port)
        .transpose()?;
    if renderer_debug_port.is_some() && renderer_debug_port == main_inspector_port {
        bail!("renderer and main Inspector selected the same loopback port");
    }
    let arguments = build_cursor_arguments(renderer_debug_port, main_inspector_port, cursor_args);
    let mut command = Command::new(&installation.executable);
    command
        .args(&arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let child = command.spawn().with_context(|| {
        format!(
            "failed to launch Cursor executable {}",
            installation.executable.display()
        )
    })?;
    let mut rendered_command = vec![installation.executable.as_os_str().to_os_string()];
    rendered_command.extend(arguments);
    Ok(LaunchedCursor {
        child,
        renderer_debug_port,
        main_inspector_port,
        command: rendered_command,
    })
}

#[must_use]
pub fn build_cursor_arguments(
    renderer_debug_port: Option<u16>,
    main_inspector_port: Option<u16>,
    cursor_args: &[OsString],
) -> Vec<OsString> {
    let mut arguments = vec![OsString::from("--locale=zh-cn")];
    if let Some(port) = renderer_debug_port {
        arguments.extend([
            OsString::from("--remote-debugging-address=127.0.0.1"),
            OsString::from(format!("--remote-debugging-port={port}")),
            OsString::from(format!("--remote-allow-origins=http://127.0.0.1:{port}")),
        ]);
    }
    if let Some(port) = main_inspector_port {
        arguments.push(OsString::from(format!("--inspect-brk=127.0.0.1:{port}")));
    }
    arguments.extend(cursor_args.iter().cloned());
    arguments
}

fn comparable_path(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('/', "\\")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_local_debug_arguments_before_passthrough() {
        let arguments = build_cursor_arguments(
            Some(49173),
            Some(49174),
            &[
                OsString::from("--new-window"),
                OsString::from("C:\\work tree"),
            ],
        );
        assert_eq!(
            arguments,
            [
                "--locale=zh-cn",
                "--remote-debugging-address=127.0.0.1",
                "--remote-debugging-port=49173",
                "--remote-allow-origins=http://127.0.0.1:49173",
                "--inspect-brk=127.0.0.1:49174",
                "--new-window",
                "C:\\work tree",
            ]
            .map(OsString::from)
            .to_vec()
        );
    }

    #[test]
    fn safe_mode_omits_debugging_flags() {
        let arguments = build_cursor_arguments(None, None, &[OsString::from("--verbose")]);
        assert_eq!(
            arguments,
            ["--locale=zh-cn", "--verbose"].map(OsString::from).to_vec()
        );
    }

    #[test]
    fn selected_ports_are_nonzero() {
        assert_ne!(select_loopback_port().unwrap(), 0);
    }

    #[test]
    fn path_comparison_normalizes_windows_prefix_and_case() {
        assert_eq!(
            comparable_path(Path::new(r"\\?\C:\Apps\Cursor.exe")),
            comparable_path(Path::new(r"c:/apps/cursor.exe"))
        );
    }
}
