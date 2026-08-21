#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

// SPDX-License-Identifier: GPL-3.0-only

mod windows_ui;

use std::env;
use std::time::Duration;

use anyhow::Result;
use cursor_zh_core::cli::{Command, parse_args};
use cursor_zh_core::config::ConfigStore;
use cursor_zh_core::cursor::detect_cursor;
use cursor_zh_core::inspector::{build_main_menu_script, install_main_menu_hook};
use cursor_zh_core::language_pack::LanguagePackManager;
use cursor_zh_core::process::{find_existing_cursor_processes, launch_cursor};
use cursor_zh_core::renderer::{
    RendererWatchdog, build_renderer_script, validate_profile_signatures, wait_for_first_injection,
};
use cursor_zh_core::update::{UpdateChecker, default_state_path};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        windows_ui::report_error(&format!("{error:#}"));
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let command = parse_args(env::args_os().skip(1))?;
    match command {
        Command::Version => {
            println!(
                "{} {}",
                cursor_zh_core::PRODUCT_NAME,
                env!("CARGO_PKG_VERSION")
            );
        }
        Command::Doctor => print_doctor()?,
        Command::Launch(options) => launch(options).await?,
        Command::Repair => repair_language_state()?,
        Command::Restore => restore_language_state()?,
    }
    Ok(())
}

async fn launch(options: cursor_zh_core::cli::LaunchOptions) -> Result<()> {
    let store = ConfigStore::default_for_current_user()?;
    let config = store.load_or_default()?;
    spawn_update_check(config.update_check);
    let installation = detect_cursor(config.cursor_path.as_deref())?;
    let metadata = installation.metadata()?;
    let catalog = cursor_zh_core::embedded_translation_catalog()?;
    let issues = catalog.validate(true);
    if !issues.is_empty() {
        anyhow::bail!("翻译目录未通过发布校验：{issues:?}");
    }
    let compatibility = cursor_zh_core::embedded_compatibility_catalog()?;
    compatibility.validate().map_err(anyhow::Error::msg)?;
    let profile = compatibility.find_by_commit(&metadata.commit);

    prepare_language_nonfatal(&installation, &catalog)?;

    let renderer_enabled = config.renderer_injection && !options.safe_mode;
    let native_menu_enabled =
        config.native_menu_injection && options.native_menu && !options.safe_mode;
    let main_menu_script = native_menu_enabled
        .then(|| build_main_menu_script(&catalog, env!("CARGO_PKG_VERSION")))
        .transpose()?;
    let mut launched = launch_cursor(
        &installation,
        &options.cursor_args,
        renderer_enabled,
        native_menu_enabled,
    )?;
    println!(
        "已启动 Cursor {}（PID {}）",
        metadata.cursor_version,
        launched.child.id()
    );
    if native_menu_enabled && profile.is_none() {
        eprintln!("当前 Cursor 版本尚未认证，原生菜单仅应用精确标签规则");
    }

    if let (Some(inspector_port), Some(script)) =
        (launched.main_inspector_port, main_menu_script.as_deref())
    {
        match install_main_menu_hook(inspector_port, script, 20).await {
            Ok(report) => println!(
                "原生菜单汉化已就绪：Inspector target {}，主进程已恢复",
                report.target_id
            ),
            Err(error) => {
                eprintln!("原生菜单早期注入失败，将安全降级重启：{error:#}");
                terminate_launched_child(&mut launched.child)?;
                wait_for_cursor_shutdown(&installation).await?;
                launched =
                    launch_cursor(&installation, &options.cursor_args, renderer_enabled, false)?;
                println!("已在不暂停主进程的降级模式下重启 Cursor");
            }
        }
    }

    if let Some(debug_port) = launched.renderer_debug_port {
        if let Some(profile) = profile {
            match validate_profile_signatures(debug_port, profile, 40).await {
                Ok(true) => {}
                Ok(false) => {
                    eprintln!("Renderer 结构签名不匹配，仅继续使用精确作用域规则");
                }
                Err(error) => {
                    eprintln!("Renderer 结构签名检查失败，仅继续使用精确作用域规则：{error:#}");
                }
            }
        }
        let script = build_renderer_script(
            &catalog,
            env!("CARGO_PKG_VERSION"),
            options.untranslated_output.is_some(),
        )?;
        let mut watchdog =
            RendererWatchdog::new(debug_port, script, options.untranslated_output.clone());
        match wait_for_first_injection(&mut watchdog, 40).await {
            Ok(report) => println!(
                "Renderer 汉化已就绪：{} 个 Workbench target，{} 个新注入",
                report.workbench_targets,
                report.injected.len()
            ),
            Err(error) => eprintln!("Cursor 已启动，但 Renderer 汉化尚未就绪：{error:#}"),
        }
        watchdog.run_until_exit(&mut launched.child).await?;
    } else {
        wait_for_exit(&mut launched.child).await?;
    }
    Ok(())
}

