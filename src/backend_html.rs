use crate::{Backend, Command, AST};
use std::collections::HashMap;

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
        println!("{}", x1);

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
pub struct BackendHtml;

impl BackendHtml {
    // fn get_curr(&mut self) -> &mut DomNode {
    //     let mut res = &mut self.inner;
    //     for i in self.curr {
    //         res = &mut res.child_nodes_mut()[i];
    //     }
    //     res
    // }
}

impl Backend for BackendHtml {
    type Rendered = DomNode;
    type Output = String;

    fn backend_id() -> &'static str {
        "html"
    }

    fn emit_text(&mut self, text: String) -> DomNode {
        DomNode::Text(text.replace("\n", "<br />"))
    }

    fn run_command(&mut self, cmd: Command) -> Option<DomNode> {
        if cmd.backend.is_some() {
            return None;
        }
        // very lazy - just to get it to work
        Some(DomNode::Tag(HtmlTag {
            tag_name: cmd.cmd,
            attributes: cmd.attributes,
            child_nodes: cmd
                .arguments
                .into_iter()
                .filter_map(|arg| self.handle_node(arg))
                .collect(),
        }))
    }

    fn compile_ast(&mut self, ast: AST) -> String {
        // fixme: for now - as a very crude first impl, we only use the content
        ast.nodes
            .into_iter()
            .filter_map(|node| self.handle_node(node))
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("")
    }
}
