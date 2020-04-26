/*
Language items:
In the following, a space denotes r'\s+'

Escaped chars:
'\#', '\;', '\\', '\!', '\.', '\@', '\[', '\]'

'\<LF>' deletes the line feed

<Text>
Text to be displayed

# Comment
Comment

[<Cmd> <Args>]
(Within Text) A Command Call
Args are separated with ';'
Cmd can end with a dict literal, denoting its attributes
-> dict literals are '{' (key '=' value),* '}'

!<key> <value> <EOL>
(At start of line) Setting Metadata

.<Cmd> <Args> <EOL>
A Command Call on a single line

@<Backend>[Stuff]
Backend specific stuff
*/

pub use self::parser::parse;
use std::collections::HashMap;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Command {
    pub backend: Option<String>,
    pub cmd: String,
    pub attributes: HashMap<String, String>,
    pub arguments: Vec<ASTNode>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ASTNode {
    Text(String),
    CommandCall(Command),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AST {
    metadata: HashMap<String, String>,
    nodes: Vec<ASTNode>,
}

pub trait Backend {
    type Rendered;
    type Output;

    fn backend_id() -> &'static str;

    fn emit_text(&mut self, text: String) -> Self::Rendered;

    // todo: commands such as 'sec', 'b' or 'i' should get their own function and an enum type
    fn run_command(&mut self, cmd: Command) -> Option<Self::Rendered>;

    fn handle_node(&mut self, node: ASTNode) -> Option<Self::Rendered> {
        Some(match node {
            ASTNode::Text(s) => self.emit_text(s),
            ASTNode::CommandCall(c) => {
                if c.backend.is_none() || c.backend.as_deref() == Some(Self::backend_id()) {
                    self.run_command(c)?
                } else {
                    return None;
                }
            }
        })
    }

    fn compile_ast(&mut self, ast: AST) -> Self::Output;
}

// mod auto_escape;
mod parser;

pub mod backend_html;
