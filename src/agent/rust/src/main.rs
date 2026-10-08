use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct SystemStatus {
    cpu: f64,
    memory: f64,
}

fn main() {
    let mut monitor = Command::new("python")
        .arg("../python/main.py")
        .stdout(Stdio::piped())
        .spawn()
        .expect("Python Monitorの起動に失敗しました");

    let stdout = monitor
        .stdout
        .take()
        .expect("Pythonの標準出力を取得できませんでした");

    let reader = BufReader::new(stdout);

    for line in reader.lines() {
        let line = line.expect("Pythonからのデータ読み取りに失敗しました");

        match serde_json::from_str::<SystemStatus>(&line) {
            Ok(status) => {
                println!(
                    "CPU: {:.1}% | Memory: {:.1}%",
                    status.cpu, status.memory
                );
            }
            Err(error) => {
                eprintln!("JSON解析エラー: {error}");
            }
        }
    }
}