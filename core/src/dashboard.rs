use parking_lot::Mutex;
use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use crate::log::LogDto;

const MIB: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardDto {
    pub maker: usize,
    pub log: usize,
    pub sender: usize,
    pub plugin: usize,
    /// Configured events per time unit of event-rate logs.
    pub eps: i64,
    /// Events generated in the last second.
    pub actual_eps: u64,
    /// Configured bytes per time unit of byte-rate logs.
    pub bps: i64,
    /// Bytes generated in the last second.
    pub actual_bps: u64,
    /// Process CPU usage, percent of all cores.
    pub cpu: f64,
    /// Resident memory in MiB.
    pub memory: u64,
    /// Memory limit (cgroup limit or physical memory) in MiB.
    pub max_memory: u64,
    /// OS threads of the process (omitted where the platform does not expose it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread: Option<usize>,
    pub scenario: usize,
    pub version: String,
    pub build_time: Option<String>,
}

pub struct Counts {
    pub maker: usize,
    pub sender: usize,
    pub plugin: usize,
    pub scenario: usize,
}

pub struct Dashboard {
    system: Mutex<System>,
    pid: Option<Pid>,
}

impl Default for Dashboard {
    fn default() -> Self {
        let dashboard = Self {
            system: Mutex::new(System::new()),
            pid: sysinfo::get_current_pid().ok(),
        };
        // CPU usage is measured between refreshes; take the first sample now.
        dashboard.process_usage();
        dashboard
    }
}

fn thread_count() -> Option<usize> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("Threads:"))
        .and_then(|n| n.trim().parse().ok())
}

impl Dashboard {
    pub fn build(&self, logs: &[LogDto], counts: Counts) -> DashboardDto {
        let (eps, actual_eps, bps, actual_bps) = logs.iter().fold((0i64, 0u64, 0i64, 0u64), |(e, ae, b, ab), log| {
            let bytes = log.eps_unit == "bytes";
            (
                if bytes { e } else { e.saturating_add(log.eps) },
                ae + log.current_eps,
                if bytes { b.saturating_add(log.eps) } else { b },
                ab + log.bytes_per_sec,
            )
        });
        let (cpu, memory, max_memory) = self.process_usage();
        DashboardDto {
            maker: counts.maker,
            log: logs.len(),
            sender: counts.sender,
            plugin: counts.plugin,
            eps,
            actual_eps,
            bps,
            actual_bps,
            cpu,
            memory,
            max_memory,
            thread: thread_count(),
            scenario: counts.scenario,
            version: env!("CARGO_PKG_VERSION").to_owned(),
            build_time: option_env!("LOGMAKER_BUILD_TIME").map(str::to_owned),
        }
    }

    /// CPU usage since the previous call, resident memory and memory limit.
    fn process_usage(&self) -> (f64, u64, u64) {
        let mut system = self.system.lock();
        system.refresh_memory();
        let max_memory = system
            .cgroup_limits()
            .map_or(system.total_memory(), |limits| limits.total_memory)
            / MIB;
        let Some(pid) = self.pid else {
            return (0.0, 0, max_memory);
        };
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        let Some(process) = system.process(pid) else {
            return (0.0, 0, max_memory);
        };
        let cores = std::thread::available_parallelism().map_or(1, usize::from) as f64;
        let cpu = (f64::from(process.cpu_usage()) / cores * 10.0).trunc() / 10.0;
        (cpu, process.memory() / MIB, max_memory)
    }
}
