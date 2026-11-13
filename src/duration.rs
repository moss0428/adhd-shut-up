use std::fmt::Write;

pub fn format_minutes(minutes: u64) -> String {
    format_duration(minutes, false)
}

pub fn format_countdown(seconds: u32) -> String {
    let mut duration = format_duration(u64::from(seconds / 60), true);
    write!(duration, "{:02}秒", seconds % 60).unwrap();
    duration
}

fn format_duration(total_minutes: u64, countdown: bool) -> String {
    let days = total_minutes / 1_440;
    let hours = total_minutes / 60 % 24;
    let minutes = total_minutes % 60;
    let mut duration = String::new();
    if days > 0 {
        write!(duration, "{days}天").unwrap();
    }
    if hours > 0 || (countdown && days > 0) {
        write!(duration, "{hours}小时").unwrap();
    }
    if countdown {
        write!(duration, "{minutes:02}分").unwrap();
    } else if minutes > 0 || duration.is_empty() {
        write!(duration, "{minutes}分").unwrap();
    }
    duration
}
