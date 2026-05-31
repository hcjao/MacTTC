use std::env;

pub(crate) struct TrayLabels {
    pub(crate) show: &'static str,
    pub(crate) schedule_title: &'static str,
    pub(crate) schedule_off: &'static str,
    pub(crate) interval_3: &'static str,
    pub(crate) autostart: &'static str,
    pub(crate) quit: &'static str,
}

pub(crate) fn tray_labels() -> TrayLabels {
    if system_language_is_chinese() {
        TrayLabels {
            show: "顯示 MacTTC",
            schedule_title: "下載排程",
            schedule_off: "關閉排程",
            interval_3: "每 3 小時",
            autostart: "開機啟動",
            quit: "退出",
        }
    } else {
        TrayLabels {
            show: "Show MacTTC",
            schedule_title: "Download Schedule",
            schedule_off: "Turn Schedule Off",
            interval_3: "Every 3 hours",
            autostart: "Launch at Login",
            quit: "Quit",
        }
    }
}

fn system_language_is_chinese() -> bool {
    if let Ok(output) = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
    {
        return apple_languages_primary_is_chinese(&String::from_utf8_lossy(&output.stdout));
    }

    ["LANG", "LC_ALL", "LC_MESSAGES"]
        .iter()
        .filter_map(|key| env::var(key).ok())
        .any(|value| value.to_lowercase().starts_with("zh"))
}

pub(crate) fn apple_languages_primary_is_chinese(languages: &str) -> bool {
    let languages = languages.to_lowercase();

    if let Some(start) = languages.find('"') {
        if let Some(end) = languages[start + 1..].find('"') {
            return languages[start + 1..start + 1 + end].starts_with("zh");
        }
    }

    languages
        .lines()
        .map(|line| {
            line.trim()
                .trim_end_matches(',')
                .trim_matches('"')
                .trim()
                .to_string()
        })
        .find(|line| !line.is_empty() && line != "(" && line != ")")
        .is_some_and(|language| language.starts_with("zh"))
}
