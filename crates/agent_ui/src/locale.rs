use std::sync::LazyLock;

static LOCALE: LazyLock<String> = LazyLock::new(|| {
    if let Some(locale) = read_zed_settings_locale() {
        return locale;
    }

    let generic = ["C", "POSIX", "C.UTF-8", "C.utf8"];

    for var in &["LANG", "LANGUAGE", "LC_ALL"] {
        if let Ok(val) = std::env::var(var) {
            let v = val.trim().to_lowercase();
            if !v.is_empty() && !generic.contains(&v.as_str()) {
                return v;
            }
        }
    }

    #[cfg(target_os = "macos")]
    if let Ok(out) = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLocale"])
        .output()
    {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_lowercase();
        if !s.is_empty() {
            return s;
        }
    }

    String::new()
});

fn read_zed_settings_locale() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let path = std::path::Path::new(&home).join(".config/zed/settings.json");
    let content = std::fs::read_to_string(path).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        if let Some(pos) = trimmed.find("\"locale\"") {
            let rest = &trimmed[pos + 8..];
            if let Some(colon) = rest.find(':') {
                let after = rest[colon + 1..].trim();
                if let Some(start) = after.find('"') {
                    let after = &after[start + 1..];
                    if let Some(end) = after.find('"') {
                        let locale = &after[..end];
                        if !locale.is_empty() {
                            return Some(locale.to_lowercase());
                        }
                    }
                }
            }
        }
    }
    None
}

fn is_zh() -> bool {
    LOCALE.starts_with("zh")
}

static ZH: &[(&str, &str)] = &[
    ("Context", "上下文"),
    ("Files & Directories", "文件和目录"),
    ("Symbols", "符号"),
    ("Threads", "线程"),
    ("Skills", "技能"),
    ("Image", "图片"),
    ("Selection", "选区"),
    ("Branch Diff", "分支差异"),
    ("Fetch", "网页抓取"),
    ("Diagnostics", "诊断信息"),
];

pub fn t(key: &'static str) -> &'static str {
    if is_zh() {
        ZH.iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| *v)
            .unwrap_or(key)
    } else {
        key
    }
}
