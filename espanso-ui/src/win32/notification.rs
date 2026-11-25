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

use std::{
    path::PathBuf,
    sync::{
        mpsc::{channel, Sender},
        Arc, Mutex,
    },
};

use anyhow::{anyhow, bail, Result};
use log::{error, warn};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::LazyLock;
use tauri_winrt_notification::{IconCrop, Toast};

/// The name Windows shows above each notification comes from this ID's registration, so xpando
/// registers its own (per user, no installer needed) instead of borrowing espanso's.
const XPANDO_APP_USER_MODEL_ID: &str = "app.xpando.desktop";

fn register_app_user_model_id(icon_path: &std::path::Path) {
    let key = format!(r"HKCU\Software\Classes\AppUserModelId\{XPANDO_APP_USER_MODEL_ID}");
    let icon = icon_path.to_string_lossy().to_string();
    for (name, value) in [("DisplayName", "xpando"), ("IconUri", icon.as_str())] {
        let result = Command::new("reg")
            .args(["add", &key, "/v", name, "/t", "REG_SZ", "/d", value, "/f"])
            .creation_flags(0x0800_0000)
            .output();
        if let Err(err) = result {
            warn!("unable to register the notification name: {}", err);
            return;
        }
    }
}

static SEND_CHANNEL: LazyLock<Arc<Mutex<Option<Sender<String>>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(None)));

pub fn initialize_notification_thread(notification_icon_path: PathBuf) -> Result<()> {
    let (sender, receiver) = channel::<String>();

    {
        let mut lock = SEND_CHANNEL
            .lock()
            .map_err(|e| anyhow!("failed to define shared notification sender: {}", e))?;
        *lock = Some(sender);
    }

    std::thread::Builder::new()
        .name("notification-thread".to_string())
        .spawn(move || {
            register_app_user_model_id(&notification_icon_path);

            while let Ok(message) = receiver.recv() {
                if let Err(err) = Toast::new(XPANDO_APP_USER_MODEL_ID)
                    .icon(&notification_icon_path, IconCrop::Square, "xpando")
                    .title("xpando")
                    .text1(&message)
                    .sound(None)
                    .show()
                    .map_err(|e| anyhow!("failed to show notification: {}", e))
                {
                    error!("unable to show notification: {}", err);
                }
            }
        })?;

    Ok(())
}

pub fn show_notification(msg: &str) -> Result<()> {
    let mut lock = SEND_CHANNEL
        .lock()
        .map_err(|e| anyhow!("unable to acquire notification send channel: {}", e))?;
    match &mut *lock {
        Some(sender) => {
            sender.send(msg.to_string())?;
            Ok(())
        }
        None => bail!("notification sender not available"),
    }
}
