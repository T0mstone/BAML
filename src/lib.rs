/*
Language items:
In the following, a space denotes r'\s+'

Escaped chars:
'\<LF>' -> ''
'\<SPACE>' -> ''
    - only the true space character, not any other whitespace
    - used to opt into whitespace at the start of command args
'\<x>' -> '<x>'

<Text>
Text to be displayed

# Comment
Comment

[<Cmd> <Args>]
(Within Text) A Command Call
Args are separated with ';'
Cmd can end with a dict literal, denoting its attributes
-> dict literals are '{' (key '=' value),* '}'
-> Cmd can be backend@cmd for backend specific stuff

!<key> <value> <EOL>
(At start of line) Setting Metadata

.<Cmd> <Args> <EOL>
A Command Call on a single line
*/

pub use self::parser::parse;
use std::collections::HashMap;
use std::str::FromStr;

#[allow(unused)]
macro_rules! dbgs {
    ($($e:expr),+) => {
        println!(
            concat!(
                "[",
                file!(),
                "@",
                line!(),
                "] "
                $(
                , stringify!($e),
                " = {:?}",
                )", "+
            ),
            $($e),+
        )
    };
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Command {
    pub backend: Option<String>,
    pub cmd: String,
    pub attributes: HashMap<String, String>,
    pub arguments: Vec<ASTNode>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum BasicCommandType {
    Bold,
    Italic,
    Section(usize),
    VertSpace,
    HorSpace,
    Image,
}

impl FromStr for BasicCommandType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use self::BasicCommandType::*;
        Ok(match s {
            "b" => Bold,
            "i" => Italic,
            "sec" => Section(0),
            "vspace" => VertSpace,
            "hspace" => HorSpace,
            "img" => Image,
            s if s.starts_with("sec") && s[3..].chars().all(|c| c.is_ascii_digit()) => {
                Section(s[3..].parse().unwrap())
            }
            _ => return Err(()),
        })
    }
}

impl ToString for BasicCommandType {
    fn to_string(&self) -> String {
        use self::BasicCommandType::*;
        match self {
            Bold => "b",
            Italic => "i",
            Section(0) => "sec",
            Section(n) => return format!("sec{}", n),
            VertSpace => "vspace",
            HorSpace => "hspace",
            Image => "img",
        }
        .to_string()
    }
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

    fn run_command(&mut self, cmd: Command) -> Option<Self::Rendered>;

    fn run_basic_command(
        &mut self,
        cmd: BasicCommandType,
        attrs: HashMap<String, String>,
        args: Vec<ASTNode>,
    ) -> Option<Self::Rendered> {
        self.run_command(Command {
            backend: None,
            cmd: cmd.to_string(),
            attributes: attrs,
            arguments: args,
        })
    }

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

pub mod backend_html;
mod parser;
pub mod template;
