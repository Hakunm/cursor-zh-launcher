// SPDX-License-Identifier: GPL-3.0-only

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use anyhow::{Result, bail};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    Launch(LaunchOptions),
    Doctor,
    Repair,
    Restore,
    Version,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LaunchOptions {
    pub safe_mode: bool,
    pub native_menu: bool,
    pub untranslated_output: Option<PathBuf>,
    pub cursor_args: Vec<OsString>,
}

pub fn parse_args<I, S>(arguments: I) -> Result<Command>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let arguments = arguments.into_iter().map(Into::into).collect::<Vec<_>>();
    let mut options = LaunchOptions {
        native_menu: true,
        ..LaunchOptions::default()
    };
    let mut command = None;
    let mut index = 0;
    let mut pass_through_only = false;
    while index < arguments.len() {
        let argument = &arguments[index];
        if pass_through_only {
            options.cursor_args.push(argument.clone());
            index += 1;
            continue;
        }
        if argument == OsStr::new("--") {
            pass_through_only = true;
            index += 1;
            continue;
        }
        let value = argument.to_string_lossy();
        match value.as_ref() {
            "--zh-doctor" => set_command(&mut command, Command::Doctor)?,
            "--zh-repair" => set_command(&mut command, Command::Repair)?,
            "--zh-restore" => set_command(&mut command, Command::Restore)?,
            "--zh-version" => set_command(&mut command, Command::Version)?,
            "--zh-safe-mode" => options.safe_mode = true,
            "--zh-no-native-menu" => options.native_menu = false,
            "--zh-collect-untranslated" => {
                index += 1;
                let output = arguments
                    .get(index)
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from)
                    .ok_or_else(|| {
                        anyhow::anyhow!("--zh-collect-untranslated requires a file path")
                    })?;
                options.untranslated_output = Some(output);
            }
            value if value.starts_with("--zh-") => bail!("unknown launcher option: {value}"),
            _ => options.cursor_args.push(argument.clone()),
        }
        index += 1;
    }
    if let Some(command) = command {
        if options.safe_mode
            || !options.native_menu
            || options.untranslated_output.is_some()
            || !options.cursor_args.is_empty()
        {
            bail!("diagnostic commands cannot be combined with launch options or Cursor arguments");
        }
        Ok(command)
    } else {
        Ok(Command::Launch(options))
    }
}

fn set_command(slot: &mut Option<Command>, value: Command) -> Result<()> {
    if slot.is_some() {
        bail!("only one --zh-* command may be specified");
    }
    *slot = Some(value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwards_unknown_cursor_arguments() {
        let command = parse_args(["--new-window", "C:\\work tree", "--verbose"]).unwrap();
        let Command::Launch(options) = command else {
            panic!("expected launch command");
        };
        assert_eq!(
            options.cursor_args,
            ["--new-window", "C:\\work tree", "--verbose"]
                .map(OsString::from)
                .to_vec()
        );
    }

    #[test]
    fn parses_launcher_options_without_forwarding_them() {
        let command = parse_args([
            "--zh-safe-mode",
            "--zh-no-native-menu",
            "--zh-collect-untranslated",
            "report.json",
            "--",
            "--zh-doctor",
        ])
        .unwrap();
        let Command::Launch(options) = command else {
            panic!("expected launch command");
        };
        assert!(options.safe_mode);
        assert!(!options.native_menu);
        assert_eq!(
            options.untranslated_output,
            Some(PathBuf::from("report.json"))
        );
        assert_eq!(options.cursor_args, [OsString::from("--zh-doctor")]);
    }

    #[test]
    fn rejects_command_mixing_and_unknown_reserved_flags() {
        assert!(parse_args(["--zh-doctor", "--new-window"]).is_err());
        assert!(parse_args(["--zh-unknown"]).is_err());
        assert!(parse_args(["--zh-doctor", "--zh-version"]).is_err());
    }
}
