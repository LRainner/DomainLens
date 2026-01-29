use cedarwood::{self, Cedar};

pub struct DomainRule {
    prefix_rule: Cedar,
    /// 仅在 debug 构建中保留完整的前缀字典，用于调试和测试；
    /// release 构建中会被完全移除以节省内存。
    #[cfg(debug_assertions)]
    prefix_dict: Vec<String>,
}

pub struct MatchResult {
    pub index: i32,
    pub matched_len: usize,
}

impl DomainRule {
    pub fn new(prefix_dict: Vec<String>) -> Self {
        let mut cedar = Cedar::new();
        let key_values: Vec<(&str, i32)> = prefix_dict
            .iter()
            .enumerate()
            .map(|(k, s)| (s.as_ref(), k as i32))
            .collect();
        cedar.build(&key_values);
        Self {
            prefix_rule: cedar,
            #[cfg(debug_assertions)]
            prefix_dict,
        }
    }

    pub fn search_domain(&self, domain: &str) -> Option<MatchResult> {
        match self.prefix_rule.exact_match_search(domain) {
            Some(r) => Some(MatchResult {
                index: r.0,
                matched_len: r.1,
            }),
            None => None,
        }
    }

    /// 仅在 debug 构建下可用，用 index 反查域名。
    /// release 构建中该方法不会被编译进最终二进制。
    #[cfg(debug_assertions)]
    pub fn get_domain_by_index(&self, index: i32) -> Option<&String> {
        self.prefix_dict.get(index as usize)
    }
}

#[test]
fn test_domain_rule() {
    let value = vec!["example.com".to_string(), "test.org".to_string()];
    let rule = DomainRule::new(value);

    let match_result = rule.search_domain("example.com");
    assert!(match_result.is_some());
    assert!(rule.search_domain("exampl").is_none());

    #[cfg(debug_assertions)]
    {
        let match_result = match_result.unwrap();
        assert_eq!(
            rule.get_domain_by_index(match_result.index),
            Some(&"example.com".to_string())
        );
    }
}
