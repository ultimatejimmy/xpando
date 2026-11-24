/*
 * This file is part of espanso.
 *
 * Copyright (C) 2019-2021 Federico Terzi
 *
 * espanso is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * espanso is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with espanso.  If not, see <https://www.gnu.org/licenses/>.
 */

use anyhow::Result;
use log::{debug, info};
use std::path::{Path, PathBuf};

const ICON_BINARY: &[u8] = include_bytes!("res/icon.png");
const LOGO_NO_BACKGROUND_BINARY: &[u8] = include_bytes!("res/logo_no_background.png");

#[cfg(target_os = "windows")]
const WINDOWS_NORMAL_DARK_ICO_BINARY: &[u8] = include_bytes!("res/windows/normal_dark.ico");
#[cfg(target_os = "windows")]
const WINDOWS_NORMAL_LIGHT_ICO_BINARY: &[u8] = include_bytes!("res/windows/normal_light.ico");
#[cfg(target_os = "windows")]
const WINDOWS_DISABLED_LIGHT_ICO_BINARY: &[u8] = include_bytes!("res/windows/disabled_light.ico");
#[cfg(target_os = "windows")]
const WINDOWS_DISABLED_DARK_ICO_BINARY: &[u8] = include_bytes!("res/windows/disabled_dark.ico");
#[cfg(target_os = "windows")]
const WINDOWS_LOGO_ICO_BINARY: &[u8] = include_bytes!("res/windows/logo.ico");
#[cfg(target_os = "windows")]
const WINDOWS_TRAY_EXPLAIN_IMAGE: &[u8] = include_bytes!("res/windows/tray_explain_image.png");

#[cfg(target_os = "macos")]
const MAC_BINARY: &[u8] = include_bytes!("res/macos/icon.png");
#[cfg(target_os = "macos")]
const MAC_DISABLED_BINARY: &[u8] = include_bytes!("res/macos/icondisabled.png");
#[cfg(target_os = "macos")]
const MAC_SYSTEM_DISABLED_BINARY: &[u8] = include_bytes!("res/macos/iconsystemdisabled.png");
#[cfg(target_os = "macos")]
const MAC_ACCESSIBILITY_1_BINARY: &[u8] = include_bytes!("res/accessibility_1.png");
#[cfg(target_os = "macos")]
const MAC_ACCESSIBILITY_2_BINARY: &[u8] = include_bytes!("res/accessibility_2.png");
#[cfg(target_os = "macos")]
const MAC_STATUS_ICON_EXPLAIN_IMAGE: &[u8] = include_bytes!("res/macos/icon_explain_image.png");

#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct IconPaths {
    pub form_icon: Option<PathBuf>,
    pub search_icon: Option<PathBuf>,
    pub wizard_icon: Option<PathBuf>,

    pub tray_icon_normal: Option<PathBuf>,
    pub tray_icon_disabled: Option<PathBuf>,
    pub tray_icon_system_disabled: Option<PathBuf>,

    pub accessibility_image_1: Option<PathBuf>,
    pub accessibility_image_2: Option<PathBuf>,
    pub tray_explain_image: Option<PathBuf>,

    pub logo: Option<PathBuf>,
    pub logo_no_background: Option<PathBuf>,
}

