use std::fs::OpenOptions;
use std::io::Write;
use chrono::Utc;

pub fn append_log(line: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open("share.log") {
        let ts = Utc::now().to_rfc3339();
        let _ = writeln!(f, "{} {}", ts, line);
    }
}
