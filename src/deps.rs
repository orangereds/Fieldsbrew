use crate::cellar;
use crate::config::Config;
use crate::formula::{find_formula, Formula};
use crate::version::Requirement;
use anyhow::{bail, Context, Result};
use std::collections::{HashMap, HashSet};

pub struct Plan {
    /// 拓扑序的待安装列表（依赖在前）
    pub install_order: Vec<Formula>,
    /// 已安装且满足约束的包名
    pub satisfied: Vec<String>,
}

/// 从 root 出发递归解析依赖，产出拓扑序安装计划。
/// 约束策略：已安装但版本不满足约束时直接报错（升级逻辑在 M4 交付）。
pub fn resolve(cfg: &Config, root: &str) -> Result<Plan> {
    let mut st = State {
        cfg,
        loaded: HashMap::new(),
        seen_installed: HashSet::new(),
        in_stack: HashSet::new(),
        order: Vec::new(),
        satisfied: Vec::new(),
    };
    st.visit(root, None)?;
    let install_order = st
        .order
        .into_iter()
        .map(|n| st.loaded.remove(&n).expect("loaded 必含已遍历节点"))
        .collect();
    Ok(Plan {
        install_order,
        satisfied: st.satisfied,
    })
}

/// 仅解析某个 formula 的依赖（不包含它自身），用于升级时补齐新引入的依赖。
pub fn resolve_dependencies(cfg: &Config, f: &Formula) -> Result<Plan> {
    let mut st = State {
        cfg,
        loaded: HashMap::new(),
        seen_installed: HashSet::new(),
        in_stack: HashSet::new(),
        order: Vec::new(),
        satisfied: Vec::new(),
    };
    // 把根节点标记为已处理，避免依赖环回到根时被误判
    st.loaded.insert(f.package.name.clone(), f.clone());
    for d in f.deps()? {
        st.visit(&d.name, d.req.as_ref())?;
    }
    let install_order = st
        .order
        .into_iter()
        .map(|n| st.loaded.remove(&n).expect("loaded 必含已遍历节点"))
        .collect();
    Ok(Plan {
        install_order,
        satisfied: st.satisfied,
    })
}

struct State<'a> {
    cfg: &'a Config,
    loaded: HashMap<String, Formula>,
    seen_installed: HashSet<String>,
    in_stack: HashSet<String>,
    order: Vec<String>,
    satisfied: Vec<String>,
}

impl<'a> State<'a> {
    fn visit(&mut self, name: &str, req: Option<&Requirement>) -> Result<()> {
        if self.in_stack.contains(name) {
            bail!("检测到依赖环: {name}");
        }
        if self.loaded.contains_key(name) {
            return Ok(());
        }
        let path =
            find_formula(self.cfg, name).with_context(|| format!("解析依赖时找不到: {name}"))?;
        let f = Formula::load(&path)?;

        if let Some(m) = cellar::read_manifest(self.cfg, name)? {
            if let Some(r) = req {
                if !r.matches(&m.version) {
                    bail!(
                        "{name} 已安装版本 {}，但依赖要求 {}",
                        m.version,
                        r.describe()
                    );
                }
            }
            if self.seen_installed.insert(name.to_string()) {
                self.satisfied.push(name.to_string());
                self.loaded.insert(name.to_string(), f);
            }
            return Ok(());
        }

        self.in_stack.insert(name.to_string());
        for d in f.deps()? {
            self.visit(&d.name, d.req.as_ref())?;
        }
        self.in_stack.remove(name);
        self.loaded.insert(name.to_string(), f);
        self.order.push(name.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::version::parse_requirement;

    // 纯逻辑测试：约束匹配在 version.rs 已覆盖；
    // 拓扑序依赖文件系统布局，集成验证交给 smoke test。
    #[test]
    fn test_req_describe() {
        let r = parse_requirement(">=1.2").unwrap();
        assert_eq!(r.describe(), ">=1.2");
    }
}
