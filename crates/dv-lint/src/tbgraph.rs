//! The UVM structure the Python rules and the testbench map read: the user
//! classes' TLM ports, the `connect()` calls, and the `uvm_config_db`
//! set/get calls, as uvmcheck's TlmTopologyGraph and ConfigDbGraph collect
//! them. Locations are resolved to file names here (a location in a macro
//! expansion is where the macro is used).

use crate::support::uvm_classes::is_component;
use crate::syntax::Cst;
use dv_model::{Design, Loc};

/// A file, line and column.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Where {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

/// A TLM port property of a component class.
#[derive(Clone, Debug)]
pub struct Port {
    pub component: String,
    pub name: String,
    pub ty: String,
    pub is_pull: bool,
    pub is_analysis: bool,
    pub at: Where,
}

/// A `connect()` call.
#[derive(Clone, Debug)]
pub struct Connection {
    pub in_class: String,
    pub source: String,
    pub target: String,
    pub at: Where,
}

/// A `uvm_config_db` set or get.
#[derive(Clone, Debug)]
pub struct ConfigAccess {
    pub set: bool,
    pub ty: String,
    pub in_class: String,
    pub inst: String,
    pub field: String,
    pub at: Where,
}

/// What the graph builders collected.
#[derive(Default, Debug)]
pub struct Graph {
    pub ports: Vec<Port>,
    pub connections: Vec<Connection>,
    pub config: Vec<ConfigAccess>,
    /// Each class declaration's first and last line, by class name (the
    /// first declaration of a name).
    pub spans: std::collections::HashMap<String, (Where, u32)>,
}

/// Where `loc` is reported, with `files` naming its file indices.
pub(crate) fn where_of(files: &[String], loc: Loc) -> Where {
    let name = |f: u32| files.get(f as usize).cloned().unwrap_or_default();
    match loc.expansion {
        Some((f, line)) => Where { file: name(f), line, column: 1 },
        None => Where { file: name(loc.file), line: loc.line, column: loc.column },
    }
}

impl Graph {
    /// The graph of `design`, with the syntax trees it was built from
    /// (`files`: the lint's file table).
    pub fn build(design: &Design, trees: &[sv_syntax::Tree], files: &[String]) -> Graph {
        // Number every file of every tree first, so all trees share one table.
        let mut locator = sv_sema::Locator::new(files.to_vec());
        for (ix, tree) in trees.iter().enumerate() {
            let c = Cst::new(tree, ix, locator);
            for f in 0..tree.files.len() as u32 {
                c.loc_at(sv_syntax::Origin { file: f, offset: 0 });
            }
            locator = c.into_locator();
        }
        let files = locator.into_files();
        let csts = crate::mixed_csts(trees, &files);
        let at = |loc: Loc| where_of(&files, loc);
        let mut g = Graph::default();
        // Ports: component properties whose type names a port.
        for cls in design.classes() {
            if !is_component(design, cls.id) || crate::support::uvm_classes::is_library(design, cls) {
                continue;
            }
            for p in &cls.properties {
                let is_pull = p.ty.contains("seq_item_pull_port");
                let is_analysis = p.ty.contains("analysis_port");
                if is_pull || is_analysis || p.ty.contains("_port") {
                    g.ports.push(Port {
                        component: cls.name.clone(),
                        name: p.name.clone(),
                        ty: p.ty.clone(),
                        is_pull,
                        is_analysis,
                        at: where_of(design.files(), p.loc),
                    });
                }
            }
        }
        for k in crate::rules::uvm::tlm_topology_checker::connections(&csts) {
            g.connections.push(Connection { in_class: k.class, source: k.source, target: k.target, at: at(k.loc) });
        }
        // One entry per call, as uvmcheck's ConfigDbGraph (a call typed in
        // several specializations once, by its first type).
        let mut seen = std::collections::HashSet::new();
        for k in crate::support::config_db_calls::config_db_calls(&csts) {
            if k.ty.is_empty() || !seen.insert((k.loc.file, k.loc.line, k.loc.column, k.set)) {
                continue;
            }
            g.config.push(ConfigAccess {
                set: k.set,
                ty: k.ty,
                in_class: k.class,
                inst: k.inst,
                field: k.field,
                at: at(k.loc),
            });
        }
        for c in &csts {
            for n in c.nodes(sv_syntax::SyntaxKind::ClassDecl) {
                let name = crate::support::slang_ast::decl_name(c, n);
                let (Some(first), Some(last)) = (c.first_tok(n), c.toks(n).last()) else { continue };
                let (a, b) = (at(c.loc(first)), at(c.loc(last)));
                g.spans.entry(name).or_insert((a, b.line));
            }
        }
        g
    }

    /// Ports no `connect()` names (pull and analysis ports only), as
    /// uvmcheck's `findUnconnectedPorts`.
    pub fn unconnected(&self) -> Vec<&Port> {
        self.ports
            .iter()
            .filter(|p| p.is_pull || p.is_analysis)
            .filter(|p| !self.connections.iter().any(|k| k.source.contains(&p.name) || k.target.contains(&p.name)))
            .collect()
    }
}
