use std::fs::File;
use std::io::{BufRead, BufReader};
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use domainlens::domain::DomainRule;

/// 从 Cloudflare 域名列表文件中读取前 `limit` 个域名。
/// 文件格式：
///   第 1 行: "domain"
///   第 2 行起: 每行一个域名
fn load_domains_from_file(limit: usize) -> Vec<String> {
    // bench 运行时工作目录是项目根目录，所以这里用相对路径
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

/// 基于 Cloudflare 域名列表构建 DomainRule 的基准测试。
fn bench_build_from_file(c: &mut Criterion) {
    // 这里你可以调整规模：100_000 / 500_000 / 1_000_000。
    // Cloudflare 这个文件是 top 1M，所以最大就是 1_000_000。
    let n = 1_000_000usize;

    let rules = load_domains_from_file(n);

    let mut group = c.benchmark_group("domain_rule_build_cloudflare");
    group
        .measurement_time(Duration::from_secs(5))
        .warm_up_time(Duration::from_secs(2))
        .sample_size(10);

    group.bench_function(format!("build_cloudflare_{}", n), |b| {
        b.iter(|| {
            let cloned = rules.clone();
            DomainRule::new(cloned)
        })
    });

    group.finish();
}

/// 基于 Cloudflare 域名列表的搜索基准测试（命中 + 未命中）。
fn bench_search_with_file(c: &mut Criterion) {
    let n = 1_000_000usize;
    let rules = load_domains_from_file(n);
    let rule = DomainRule::new(rules.clone());

    // 命中域名：取列表中的某个域名，比如第 100 个
    let hit_domain = rules
        .get(100)
        .cloned()
        .unwrap_or_else(|| "example.com".to_string());

    // 未命中域名：构造一个不太可能在榜单里的域名
    let miss_domain = "this-domain-definitely-not-in-cloudflare-top-list-xyz.com".to_string();

    let mut group = c.benchmark_group("domain_rule_search_cloudflare");
    group
        .measurement_time(Duration::from_secs(5))
        .warm_up_time(Duration::from_secs(2))
        .sample_size(50);

    // 命中场景
    group.bench_function("search_cloudflare_hit", |b| {
        b.iter(|| {
            let _ = rule.search_domain(&hit_domain);
        })
    });

    // 未命中场景
    group.bench_function("search_cloudflare_miss", |b| {
        b.iter(|| {
            let _ = rule.search_domain(&miss_domain);
        })
    });

    group.finish();
}

criterion_group!(benches, bench_build_from_file, bench_search_with_file,);
criterion_main!(benches);
