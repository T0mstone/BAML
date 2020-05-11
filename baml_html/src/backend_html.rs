use baml_core::{Backend, Command, AST};
use std::collections::HashMap;

mod ppm_extensions {
    use super::*;
    use ppm::{predefined_commands::tools, CommandConfig, Engine, Free, Issue};

    pub fn make_engine(vars: HashMap<String, String>) -> Engine<Free> {
        let mut res = Engine::with_predefined_commands(vars);
        res.add_command("file_meta", get_file_meta_handler as _)
            .unwrap();
        res
    }

    pub fn get_file_meta_handler(mut cfg: CommandConfig) -> String {
        let mut spl = tools::splitn_args(2, cfg.body.clone()).into_iter();
        // let mut spl = cfg.body.splitn_not_escaped(2, ':', '\\', false).into_iter();
        let path = spl.next().unwrap();
        let key = match spl.next() {
            Some(x) => x,
            None => {
                cfg.issues
                    .push(cfg.missing_args("no metadata key provided"));
                return String::new();
            }
        };

        let path = cfg.process(path);
        let key = cfg.process(key);

        let content = match std::fs::read_to_string(path) {
            Ok(x) => x,
            Err(e) => {
                cfg.issues.push(Issue::io_error(
                    e,
                    cfg.cmd_span,
                    Some("while trying to open file"),
                ));
                return String::new();
            }
        };

        let meta = baml_core::get_metadata(content);
        cfg.process(meta.get(&key).cloned().unwrap_or_default())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HtmlTag {
    tag_name: String,
    attributes: Vec<(String, String)>,
    child_nodes: Vec<DomNode>,
}

impl ToString for HtmlTag {
    fn to_string(&self) -> String {
        let x1: String = self
            .attributes
            .iter()
            .map(|(k, v)| format!("{}={:?}", k, v))
            .collect::<Vec<_>>()
            .join(" ");

        let x2: String = self
            .child_nodes
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(" ");

        format!(
            "<{t}{}{}>{}</{t}>",
            if x1.is_empty() { "" } else { " " },
            x1,
            x2,
            t = self.tag_name
        )
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DomNode {
    Tag(HtmlTag),
    Text(String),
}

impl ToString for DomNode {
    fn to_string(&self) -> String {
        match self {
            DomNode::Tag(t) => t.to_string(),
            DomNode::Text(s) => s.clone(),
        }
    }
}

// impl DomNode {
//     pub fn child_nodes(&self) -> &[Self] {
//         match self {
//             DomNode::Text(_) => &[],
//             DomNode::Tag(t) => &t.child_nodes,
//         }
//     }
//
//     // pub fn child_nodes_mut(&mut self) -> Option<&mut Vec<Self>> {
//     //     match self {
//     //         DomNode::Text(_) => &mut vec![],
//     //         DomNode::Tag(t) => &mut t.child_nodes,
//     //     }
//     // }
// }

#[derive(Clone)]
pub struct BackendHtml {
    template: String,
    vars: HashMap<String, String>,
    special_vars: HashMap<String, String>,
}

impl BackendHtml {
    // pub fn from_template_string_and_dir(template: String, dir: PathBuf) -> Self {
    //     Self {
    //         template_engine: TemplateEngine::from_string_and_dir(template, dir),
    //     }
    // }
    //
    // pub fn from_template_file<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
    //     Ok(Self {
    //         template_engine: TemplateEngine::new(path, HashMap::new())?,
    //     })
    // }

    pub fn new(template: String, vars: HashMap<String, String>) -> Self {
        Self {
            template,
            vars,
            special_vars: HashMap::new(),
        }
    }

    pub fn node_from_command(&mut self, cmd: Command) -> DomNode {
        // todo: improve this
        DomNode::Tag(HtmlTag {
            tag_name: cmd.cmd,
            attributes: cmd.attributes,
            child_nodes: cmd
                .arguments
                .into_iter()
                .filter_map(|arg| self.handle_node(arg))
                .collect(),
        })
    }

    pub fn set_special_vars(&mut self, content_var: String, meta: &HashMap<String, String>) {
        if !self.special_vars.is_empty() {
            self.special_vars.clear()
        }
        for (k, v) in meta {
            self.special_vars.insert(format!("!{}", k), v.clone());
        }
        self.special_vars.insert("content".to_string(), content_var);
    }

    pub fn main(&mut self) -> String {
        let mut engine = ppm_extensions::make_engine(self.vars.clone());
        engine.vars.extend(self.vars.clone());
        engine.vars.extend(self.special_vars.clone());
        let (res, is) = engine.process_new(self.template.clone());
        for issue in is {
            eprintln!("warning: {}", issue.display(&self.template));
        }
        res
    }
}

impl Backend for BackendHtml {
    type Rendered = DomNode;
    type Output = String;

    fn backend_id() -> &'static str {
        "html"
    }

    fn emit_text(&mut self, text: String) -> DomNode {
        DomNode::Text(text.replace("\n", "<br />\n"))
    }

    fn run_command(&mut self, mut cmd: Command) -> Option<DomNode> {
        match cmd.backend.as_deref() {
            Some("html") => {
                // this serves the purpose of allowing you to insert any html tag with nice syntax
                // you could probably insert a tag as raw html but that's ugly
                if cmd.cmd.starts_with("tag.") {
                    cmd.cmd.replace_range(0..4, "");
                    Some(self.node_from_command(cmd))
                } else {
                    None
                }
            }
            Some(_) => None,
            None => {
                // todo: handle some of these commands differently
                Some(self.node_from_command(cmd))
            }
        }
    }

    fn compile_ast(&mut self, ast: AST) -> String {
        let content = ast
            .nodes
            .into_iter()
            .filter_map(|node| self.handle_node(node))
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("");
        self.set_special_vars(content, &ast.metadata);
        self.main()
    }
}
