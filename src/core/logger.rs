use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_MUTEX: Mutex<()> = Mutex::new(());
static LAST_PRUNED: Mutex<u64> = Mutex::new(0);

const RETENTION_SECS: u64 = 7 * 86_400; // 7 days

#[cfg(windows)]
fn get_local_now() -> (u64, String) {
    #[repr(C)]
    #[derive(Default, Copy, Clone)]
    struct SystemTimeWin32 {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }

    extern "system" {
        fn GetLocalTime(lpSystemTime: *mut SystemTimeWin32);
    }

    let mut st = SystemTimeWin32::default();
    unsafe { GetLocalTime(&mut st); }

    let epoch_s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let formatted = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        st.year, st.month, st.day, st.hour, st.minute, st.second, st.milliseconds
    );

    (epoch_s, formatted)
}

#[cfg(not(windows))]
fn get_local_now() -> (u64, String) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let epoch_s = now.as_secs();
    let ms = now.subsec_millis();
    (epoch_s, format!("{}.{:03}", epoch_s, ms))
}

pub fn get_log_file_path() -> PathBuf {
    crate::core::state::get_appdata_dir().join("dlss-studio.log")
}

/// Parses a date string like "2026-09-10" into approximate epoch seconds for pruning comparison
fn parse_date_to_epoch_approx(date_str: &str) -> Option<u64> {
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year: u64 = parts[0].parse().ok()?;
    let month: u64 = parts[1].parse().ok()?;
    let day: u64 = parts[2].parse().ok()?;

    if year < 1970 || month < 1 || month > 12 || day < 1 || day > 31 {
        return None;
    }

    // Days from 1970 to beginning of year
    let mut total_days = (year - 1970) * 365 + (year - 1969) / 4;
    let days_in_month = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month {
        total_days += days_in_month[(m - 1) as usize];
    }
    if month > 2 && (year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)) {
        total_days += 1; // leap year
    }
    total_days += day - 1;

    Some(total_days * 86_400)
}

/// Prunes entries older than 7 days from the single log file
pub fn prune_old_entries() {
    let log_path = get_log_file_path();
    if !log_path.exists() {
        return;
    }

    let now_s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if now_s < RETENTION_SECS {
        return;
    }
    let cutoff_s = now_s - RETENTION_SECS;

    let file = match fs::File::open(&log_path) {
        Ok(f) => f,
        Err(_) => return,
    };

    let reader = BufReader::new(file);
    let mut retained_lines = Vec::new();
    let mut pruned_any = false;

    for line_res in reader.lines() {
        if let Ok(line) = line_res {
            // Check if line starts with [YYYY-MM-DD
            if line.starts_with('[') && line.len() > 11 && line.as_bytes()[11] == b' ' {
                let date_part = &line[1..11];
                if let Some(line_epoch) = parse_date_to_epoch_approx(date_part) {
                    // Allow 1 day buffer to prevent timezone boundary issues
                    if line_epoch + 86_400 < cutoff_s {
                        pruned_any = true;
                        continue;
                    }
                }
            }
            retained_lines.push(line);
        }
    }

    if pruned_any {
        let temp_path = log_path.with_extension("log.tmp");
        if let Ok(mut temp_file) = fs::File::create(&temp_path) {
            for line in retained_lines {
                let _ = writeln!(temp_file, "{}", line);
            }
            let _ = temp_file.flush();
            drop(temp_file);
            let _ = fs::rename(&temp_path, &log_path);
        }
    }
}

pub fn log(level: &str, target: &str, message: &str) {
    let (epoch_s, timestamp) = get_local_now();
    let log_line = format!("[{}] [{}] [{}] {}\n", timestamp, level, target, message);

    let _lock = LOG_MUTEX.lock();

    let log_path = get_log_file_path();
    if let Some(parent) = log_path.parent() {
        if !parent.exists() {
            let _ = fs::create_dir_all(parent);
        }
    }

    // Prune periodically (at most once every 6 hours)
    if let Ok(mut last_pruned) = LAST_PRUNED.lock() {
        if epoch_s.saturating_sub(*last_pruned) > 21_600 {
            *last_pruned = epoch_s;
            prune_old_entries();
        }
    }

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
        let _ = file.write_all(log_line.as_bytes());
    }
}

pub fn info(target: &str, msg: &str) {
    log("INFO", target, msg);
}

#[allow(dead_code)]
pub fn warn(target: &str, msg: &str) {
    log("WARN", target, msg);
}

#[allow(dead_code)]
pub fn error(target: &str, msg: &str) {
    log("ERROR", target, msg);
}

#[allow(dead_code)]
pub fn debug(target: &str, msg: &str) {
    log("DEBUG", target, msg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_date_parsing() {
        let epoch = parse_date_to_epoch_approx("2026-09-10").unwrap();
        assert!(epoch > 1_700_000_000);
    }

    #[test]
    fn test_log_and_prune() {
        let temp_dir = std::env::temp_dir().join(format!("dlss_logger_test_{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()));
        fs::create_dir_all(&temp_dir).unwrap();
        let log_file = temp_dir.join("dlss-studio.log");

        // Write an old entry (from 2020) and a recent entry
        let old_entry = "[2020-01-01 12:00:00.000] [INFO] [test] Old message\n";
        let new_entry = "[2026-09-10 12:00:00.000] [INFO] [test] New message\n";
        fs::write(&log_file, format!("{}{}", old_entry, new_entry)).unwrap();

        // Run prune logic directly on this file
        let file = fs::File::open(&log_file).unwrap();
        let reader = BufReader::new(file);
        let now_s = parse_date_to_epoch_approx("2026-09-10").unwrap();
        let cutoff_s = now_s - RETENTION_SECS;
        let mut kept = Vec::new();
        for line in reader.lines() {
            let l = line.unwrap();
            let date_part = &l[1..11];
            if let Some(line_epoch) = parse_date_to_epoch_approx(date_part) {
                if line_epoch + 86_400 < cutoff_s {
                    continue;
                }
            }
            kept.push(l);
        }

        assert_eq!(kept.len(), 1);
        assert!(kept[0].contains("New message"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_live_logger_write() {
        info("test", "Testing single rolling log file write");
        let path = get_log_file_path();
        assert!(path.exists(), "Log file should exist at {}", path.display());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("Testing single rolling log file write"));
    }
}
