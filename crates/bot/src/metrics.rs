use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SystemMetrics {
    pub cpu_temp_c: Option<f32>,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub ram_percent: u8,
    pub battery: Option<BatteryInfo>,
    pub uptime_secs: u64,
}

#[derive(Debug, Clone)]
pub struct BatteryInfo {
    pub capacity_percent: u8,
    pub status: String,
    pub ac_online: bool,
}

impl SystemMetrics {
    pub fn collect() -> Self {
        Self {
            cpu_temp_c: read_cpu_temp(),
            ram_used_mb: read_ram_info().map(|(u, _)| u).unwrap_or(0),
            ram_total_mb: read_ram_info().map(|(_, t)| t).unwrap_or(0),
            ram_percent: read_ram_info()
                .map(|(u, t)| {
                    if t > 0 {
                        ((u as f64 / t as f64) * 100.0) as u8
                    } else {
                        0
                    }
                })
                .unwrap_or(0),
            battery: read_battery_info(),
            uptime_secs: read_uptime_secs().unwrap_or(0),
        }
    }
}

pub fn read_cpu_temp() -> Option<f32> {
    // 1. Try /sys/class/thermal/thermal_zone*/temp
    let thermal_dir = Path::new("/sys/class/thermal");
    if let Ok(entries) = fs::read_dir(thermal_dir) {
        let mut max_temp = 0.0f32;
        let mut found = false;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with("thermal_zone") {
                let temp_file = path.join("temp");
                if let Ok(content) = fs::read_to_string(temp_file) {
                    if let Ok(val) = content.trim().parse::<f32>() {
                        let temp_c = val / 1000.0;
                        if temp_c > 0.0 && temp_c < 120.0 {
                            if temp_c > max_temp {
                                max_temp = temp_c;
                            }
                            found = true;
                        }
                    }
                }
            }
        }
        if found {
            return Some(max_temp);
        }
    }

    // 2. Try /sys/class/hwmon/hwmon*/temp*_input
    let hwmon_dir = Path::new("/sys/class/hwmon");
    if let Ok(entries) = fs::read_dir(hwmon_dir) {
        let mut max_temp = 0.0f32;
        let mut found = false;
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(files) = fs::read_dir(&path) {
                for file in files.flatten() {
                    let fname = file.file_name();
                    let s = fname.to_string_lossy();
                    if s.starts_with("temp") && s.ends_with("_input") {
                        if let Ok(content) = fs::read_to_string(file.path()) {
                            if let Ok(val) = content.trim().parse::<f32>() {
                                let temp_c = val / 1000.0;
                                if temp_c > 0.0 && temp_c < 120.0 {
                                    if temp_c > max_temp {
                                        max_temp = temp_c;
                                    }
                                    found = true;
                                }
                            }
                        }
                    }
                }
            }
        }
        if found {
            return Some(max_temp);
        }
    }

    None
}

pub fn read_ram_info() -> Option<(u64, u64)> {
    let content = fs::read_to_string("/proc/meminfo").ok()?;
    parse_meminfo(&content)
}

pub fn parse_meminfo(content: &str) -> Option<(u64, u64)> {
    let mut total_kb: Option<u64> = None;
    let mut avail_kb: Option<u64> = None;

    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            total_kb = extract_kb(line);
        } else if line.starts_with("MemAvailable:") {
            avail_kb = extract_kb(line);
        }
    }

    let total = total_kb?;
    let avail = avail_kb.unwrap_or(0);
    let used = total.saturating_sub(avail);

    Some((used / 1024, total / 1024))
}

fn extract_kb(line: &str) -> Option<u64> {
    line.split_whitespace()
        .nth(1)
        .and_then(|val| val.parse::<u64>().ok())
}

pub fn read_uptime_secs() -> Option<u64> {
    let content = fs::read_to_string("/proc/uptime").ok()?;
    let first = content.split_whitespace().next()?;
    let secs = first.parse::<f64>().ok()?;
    Some(secs as u64)
}

pub fn read_battery_info() -> Option<BatteryInfo> {
    let ps_dir = Path::new("/sys/class/power_supply");
    if !ps_dir.exists() {
        return None;
    }

    let entries = fs::read_dir(ps_dir).ok()?;
    let mut found_battery = false;
    let mut capacity: u8 = 0;
    let mut status = String::from("Unknown");
    let mut ac_online = false;

    for entry in entries.flatten() {
        let path = entry.path();
        let type_file = path.join("type");
        let ptype = fs::read_to_string(&type_file).unwrap_or_default();
        let ptype_trimmed = ptype.trim().to_lowercase();

        if ptype_trimmed == "battery" {
            found_battery = true;
            if let Ok(cap_str) = fs::read_to_string(path.join("capacity")) {
                if let Ok(cap) = cap_str.trim().parse::<u8>() {
                    capacity = cap;
                }
            }
            if let Ok(st_str) = fs::read_to_string(path.join("status")) {
                status = st_str.trim().to_string();
            }
        } else if ptype_trimmed == "mains" || ptype_trimmed == "ac" {
            if let Ok(online_str) = fs::read_to_string(path.join("online")) {
                if online_str.trim() == "1" {
                    ac_online = true;
                }
            }
        }
    }

    if found_battery {
        Some(BatteryInfo {
            capacity_percent: capacity,
            status,
            ac_online,
        })
    } else {
        None
    }
}

pub fn format_duration(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    let s = secs % 60;

    if days > 0 {
        format!("{} дн. {} ч.", days, hours)
    } else if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, mins, s)
    } else {
        format!("{:02}:{:02}", mins, s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_meminfo() {
        let fake_meminfo = r#"
MemTotal:        8192000 kB
MemFree:         1024000 kB
MemAvailable:    4096000 kB
Buffers:          500000 kB
Cached:          2000000 kB
"#;
        let (used, total) = parse_meminfo(fake_meminfo).unwrap();
        assert_eq!(total, 8000);
        assert_eq!(used, 4000);
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(45), "00:45");
        assert_eq!(format_duration(3665), "01:01:05");
        assert_eq!(format_duration(90000), "1 дн. 1 ч.");
    }
}