#[cfg(target_os = "windows")]
pub fn load_icon_paths(runtime_dir: &Path) -> Result<IconPaths> {
    let light = taskbar_is_light();
    Ok(IconPaths {
        form_icon: Some(extract_icon(
            WINDOWS_LOGO_ICO_BINARY,
            &runtime_dir.join("formv3.ico"),
        )?),
        search_icon: Some(extract_icon(ICON_BINARY, &runtime_dir.join("search3.png"))?),
        wizard_icon: Some(extract_icon(
            WINDOWS_LOGO_ICO_BINARY,
            &runtime_dir.join("wizardv3.ico"),
        )?),
        tray_icon_normal: Some(extract_icon(
            if light {
                WINDOWS_NORMAL_LIGHT_ICO_BINARY
            } else {
                WINDOWS_NORMAL_DARK_ICO_BINARY
            },
            &runtime_dir.join(if light {
                "tray-normal-light.ico"
            } else {
                "tray-normal-dark.ico"
            }),
        )?),
        tray_icon_disabled: Some(extract_icon(
            if light {
                WINDOWS_DISABLED_LIGHT_ICO_BINARY
            } else {
                WINDOWS_DISABLED_DARK_ICO_BINARY
            },
            &runtime_dir.join(if light {
                "tray-disabled-light.ico"
            } else {
                "tray-disabled-dark.ico"
            }),
        )?),
        logo: Some(extract_icon(ICON_BINARY, &runtime_dir.join("iconv3.png"))?),
        logo_no_background: Some(extract_icon(
            LOGO_NO_BACKGROUND_BINARY,
            &runtime_dir.join("icon_no_backgroundv3.png"),
        )?),
        tray_explain_image: Some(extract_icon(
            WINDOWS_TRAY_EXPLAIN_IMAGE,
            &runtime_dir.join("tray_explain_image.png"),
        )?),
        ..Default::default()
    })
}

#[cfg(target_os = "macos")]
pub fn load_icon_paths(runtime_dir: &Path) -> Result<IconPaths> {
    Ok(IconPaths {
        search_icon: Some(extract_icon(
            ICON_BINARY,
            &runtime_dir.join("searchv2.png"),
        )?),
        tray_icon_normal: Some(extract_icon(MAC_BINARY, &runtime_dir.join("normalv2.png"))?),
        tray_icon_disabled: Some(extract_icon(
            MAC_DISABLED_BINARY,
            &runtime_dir.join("disabledv2.png"),
        )?),
        tray_icon_system_disabled: Some(extract_icon(
            MAC_SYSTEM_DISABLED_BINARY,
            &runtime_dir.join("systemdisabledv2.png"),
        )?),
        logo: Some(extract_icon(ICON_BINARY, &runtime_dir.join("iconv3.png"))?),
        logo_no_background: Some(extract_icon(
            LOGO_NO_BACKGROUND_BINARY,
            &runtime_dir.join("icon_no_background3.png"),
        )?),
        accessibility_image_1: Some(extract_icon(
            MAC_ACCESSIBILITY_1_BINARY,
            &runtime_dir.join("accessibility_1.png"),
        )?),
        accessibility_image_2: Some(extract_icon(
            MAC_ACCESSIBILITY_2_BINARY,
            &runtime_dir.join("accessibility_2.png"),
        )?),
        tray_explain_image: Some(extract_icon(
            MAC_STATUS_ICON_EXPLAIN_IMAGE,
            &runtime_dir.join("icon_explain_image.png"),
        )?),
        ..Default::default()
    })
}

#[cfg(target_os = "linux")]
pub fn load_icon_paths(runtime_dir: &Path) -> Result<IconPaths> {
    Ok(IconPaths {
        logo: Some(extract_icon(ICON_BINARY, &runtime_dir.join("iconv3.png"))?),
        search_icon: Some(extract_icon(ICON_BINARY, &runtime_dir.join("search3.png"))?),
        logo_no_background: Some(extract_icon(
            LOGO_NO_BACKGROUND_BINARY,
            &runtime_dir.join("icon_no_background3.png"),
        )?),
        ..Default::default()
    })
}

fn extract_icon(data: &[u8], target_file: &Path) -> Result<PathBuf> {
    if target_file.exists() {
        debug!(
            "skipping extraction for '{}', as it's already present",
            target_file.display()
        );
    } else {
        std::fs::write(target_file, data)?;
        info!("extracted icon to: {}", target_file.display());
    }

    Ok(target_file.to_owned())
}

/// Whether the Windows taskbar is light (so the tray icon must be dark). Read once, when the
/// engine starts. Defaults to dark when Windows does not say.
#[cfg(target_os = "windows")]
fn taskbar_is_light() -> bool {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "/v",
            "SystemUsesLightTheme",
        ])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("0x1"))
        .unwrap_or(false)
}
