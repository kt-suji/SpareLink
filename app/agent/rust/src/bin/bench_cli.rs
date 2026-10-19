use std::env;

#[path = "../bench.rs"]
mod bench;

fn print_help() {
    println!("SpareLink CPU Benchmark CLI v0.1.0");
    println!("他システムやスクリプトから即座に呼び出し可能な超軽量CPU性能計測ツール\n");
    println!("使用法:");
    println!("  sparelink-bench [オプション]\n");
    println!("オプション:");
    println!("  -c, --cores <N>     測定に使用する論理コア数（デフォルト: 自動検出全コア）");
    println!("  -j, --json          結果をJSON形式で出力（外部ツール連携用）");
    println!("  -s, --score-only    スコア数値のみを出力（シェルスクリプト・変数代入用）");
    println!("  -q, --quiet         --score-only と同義");
    println!("  -h, --help          このヘルプメッセージを表示\n");
    println!("使用例:");
    println!("  sparelink-bench                     # 通常実行（全コア計測）");
    println!("  sparelink-bench --json              # JSON出力");
    println!("  sparelink-bench --score-only        # スコア数値のみ出力");
    println!("  sparelink-bench -c 8                # 8コアに限定して計測");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut json_mode = false;
    let mut score_only = false;
    let mut custom_cores: Option<u32> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_help();
                return;
            }
            "-j" | "--json" => {
                json_mode = true;
            }
            "-s" | "--score-only" | "-q" | "--quiet" => {
                score_only = true;
            }
            "-c" | "--cores" => {
                if i + 1 < args.len() {
                    if let Ok(c) = args[i + 1].parse::<u32>() {
                        if c > 0 {
                            custom_cores = Some(c);
                        }
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    let detected_cores = custom_cores.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(4)
    });

    let res = bench::run_benchmark_details(detected_cores);

    if score_only {
        println!("{}", res.score);
    } else if json_mode {
        let json = serde_json::json!({
            "score": res.score,
            "cores": res.cores,
            "elapsed_seconds": (res.elapsed_secs * 1000.0).round() / 1000.0,
            "mops": (res.mops * 10.0).round() / 10.0,
            "iterations_per_core": res.iterations_per_core
        });
        println!("{}", json);
    } else {
        println!("==================================================");
        println!("  SpareLink CPU Benchmark CLI v0.1.0");
        println!("==================================================");
        println!("  測定コア数 : {} 論理コア", res.cores);
        println!("  所要時間   : {:.3} 秒", res.elapsed_secs);
        println!("  演算能力   : {:.1} Mops/s", res.mops);
        println!("  性能スコア : {} pt", res.score);
        println!("==================================================");
    }
}