fn prepare_language_nonfatal(
    installation: &cursor_zh_core::cursor::CursorInstallation,
    catalog: &cursor_zh_core::translation::TranslationCatalog,
) -> Result<()> {
    if let Err(error) =
        LanguagePackManager::default_for_current_user()?.prepare(installation, catalog)
    {
        windows_ui::report_warning(&format!(
            "官方中文语言准备失败，将继续使用运行时汉化启动 Cursor。\n\n{error:#}"
        ));
    }
    Ok(())
}

fn spawn_update_check(enabled: bool) {
    if !enabled {
        return;
    }
    let Some(api_url) = option_env!("CURSOR_ZH_RELEASES_API").filter(|value| !value.is_empty())
    else {
        return;
    };
    let Some(local_app_data) = env::var_os("LOCALAPPDATA") else {
        return;
    };
    let state_path = default_state_path(std::path::Path::new(&local_app_data));
    let api_url = api_url.to_string();
    tokio::spawn(async move {
        let result = async {
            let checker = UpdateChecker::new(api_url, state_path, env!("CARGO_PKG_VERSION"))?;
            checker.check_if_due().await
        }
        .await;
        if let Ok(Some(update)) = result {
            windows_ui::prompt_update(&update.tag_name, &update.release_url);
        }
    });
}

async fn wait_for_exit(child: &mut std::process::Child) -> Result<()> {
    loop {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn terminate_launched_child(child: &mut std::process::Child) -> Result<()> {
    if child.try_wait()?.is_none() {
        child.kill()?;
    }
    let _ = child.wait()?;
    Ok(())
}

async fn wait_for_cursor_shutdown(
    installation: &cursor_zh_core::cursor::CursorInstallation,
) -> Result<()> {
    for _ in 0..20 {
        if find_existing_cursor_processes(installation).is_empty() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    anyhow::bail!("本次 Cursor 子进程未在降级重启前完全退出")
}

fn print_doctor() -> Result<()> {
    let store = ConfigStore::default_for_current_user()?;
    let config = store.load_or_default()?;
    let installation = detect_cursor(config.cursor_path.as_deref())?;
    let metadata = installation.metadata()?;
    let language_manager = LanguagePackManager::default_for_current_user()?;
    let language = language_manager.inspect()?;
    let translations = cursor_zh_core::embedded_translation_catalog()?;
    let translation_issues = translations.validate(true);
    let compatibility = cursor_zh_core::embedded_compatibility_catalog()?;
    compatibility.validate().map_err(anyhow::Error::msg)?;
    let profile = compatibility.find_by_commit(&metadata.commit);
    let report = serde_json::json!({
        "launcherVersion": env!("CARGO_PKG_VERSION"),
        "updateSourceConfigured": option_env!("CURSOR_ZH_RELEASES_API").is_some_and(|value| !value.is_empty()),
        "configPath": store.path(),
        "cursor": {
            "executable": installation.executable,
            "version": metadata.cursor_version,
            "vscodeVersion": metadata.vscode_version,
            "commit": metadata.commit,
            "applicationName": metadata.application_name,
            "dataFolderName": metadata.data_folder_name,
        },
        "features": {
            "rendererInjection": config.renderer_injection,
            "nativeMenuInjection": config.native_menu_injection,
            "updateCheck": config.update_check,
        },
        "language": language,
        "catalog": {
            "nlsEntries": translations.nls.len(),
            "runtimeEntries": translations.runtime.len(),
            "validationIssues": translation_issues,
        },
        "compatibility": {
            "profile": profile.map(|value| value.id.as_str()),
            "nativeMenuCertified": profile.is_some_and(|value| value.native_menu_supported),
        },
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn repair_language_state() -> Result<()> {
    let store = ConfigStore::default_for_current_user()?;
    let config = store.load_or_default()?;
    let installation = detect_cursor(config.cursor_path.as_deref())?;
    let catalog = cursor_zh_core::embedded_translation_catalog()?;
    let issues = catalog.validate(true);
    if !issues.is_empty() {
        anyhow::bail!("翻译目录未通过发布校验：{issues:?}");
    }
    let manager = LanguagePackManager::default_for_current_user()?;
    let report = manager.prepare(&installation, &catalog)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn restore_language_state() -> Result<()> {
    let manager = LanguagePackManager::default_for_current_user()?;
    let report = manager.restore()?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
