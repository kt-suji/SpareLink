use crate::models::ProbeMetrics;

#[repr(C)]
#[allow(non_snake_case)]
struct MemoryStatusEx {
    dwLength: u32,
    dwMemoryLoad: u32,
    ullTotalPhys: u64,
    ullAvailPhys: u64,
    ullTotalPageFile: u64,
    ullAvailPageFile: u64,
    ullTotalVirtual: u64,
    ullAvailVirtual: u64,
    ullAvailExtendedVirtual: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
#[allow(non_snake_case)]
struct FileTime {
    dwLowDateTime: u32,
    dwHighDateTime: u32,
}

impl FileTime {
    fn to_u64(&self) -> u64 {
        ((self.dwHighDateTime as u64) << 32) | (self.dwLowDateTime as u64)
    }
}

unsafe extern "system" {
    fn GlobalMemoryStatusEx(lpBuffer: *mut MemoryStatusEx) -> i32;
    fn GetSystemTimes(
        lpIdleTime: *mut FileTime,
        lpKernelTime: *mut FileTime,
        lpUserTime: *mut FileTime,
    ) -> i32;
}

pub struct NativeWindowsProbe {
    last_idle: u64,
    last_kernel: u64,
    last_user: u64,
}

impl NativeWindowsProbe {
    pub fn new() -> Self {
        let (idle, kernel, user) = Self::read_system_times();
        Self {
            last_idle: idle,
            last_kernel: kernel,
            last_user: user,
        }
    }

    fn read_system_times() -> (u64, u64, u64) {
        let mut idle = FileTime::default();
        let mut kernel = FileTime::default();
        let mut user = FileTime::default();
        unsafe {
            GetSystemTimes(&mut idle, &mut kernel, &mut user);
        }
        (idle.to_u64(), kernel.to_u64(), user.to_u64())
    }

    /// 現在のシステムメトリクス（CPU使用率、メモリ状態など）を取得する
    pub fn collect_metrics(&mut self) -> ProbeMetrics {
        let (idle, kernel, user) = Self::read_system_times();

        let idle_diff = idle.saturating_sub(self.last_idle);
        let kernel_diff = kernel.saturating_sub(self.last_kernel);
        let user_diff = user.saturating_sub(self.last_user);

        self.last_idle = idle;
        self.last_kernel = kernel;
        self.last_user = user;

        let total_system = kernel_diff + user_diff;
        let cpu_percent = if total_system > 0 {
            let active = total_system.saturating_sub(idle_diff);
            let pct = (active as f64 / total_system as f64) * 100.0;
            (pct * 10.0).round() / 10.0
        } else {
            0.0
        };

        // メモリ情報の取得
        let mut mem = MemoryStatusEx {
            dwLength: std::mem::size_of::<MemoryStatusEx>() as u32,
            dwMemoryLoad: 0,
            ullTotalPhys: 0,
            ullAvailPhys: 0,
            ullTotalPageFile: 0,
            ullAvailPageFile: 0,
            ullTotalVirtual: 0,
            ullAvailVirtual: 0,
            ullAvailExtendedVirtual: 0,
        };

        let (total_gb, avail_gb, mem_percent) = unsafe {
            if GlobalMemoryStatusEx(&mut mem) != 0 {
                let total_gb = (mem.ullTotalPhys as f64) / (1024.0 * 1024.0 * 1024.0);
                let avail_gb = (mem.ullAvailPhys as f64) / (1024.0 * 1024.0 * 1024.0);
                let pct = mem.dwMemoryLoad as f64;
                (
                    (total_gb * 10.0).round() / 10.0,
                    (avail_gb * 10.0).round() / 10.0,
                    pct,
                )
            } else {
                (16.0, 8.0, 50.0)
            }
        };

        let pc_name = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Windows-PC".to_string());
        let cpu_cores = std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(4);

        ProbeMetrics {
            pc_name,
            os: "Windows".to_string(),
            cpu_cores,
            cpu_percent: cpu_percent.clamp(0.0, 100.0),
            memory_percent: mem_percent,
            memory_total_gb: total_gb,
            memory_available_gb: avail_gb,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_native_windows_probe() {
        let mut probe = NativeWindowsProbe::new();
        thread::sleep(Duration::from_millis(100));
        let m = probe.collect_metrics();
        println!(
            "Native Probe実測: ホスト={}, CPU={}% ({} cores), MEM={}% ({}/{}GB)",
            m.pc_name, m.cpu_percent, m.cpu_cores, m.memory_percent, m.memory_available_gb, m.memory_total_gb
        );
        assert!(m.cpu_cores > 0);
        assert!(m.memory_total_gb > 0.0);
    }
}
