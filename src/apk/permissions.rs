//! Permission metadata: protection-level classification + one-line help.
//!
//! The manifest only carries permission *names*; the level table below covers
//! the common AOSP permissions so the UI can flag dangerous ones. Unknown
//! permissions (OEM / app-defined) degrade to `Unknown` with a generic hint
//! instead of being hidden.

use serde::{Deserialize, Serialize};

/// Protection bucket shown in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PermissionLevel {
    Normal,
    Dangerous,
    Signature,
    #[default]
    Unknown,
}

impl PermissionLevel {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Dangerous => "Dangerous",
            Self::Signature => "Signature",
            Self::Unknown => "Unknown",
        }
    }
}

/// One permission row for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionInfo {
    pub name: String,
    pub level: PermissionLevel,
    pub description: String,
}

pub fn classify_permission(name: &str) -> PermissionLevel {
    match name {
        // --- Dangerous (runtime-granted) ---
        n if n.ends_with(".CAMERA")
            || n.ends_with(".RECORD_AUDIO")
            || n.ends_with(".ACCESS_FINE_LOCATION")
            || n.ends_with(".ACCESS_COARSE_LOCATION")
            || n.ends_with(".ACCESS_BACKGROUND_LOCATION")
            || n.ends_with(".READ_CONTACTS")
            || n.ends_with(".WRITE_CONTACTS")
            || n.ends_with(".GET_ACCOUNTS")
            || n.ends_with(".READ_CALENDAR")
            || n.ends_with(".WRITE_CALENDAR")
            || n.ends_with(".READ_CALL_LOG")
            || n.ends_with(".WRITE_CALL_LOG")
            || n.ends_with(".READ_PHONE_STATE")
            || n.ends_with(".READ_PHONE_NUMBERS")
            || n.ends_with(".CALL_PHONE")
            || n.ends_with(".ADD_VOICEMAIL")
            || n.ends_with(".USE_SIP")
            || n.ends_with(".SEND_SMS")
            || n.ends_with(".RECEIVE_SMS")
            || n.ends_with(".READ_SMS")
            || n.ends_with(".RECEIVE_MMS")
            || n.ends_with(".RECEIVE_WAP_PUSH")
            || n.ends_with(".READ_EXTERNAL_STORAGE")
            || n.ends_with(".WRITE_EXTERNAL_STORAGE")
            || n.ends_with(".READ_MEDIA_IMAGES")
            || n.ends_with(".READ_MEDIA_VIDEO")
            || n.ends_with(".READ_MEDIA_AUDIO")
            || n.ends_with(".ACCESS_MEDIA_LOCATION")
            || n.ends_with(".BODY_SENSORS")
            || n.ends_with(".BODY_SENSORS_BACKGROUND")
            || n.ends_with(".ACTIVITY_RECOGNITION")
            || n.ends_with(".READ_PHONE_STATE")
            || n.ends_with(".POST_NOTIFICATIONS")
            || n.ends_with(".NEARBY_WIFI_DEVICES")
            || n.ends_with(".BLUETOOTH_SCAN")
            || n.ends_with(".BLUETOOTH_CONNECT")
            || n.ends_with(".BLUETOOTH_ADVERTISE") =>
        {
            PermissionLevel::Dangerous
        }
        // --- Signature / privileged ---
        n if n.ends_with(".MANAGE_EXTERNAL_STORAGE")
            || n.ends_with(".SYSTEM_ALERT_WINDOW")
            || n.ends_with(".WRITE_SETTINGS")
            || n.ends_with(".REQUEST_INSTALL_PACKAGES")
            || n.ends_with(".PACKAGE_USAGE_STATS")
            || n.ends_with(".BIND_ACCESSIBILITY_SERVICE")
            || n.ends_with(".BIND_DEVICE_ADMIN")
            || n.ends_with(".MANAGE_DEVICE_POLICY")
            || n.ends_with(".READ_PRIVILEGED_PHONE_STATE")
            || n.ends_with(".INSTALL_PACKAGES")
            || n.ends_with(".DELETE_PACKAGES")
            || n.ends_with(".STATUS_BAR")
            || n.ends_with(".WRITE_SECURE_SETTINGS")
            || n.ends_with(".DUMP")
            || n.ends_with(".READ_LOGS")
            || n.ends_with(".CAPTURE_AUDIO_OUTPUT") =>
        {
            PermissionLevel::Signature
        }
        // --- Normal ---
        n if n.starts_with("android.permission.")
            || n.starts_with("com.android.")
            || n.starts_with("com.google.android.") =>
        {
            PermissionLevel::Normal
        }
        _ => PermissionLevel::Unknown,
    }
}

