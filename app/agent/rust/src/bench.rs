use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

/// Lightweight multi-core CPU micro-benchmark.
/// Evaluates integer instruction-level parallelism (ILP), L1/L2 cache throughput, and FPU FMA latency.
fn compute_composite_workload(iterations: usize) -> u64 {
    const BUFFER_SIZE: usize = 4096; // 4096 * 8 bytes = 32 KB (L1/L2キャッシュに収まる作業枠)
    let mut buffer = [0u64; BUFFER_SIZE];

    // 4並列の独立したPRNGステート（命令レベル並列性 ILP を最大化）
    let mut s0: u64 = 0x123456789abcdef0;
    let mut s1: u64 = 0x0fedcba987654321;
    let mut s2: u64 = 0x5555555555555555;
    let mut s3: u64 = 0xaaaaaaaaaaaaaaaa;

    // 浮動小数点アキュムレータ（FPU積和演算）
    let mut f0: f64 = 1.0001;
    let mut f1: f64 = 0.9999;

    let outer_loops = (iterations / BUFFER_SIZE).max(1);

    for loop_idx in 0..outer_loops {
        let offset = (loop_idx * 17) & (BUFFER_SIZE - 1);

        for i in 0..BUFFER_SIZE {
            // 1. 高ILP整数ビットミキシング (4並列ALUポートを同時に稼働)
            s0 ^= s0 << 23;
            s1 ^= s1 >> 17;
            s2 = s2.wrapping_mul(6364136223846793005).wrapping_add(1);
            s3 = s3.rotate_left(19) ^ s0;

            let mixed = s0.wrapping_add(s1) ^ s2.wrapping_add(s3);

            // 2. キャッシュメモリアクセス (L1/L2ロード・ストア)
            let idx = (i + offset) & (BUFFER_SIZE - 1);
            buffer[idx] = buffer[idx].wrapping_add(mixed);

            // 3. 浮動小数点積和演算
            f0 = f0 * 1.0000001 + 0.0000002;
            f1 = f1 * 0.9999999 + 0.0000001;
        }
    }

    buffer[0] ^ buffer[BUFFER_SIZE - 1] ^ (f0 as u64) ^ (f1 as u64)
}

/** ベンチマーク詳細結果 */
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BenchmarkResult {
    pub cores: u32,
    pub elapsed_secs: f64,
    pub mops: f64,
    pub score: u32,
    pub iterations_per_core: usize,
}

/**
 * CPU性能ベンチマークを実行し、詳細な測定結果オブジェクトを返す。
 *
 * @param logical_cores 測定に使用する論理コア数（1以上の整数）
 * @return 詳細測定結果（コア数、所要時間、Mops/s、性能スコア等）
 */
pub fn run_benchmark_details(logical_cores: u32) -> BenchmarkResult {
    let cores = logical_cores.clamp(1, 128) as usize;
    // 1スレッドあたりの反復回数（約0.06〜0.15秒で安定して完了するよう設定）
    let iterations_per_core = 20_000_000;

    let total_found = Arc::new(AtomicU64::new(0));
    let start_time = Instant::now();

    let mut handles = Vec::with_capacity(cores);

    for _ in 0..cores {
        let total_counter = Arc::clone(&total_found);
        let handle = thread::spawn(move || {
            let count = compute_composite_workload(iterations_per_core);
            total_counter.fetch_add(count, Ordering::Relaxed);
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.join();
    }

    let elapsed = start_time.elapsed();
    let elapsed_secs = elapsed.as_secs_f64().max(0.001); // 0除算防止

    // 総処理演算量 (各反復でALU 4回 + メモリ 1回 + FPU 2回 = 約7 ops)
    let total_ops = (iterations_per_core * cores * 7) as f64;
    let ops_per_sec = total_ops / elapsed_secs;
    let mops = ops_per_sec / 1_000_000.0;

    // スケーリング: 毎秒100万 ops = 1 pt（4〜5桁: 5,000〜50,000 pt 帯に正規化）
    let raw_score = ops_per_sec / 1_000_000.0;
    let score = (raw_score.round() as u32).max(10);

    BenchmarkResult {
        cores: cores as u32,
        elapsed_secs,
        mops,
        score,
        iterations_per_core,
    }
}

/**
 * CPU性能ベンチマークを実行し、基準性能スコアを算出する。
 *
 * @param logical_cores 測定に使用する論理コア数（1以上の整数）
 * @return 性能スコア（4〜5桁スケール: 5,000〜50,000 pt）
 */
#[allow(dead_code)]
pub fn run_quick_cpu_benchmark(logical_cores: u32) -> u32 {
    let res = run_benchmark_details(logical_cores);
    println!(
        "[Benchmark] コア数: {} | 所要時間: {:.3}秒 | 演算能力: {:.1} Mops/s | 性能スコア: {} pt",
        res.cores, res.elapsed_secs, res.mops, res.score
    );
    res.score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_runs_and_returns_positive_score() {
        let score = run_quick_cpu_benchmark(2);
        assert!(score > 0, "ベンチマークスコアは正の整数であるべき");
    }

    #[test]
    fn test_benchmark_multicore() {
        let score = run_quick_cpu_benchmark(16);
        assert!(score >= 1000, "16コアでは1000pt以上のスコアが出るべき");
    }
}
