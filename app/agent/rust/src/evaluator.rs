use crate::config::EvaluatorSettings;
use crate::models::{ProbeMetrics, SurplusCapacity, SurplusEvaluationStatus};

/// コア数に対するメモリ総量のバランス係数（RAM Capacity Factor）を算出する。
/// - 基準: 1論理コアあたり 2.0 GB（例: 16コアで32GB）= 係数 1.00
/// - メモリ不足（1コアあたり < 2.0 GB）: 0.50 〜 1.00
///   （例: 20コアで16GB = 0.8GB/コアなら約0.76倍。全コアを回すジョブでスワップを起こしやすいため割引）
/// - メモリ潤沢（1コアあたり > 2.0 GB）: 1.00 〜 1.25
///   （例: 16コアで64GB = 4.0GB/コアなら1.10倍、96GBなら1.15倍。大容量並列データ処理能力を加味）
pub fn calculate_ram_capacity_factor(cpu_cores: u32, memory_total_gb: f64) -> f64 {
    let cores = (cpu_cores as f64).max(1.0);
    let ram_per_core = (memory_total_gb / cores).max(0.1);

    if ram_per_core < 2.0 {
        // 0.60 + 0.40 * (ram_per_core / 2.0)
        (0.60 + 0.40 * (ram_per_core / 2.0)).clamp(0.50, 1.00)
    } else {
        // 2.0GB/コア以上は緩やかにボーナス（対数スケーリング、最大1.25倍）
        let ratio = ram_per_core / 2.0;
        let bonus = (ratio.ln() * 0.12).clamp(0.0, 0.25);
        1.0 + bonus
    }
}

/// Evaluates allocatable surplus computing resources based on hardware baseline and live telemetry.
#[allow(dead_code)]
pub fn evaluate_surplus_capacity(
    metrics: &ProbeMetrics,
    benchmark_score: Option<u32>,
) -> SurplusCapacity {
    evaluate_surplus_capacity_with_config(metrics, benchmark_score, &EvaluatorSettings::default())
}