pub fn describe_permission(name: &str) -> String {
    // A few high-value explanations; everything else gets a level-based hint.
    if let Some(short) = name.rsplit('.').next() {
        let specific: Option<&str> = match short {
            "CAMERA" => Some("Take pictures and video."),
            "RECORD_AUDIO" => Some("Record microphone audio."),
            "ACCESS_FINE_LOCATION" => Some("Precise device location."),
            "ACCESS_BACKGROUND_LOCATION" => Some("Location while in the background."),
            "READ_CONTACTS" => Some("Read the user's contacts."),
            "SEND_SMS" | "RECEIVE_SMS" | "READ_SMS" => Some("Send / read SMS messages."),
            "READ_EXTERNAL_STORAGE" => Some("Read shared storage (photos, downloads)."),
            "POST_NOTIFICATIONS" => Some("Show notifications (Android 13+)."),
            "MANAGE_EXTERNAL_STORAGE" => Some("All-files access (special app access)."),
            "SYSTEM_ALERT_WINDOW" => Some("Draw over other apps."),
            "REQUEST_INSTALL_PACKAGES" => Some("Install other apps (APK installs)."),
            "INTERNET" => Some("Open network sockets."),
            "ACCESS_NETWORK_STATE" => Some("Read network state."),
            "VIBRATE" => Some("Control vibration."),
            "WAKE_LOCK" => Some("Keep the processor from sleeping."),
            _ => None,
        };
        if let Some(s) = specific {
            return s.to_string();
        }
    }
    match classify_permission(name) {
        PermissionLevel::Dangerous => {
            "Runtime permission: user-granted, privacy-sensitive.".to_string()
        }
        PermissionLevel::Signature => {
            "Privileged: usually only system / signed apps hold this.".to_string()
        }
        PermissionLevel::Normal => "Install-time permission: low risk.".to_string(),
        PermissionLevel::Unknown => {
            "Custom or OEM permission: check the app's docs for what it guards.".to_string()
        }
    }
}

/// Build sorted UI rows (dangerous first, then signature, normal, unknown).
pub fn describe_all(names: &[String]) -> Vec<PermissionInfo> {
    let rank = |l: PermissionLevel| match l {
        PermissionLevel::Dangerous => 0,
        PermissionLevel::Signature => 1,
        PermissionLevel::Normal => 2,
        PermissionLevel::Unknown => 3,
    };
    let mut rows: Vec<PermissionInfo> = names
        .iter()
        .map(|n| PermissionInfo {
            name: n.clone(),
            level: classify_permission(n),
            description: describe_permission(n),
        })
        .collect();
    rows.sort_by(|a, b| rank(a.level).cmp(&rank(b.level)).then(a.name.cmp(&b.name)));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dangerous_permissions_flagged() {
        assert_eq!(
            classify_permission("android.permission.CAMERA"),
            PermissionLevel::Dangerous
        );
        assert_eq!(
            classify_permission("android.permission.READ_CONTACTS"),
            PermissionLevel::Dangerous
        );
        assert_eq!(
            classify_permission("android.permission.POST_NOTIFICATIONS"),
            PermissionLevel::Dangerous
        );
    }

    #[test]
    fn privileged_permissions_flagged() {
        assert_eq!(
            classify_permission("android.permission.MANAGE_EXTERNAL_STORAGE"),
            PermissionLevel::Signature
        );
        assert_eq!(
            classify_permission("android.permission.SYSTEM_ALERT_WINDOW"),
            PermissionLevel::Signature
        );
    }

    #[test]
    fn normal_and_unknown() {
        assert_eq!(
            classify_permission("android.permission.INTERNET"),
            PermissionLevel::Normal
        );
        assert_eq!(
            classify_permission("com.oemweird.CUSTOM"),
            PermissionLevel::Unknown
        );
    }

    #[test]
    fn dangerous_sorts_first() {
        let rows = describe_all(&[
            "android.permission.INTERNET".to_string(),
            "android.permission.CAMERA".to_string(),
            "com.oem.CUSTOM".to_string(),
        ]);
        assert_eq!(rows[0].name, "android.permission.CAMERA");
        assert_eq!(rows[0].level, PermissionLevel::Dangerous);
    }
}
