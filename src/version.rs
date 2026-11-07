use std::cmp::Ordering;

/// 把点分版本号解析为可比较的数字段："1.10.2" -> [1, 10, 2]。
/// 非数字字符仅作分隔（"1.2b" -> [1, 2]），足以覆盖常见源码包版本。
pub fn parse_version(v: &str) -> Vec<u64> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in v.chars().chain(std::iter::once('.')) {
        if ch.is_ascii_digit() {
            cur.push(ch);
        } else if !cur.is_empty() {
            out.push(cur.parse().unwrap_or(0));
            cur.clear();
        }
    }
    if out.is_empty() {
        out.push(0);
    }
    out
}

/// 比较两个版本号，按数字段逐位比较、缺位补零（1.2 < 1.10）。
pub fn compare(a: &str, b: &str) -> Ordering {
    let va = parse_version(a);
    let vb = parse_version(b);
    let n = va.len().max(vb.len());
    for i in 0..n {
        let x = va.get(i).copied().unwrap_or(0);
        let y = vb.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            Ordering::Equal => continue,
            o => return o,
        }
    }
    Ordering::Equal
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Ge,
    Gt,
    Le,
    Lt,
    Eq,
}

#[derive(Debug, Clone)]
pub struct Requirement {
    pub op: Op,
    pub version: String,
}

impl Requirement {
    pub fn matches(&self, v: &str) -> bool {
        match compare(v, &self.version) {
            Ordering::Greater => matches!(self.op, Op::Ge | Op::Gt),
            Ordering::Equal => matches!(self.op, Op::Ge | Op::Le | Op::Eq),
            Ordering::Less => matches!(self.op, Op::Le | Op::Lt),
        }
    }

    pub fn describe(&self) -> String {
        let s = match self.op {
            Op::Ge => ">=",
            Op::Gt => ">",
            Op::Le => "<=",
            Op::Lt => "<",
            Op::Eq => "==",
        };
        format!("{s}{}", self.version)
    }
}

/// 解析形如 ">=1.2"、"==2.0" 的版本约束
pub fn parse_requirement(s: &str) -> anyhow::Result<Requirement> {
    let s = s.trim();
    let (op, rest) = if let Some(r) = s.strip_prefix(">=") {
        (Op::Ge, r)
    } else if let Some(r) = s.strip_prefix("<=") {
        (Op::Le, r)
    } else if let Some(r) = s.strip_prefix("==") {
        (Op::Eq, r)
    } else if let Some(r) = s.strip_prefix('>') {
        (Op::Gt, r)
    } else if let Some(r) = s.strip_prefix('<') {
        (Op::Lt, r)
    } else if let Some(r) = s.strip_prefix('=') {
        (Op::Eq, r)
    } else {
        anyhow::bail!("无法解析版本约束: {s}（支持 >= > <= < ==）");
    };
    Ok(Requirement {
        op,
        version: rest.trim().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("1.10.2"), vec![1, 10, 2]);
        assert_eq!(parse_version("1.2"), vec![1, 2]);
        assert_eq!(parse_version("v1.2"), vec![1, 2]);
        assert_eq!(parse_version(""), vec![0]);
    }

    #[test]
    fn test_compare() {
        assert_eq!(compare("1.2", "1.10"), Ordering::Less);
        assert_eq!(compare("1.10", "1.9.9"), Ordering::Greater);
        assert_eq!(compare("2.12.1", "2.12.1"), Ordering::Equal);
        assert_eq!(compare("1.3.1", "1.3"), Ordering::Greater);
        assert_eq!(compare("1.2", "1.2.0"), Ordering::Equal);
    }

    #[test]
    fn test_requirement() {
        let r = parse_requirement(">=1.2").unwrap();
        assert!(r.matches("1.2"));
        assert!(r.matches("1.10"));
        assert!(!r.matches("1.1"));

        let r = parse_requirement("<2.0").unwrap();
        assert!(r.matches("1.9"));
        assert!(!r.matches("2.0"));

        let r = parse_requirement("==1.3.1").unwrap();
        assert!(r.matches("1.3.1"));
        assert!(!r.matches("1.3.2"));

        assert!(parse_requirement("1.2").is_err());
    }
}
