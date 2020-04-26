use crate::template::TemplateEngine;
use crate::{Backend, Command, AST};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HtmlTag {
    tag_name: String,
    attributes: HashMap<String, String>,
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

impl DomNode {
    pub fn child_nodes(&self) -> &[Self] {
        match self {
            DomNode::Text(_) => &[],
            DomNode::Tag(t) => &t.child_nodes,
        }
    }

    // pub fn child_nodes_mut(&mut self) -> Option<&mut Vec<Self>> {
    //     match self {
    //         DomNode::Text(_) => &mut vec![],
    //         DomNode::Tag(t) => &mut t.child_nodes,
    //     }
    // }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BackendHtml {
    template_engine: TemplateEngine,
}

impl BackendHtml {
    pub fn from_template_file<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        Ok(Self {
            template_engine: TemplateEngine::new(path, HashMap::new())?,
        })
    }

    pub fn node_from_command(&mut self, cmd: Command) -> DomNode {
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

    pub fn process_template(&mut self, content: String, meta: &HashMap<String, String>) -> String {
        for (k, v) in meta {
            self.template_engine
                .vars
                .insert(format!("!{}", k), v.clone());
        }
        self.template_engine
            .vars
            .insert("content".to_string(), content);
        self.template_engine.run(|p| {
            std::fs::read_to_string(p)
                .ok()
                .and_then(|s| crate::parse(s).ok())
                .map_or(HashMap::new(), |ast| {
                    ast.metadata
                        .into_iter()
                        .map(|(k, v)| (format!("!{}", k), v))
                        .collect()
                })
        })
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

    fn run_command(&mut self, cmd: Command) -> Option<DomNode> {
        if cmd.backend.is_some() {
            // note: this doesn't have any backend specific commands at the moment
            return None;
        }
        Some(self.node_from_command(cmd))
    }

    fn compile_ast(&mut self, ast: AST) -> String {
        let content = ast
            .nodes
            .into_iter()
            .filter_map(|node| self.handle_node(node))
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("");
        self.process_template(content, &ast.metadata)
    }
}
