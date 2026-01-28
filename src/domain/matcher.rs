use cedarwood::{self, Cedar};

pub struct DomainRule {
    prefix_rule: Cedar,
    prefix_dict: Vec<String>,
}

pub struct MatchResult {
    index: i32,
    matched_len: usize,
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
            prefix_dict,
        }
    }

    // Option<index, matched_len, xxx>
    pub fn search_domain(&self, domain: &str) -> Option<MatchResult> {
        match self.prefix_rule.exact_match_search(domain) {
            Some(r) => Some(MatchResult {
                index: r.0,
                matched_len: r.1,
            }),
            None => None,
        }
    }

    pub fn get_domain_by_index(&self, index: i32) -> Option<&String> {
        self.prefix_dict.get(index as usize)
    }
}

#[test]
fn test_domain_rule() {
    let value = vec!["example.com".to_string(), "test.org".to_string()];
    let rule = DomainRule::new(value);

    assert_eq!(rule.search_domain("example.com").is_some(), true);
    assert_eq!(rule.search_domain("exampl").is_some(), false);

    let match_result = rule.search_domain("example.com");
    assert_eq!(
        rule.get_domain_by_index(match_result.unwrap().index),
        Some(&"example.com".to_string())
    );
}
