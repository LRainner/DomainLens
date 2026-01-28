use std::fs::File;
use std::io::{BufRead, BufReader};
use std::time::Instant;

use domainlens::domain::DomainRule;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// 从 Cloudflare 域名列表文件中读取前 `limit` 个域名。
/// 文件格式：
///   第 1 行: "domain"
///   第 2 行起: 每行一个域名
fn load_domains_from_file(limit: usize) -> Vec<String> {
    let path = "benches/cloudflare-radar_top-1000000-domains_20260121-20260128.csv";
    let file =
        File::open(path).expect("failed to open benches/cloudflare-radar_..._domains_....csv");
    let reader = BufReader::new(file);

    let mut domains = Vec::with_capacity(limit);

    for (i, line) in reader.lines().enumerate() {
        let line = line.expect("failed to read line");

        // 第一行是表头 "domain"，跳过
        if i == 0 {
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        domains.push(trimmed.to_string());

        if domains.len() >= limit {
            break;
        }
    }

    domains
}

/// 获取当前进程的内存占用（MiB）。
fn current_memory_mib(sys: &mut System) -> f64 {
    // sysinfo 0.38 的 Pid::from 接受 usize
    let pid = Pid::from(std::process::id() as usize);

    // 只刷新当前进程，并且只刷新内存字段
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        /* include_children */ false,
        ProcessRefreshKind::nothing().with_memory(),
    );

    if let Some(proc_) = sys.process(pid) {
        // memory() 返回的是字节数（u64）
        let bytes: u64 = proc_.memory();
        // 转成 MiB：bytes / (1024 * 1024)
        (bytes as f64) / (1024.0 * 1024.0)
    } else {
        0.0
    }
}

fn main() {
    // 可以按需调整：比如 100_000 / 500_000 / 1_000_000
    let n = 1_000_000usize;

    let mut sys = System::new();

    println!("Loading {} domains from file...", n);
    let domains = load_domains_from_file(n);
    println!("Loaded {} domains.", domains.len());

    // 第一次测：加载完原始数据后的基线
    let mem_after_load = current_memory_mib(&mut sys);
    println!("Memory after loading domains:        {:.2} MiB", mem_after_load);

    // 构建 DomainRule，并在构建完后显式 drop 掉原始 Vec<String>
    let build_start = Instant::now();
    let rule = DomainRule::new(domains);
    let build_elapsed = build_start.elapsed();

    // 强制做一次 GC 样的刷新，让 OS 和 allocator 有机会回收
    // （不一定 100% 立刻反映完全真实的“极限最小值”，但能显著减小噪音）
    std::thread::sleep(std::time::Duration::from_millis(200));
    let mem_after_drop = current_memory_mib(&mut sys);

    println!("Memory after building & dropping:   {:.2} MiB", mem_after_drop);
    println!(
        "Delta (peak approx. for {} rules): {:.2} MiB",
        n,
        mem_after_drop - mem_after_load
    );
    println!("Build {} domains cost:               {:.2?}", n, build_elapsed);
}