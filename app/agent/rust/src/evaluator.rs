use crate::config::EvaluatorSettings;
use crate::models::{ProbeMetrics, SurplusCapacity, SurplusEvaluationStatus};

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
    let base_score = match benchmark_score {
        Some(score) => score,
        None => {
            return SurplusCapacity {
                status: SurplusEvaluationStatus::Evaluating,
                confidence: Some(0.60),
                allocatable_cpu_score: None,
                allocatable_memory_gb: Some(allocatable_memory_gb),
                algorithm_version: Some("sparelink_v1".to_string()),
                status_note: format!(
                    "Benchmark pending. Reserved {:.1} GB OS safety margin.",
                    config.min_memory_margin_gb
                ),
            };
        }
    };

    // 3. Allocatable CPU calculation with high load threshold protection
    let cpu_usage = metrics.cpu_percent;

    if cpu_usage >= config.high_load_threshold {
        return SurplusCapacity {
            status: SurplusEvaluationStatus::Evaluated,
            confidence: Some(0.95),
            allocatable_cpu_score: Some(0),
            allocatable_memory_gb: Some(allocatable_memory_gb),
            algorithm_version: Some("sparelink_v1".to_string()),
            status_note: format!(
                "CPU load ({:.1}%) exceeds safety threshold ({:.0}%). Allocation suspended.",
                cpu_usage, config.high_load_threshold
            ),
        };
    }

    // Effective headroom ratio = (100% - usage - safety_headroom) / 100
    let raw_headroom = (100.0 - cpu_usage - config.safety_headroom) / 100.0;
    let effective_headroom = raw_headroom.max(0.0);

    let allocatable_cpu = (base_score as f64 * effective_headroom).round() as u32;

    let confidence = if cpu_usage > 60.0 { 0.85 } else { 0.95 };

    SurplusCapacity {
        status: SurplusEvaluationStatus::Evaluated,
        confidence: Some(confidence),
        allocatable_cpu_score: Some(allocatable_cpu),
        allocatable_memory_gb: Some(allocatable_memory_gb),
        algorithm_version: Some("sparelink_v1_headroom".to_string()),
        status_note: format!(
            "Normal operation. Safety headroom: CPU {:.0}%, RAM {:.0}GB.",
            config.safety_headroom, config.min_memory_margin_gb
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
}