/// Evaluates allocatable surplus capacity using custom evaluator configuration.
pub fn evaluate_surplus_capacity_with_config(
    metrics: &ProbeMetrics,
    benchmark_score: Option<u32>,
    config: &EvaluatorSettings,
) -> SurplusCapacity {
    // 1. Available physical RAM minus reserved margin for host OS
    let allocatable_memory = (metrics.memory_available_gb - config.min_memory_margin_gb).max(0.0);
    let allocatable_memory_gb = (allocatable_memory * 10.0).round() / 10.0;

    // 2. Fallback when benchmark is not yet finished
    let raw_base_score = match benchmark_score {
        Some(score) => score,
        None => {
            return SurplusCapacity {
                status: SurplusEvaluationStatus::Evaluating,
                confidence: Some(0.60),
                allocatable_cpu_score: None,
                allocatable_memory_gb: Some(allocatable_memory_gb),
                algorithm_version: Some("sparelink_v2_composite".to_string()),
                status_note: format!(
                    "Benchmark pending. Reserved {:.1} GB OS safety margin.",
                    config.min_memory_margin_gb
                ),
            };
        }
    };

    // メモリ総量とコア数のバランス係数を適用（メモリ総量を総合スコアに反映）
    let ram_factor = calculate_ram_capacity_factor(metrics.cpu_cores, metrics.memory_total_gb);
    let composite_base_score = (raw_base_score as f64 * ram_factor).round() as u32;

    // 3. 空き物理メモリがOS安全マージン以下（実効空き 0 GB）の場合の安全保護
    if allocatable_memory_gb <= 0.0 {
        return SurplusCapacity {
            status: SurplusEvaluationStatus::Evaluated,
            confidence: Some(0.95),
            allocatable_cpu_score: Some(0),
            allocatable_memory_gb: Some(0.0),
            algorithm_version: Some("sparelink_v2_composite".to_string()),
            status_note: format!(
                "Available RAM ({:.1}GB) is below safety margin ({:.1}GB). Job allocation suspended.",
                metrics.memory_available_gb, config.min_memory_margin_gb
            ),
        };
    }

    // 4. Allocatable CPU calculation with high load threshold protection
    let cpu_usage = metrics.cpu_percent;

    if cpu_usage >= config.high_load_threshold {
        return SurplusCapacity {
            status: SurplusEvaluationStatus::Evaluated,
            confidence: Some(0.95),
            allocatable_cpu_score: Some(0),
            allocatable_memory_gb: Some(allocatable_memory_gb),
            algorithm_version: Some("sparelink_v2_composite".to_string()),
            status_note: format!(
                "CPU load ({:.1}%) exceeds safety threshold ({:.0}%). Allocation suspended.",
                cpu_usage, config.high_load_threshold
            ),
        };
    }

    // Effective headroom ratio = (100% - usage - safety_headroom) / 100
    let raw_headroom = (100.0 - cpu_usage - config.safety_headroom) / 100.0;
    let effective_headroom = raw_headroom.max(0.0);

    // 空き物理メモリの余裕度による微調整（空きメモリが 0〜2GB と残り少ないときは割当を抑制）
    let mem_constraint = (allocatable_memory_gb / 2.0).clamp(0.2, 1.0);

    let allocatable_cpu = (composite_base_score as f64 * effective_headroom * mem_constraint).round() as u32;

    let confidence = if cpu_usage > 60.0 { 0.85 } else { 0.95 };

    let ram_per_core = metrics.memory_total_gb / (metrics.cpu_cores as f64).max(1.0);

    SurplusCapacity {
        status: SurplusEvaluationStatus::Evaluated,
        confidence: Some(confidence),
        allocatable_cpu_score: Some(allocatable_cpu),
        allocatable_memory_gb: Some(allocatable_memory_gb),
        algorithm_version: Some("sparelink_v2_composite".to_string()),
        status_note: format!(
            "Composite evaluation. RAM factor: {:.2}x ({:.1}GB/core). Safety: CPU {:.0}%, RAM {:.0}GB.",
            ram_factor, ram_per_core, config.safety_headroom, config.min_memory_margin_gb
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_evaluation() {
        let metrics = ProbeMetrics {
            pc_name: "TEST-PC".to_string(),
            os: "Windows 11".to_string(),
            cpu_cores: 16,
            cpu_percent: 20.0,
            memory_percent: 40.0,
            memory_total_gb: 32.0,
            memory_available_gb: 16.0,
        };

        let result = evaluate_surplus_capacity(&metrics, Some(25000));
        assert_eq!(result.status, SurplusEvaluationStatus::Evaluated);
        // 100% - 20% - 20% = 60% -> 25000 * 0.60 = 15000 pt
        assert_eq!(result.allocatable_cpu_score, Some(15000));
        // 16.0 - 4.0 = 12.0 GB
        assert_eq!(result.allocatable_memory_gb, Some(12.0));
    }

    #[test]
    fn test_high_load_protection() {
        let metrics = ProbeMetrics {
            pc_name: "TEST-PC".to_string(),
            os: "Windows 11".to_string(),
            cpu_cores: 16,
            cpu_percent: 85.0, // 80%超過
            memory_percent: 80.0,
            memory_total_gb: 32.0,
            memory_available_gb: 10.0,
        };

        let result = evaluate_surplus_capacity(&metrics, Some(1000));
        assert_eq!(result.allocatable_cpu_score, Some(0)); // 安全保護で0pt
    }

    #[test]
    fn test_ram_capacity_factor_scaling() {
        // 16 cores, 16GB (1.0 GB/core -> 0.80x)
        let factor_low = calculate_ram_capacity_factor(16, 16.0);
        assert!((factor_low - 0.80).abs() < 0.01);

        // 16 cores, 32GB (2.0 GB/core -> 1.00x)
        let factor_ideal = calculate_ram_capacity_factor(16, 32.0);
        assert!((factor_ideal - 1.00).abs() < 0.01);

        // 16 cores, 64GB (4.0 GB/core -> ~1.08x)
        let factor_high = calculate_ram_capacity_factor(16, 64.0);
        assert!(factor_high > 1.05 && factor_high < 1.15);
    }

    #[test]
    fn test_low_memory_suspension() {
        let metrics = ProbeMetrics {
            pc_name: "TEST-PC".to_string(),
            os: "Windows 11".to_string(),
            cpu_cores: 16,
            cpu_percent: 10.0,
            memory_percent: 95.0,
            memory_total_gb: 32.0,
            memory_available_gb: 3.5, // 4GB安全マージン以下
        };

        let result = evaluate_surplus_capacity(&metrics, Some(20000));
        assert_eq!(result.allocatable_cpu_score, Some(0)); // RAM不足で安全停止
        assert_eq!(result.allocatable_memory_gb, Some(0.0));
    }
}
