// use self::util::*;
use crate::parser::util::{str_split_keep_sep, Containerized};
use crate::{ASTNode, Command, AST};
use std::collections::HashMap;
use tlib::iter_tools::{IterSplit, SplitNotEscapedString};

#[path = "parser_util.rs"]
pub mod util;

mod pipeline {
    use crate::parser::util::{containerize, Containerized};
    use crate::parser::{parse_command, ParseCommandErr};
    use crate::ASTNode;
    use std::collections::HashMap;
    use tlib::iter_tools::AutoEscape;

    /// Extracts comments and metadata as well as deleting escaped line-feeds
    // todo: proper error handling
    pub fn preprocess(s: String) -> (HashMap<String, String>, String) {
        let mut meta = HashMap::new();
        let mut res = Vec::new();
        let s = s.replace("\\\n", "");
        for line in s.split('\n') {
            if line.starts_with('!') {
                let i = line
                    .char_indices()
                    .find(|(_, c)| c.is_whitespace())
                    .expect("Found Metadata without value")
                    .0;
                let i1 = line
                    .char_indices()
                    .find(|(ix, c)| ix > &i && !c.is_whitespace())
                    .expect("Found Metadata without value")
                    .0;
                meta.insert(line[1..i].to_string(), line[i1..].to_string());
            } else if line.contains('#') {
                let mut was_escaped = true;
                let line_iter = line
                    .split('#')
                    .take_while(|sl| std::mem::replace(&mut was_escaped, sl.ends_with('\\')))
                    .collect::<String>();
                if !line_iter.is_empty() {
                    res.push(line_iter);
                }
            } else {
                res.push(line.to_string());
            }
        }
        (meta, res.join("\n"))
    }

    /// Transforms single line function calls into proper ones
    pub fn desugar_slfcalls(s: String) -> String {
        s.split('\n')
            .map(|line| {
                if line.starts_with('.') {
                    format!("[{}]", &line[1..])
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    // note: vvv idk why but the IDE doesn't detect the very CLEAR AND UNAMBIGUOUS `use tlib::iter_tools::AutoEscape;` at the top
    // noinspection RsUnresolvedReference
    /// Containerizes the input, respecting escape characters
    fn parse_step1(s: String) -> Vec<Containerized<String>> {
        let mut iter = s.chars().auto_escape(|&c| c == '\\').peekable();

        containerize(
            &mut iter,
            |&(esc, c)| !esc && c == '[',
            |&(esc, c)| !esc && c == ']',
        )
        .into_iter()
        .map(|c| {
            c.map(|v| {
                v.into_iter()
                    .flat_map(|(esc, c)| if esc { vec!['\\', c] } else { vec![c] })
                    .collect::<String>()
            })
        })
        .collect()
    }

    fn parse_step2(v: Vec<Containerized<String>>) -> Result<Vec<ASTNode>, ParseCommandErr> {
        v.into_iter()
            .map(|c| match c {
                Containerized::Free(s) => Ok(ASTNode::Text(s)),
                Containerized::Contained(v) => {
                    let f = parse_command(v)?;
                    Ok(ASTNode::CommandCall(f(parse_step2)?))
                }
            })
            .collect()
    }

    pub fn parse_desugared(s: String) -> Result<Vec<ASTNode>, ParseCommandErr> {
        parse_step2(parse_step1(s))
    }
}

// pub fn split_unescaped_string<'a>(
//     s: &'a str,
//     sep: char,
//     max_len: Option<usize>,
//     // keep_sep will keep the separator (on the right side of the split)
//     keep_sep: bool,
//     keep_backslash: bool,
// ) -> impl Iterator<Item = String> + 'a {
//     s.chars()
//         .auto_escape(indicator('\\'))
//         .split_impl(max_len, move |&(esc, c)| !esc && c == sep, keep_sep)
//         .map(move |v| {
//             if keep_backslash {
//                 v.into_iter().flat_map(reverse_auto_escape).collect()
//             } else {
//                 v.into_iter().map(|(_, c)| c).collect()
//             }
//         })
// }

// note: vvv idk why but the IDE doesn't detect the very CLEAR AND UNAMBIGUOUS `use tlib::iter_tools::{IterSplit, SplitNotEscapedString};` at the top
// noinspection RsUnresolvedReference
fn parse_attrs(s: &str) -> Vec<(String, String)> {
    s.split_not_escaped::<Vec<_>>(';', '\\', false)
        .into_iter()
        // split_unescaped_string(s, ';', None, false, false)
        // .inspect(|s| println!("> {}", s))
        .filter(|s| s.contains('='))
        .map(|mut sl| {
            let i = sl.char_indices().find(|&(_, c)| c == '=').unwrap().0;
            let r = if i + 1 == sl.len() {
                String::new()
            } else {
                sl.split_off(i + 1)
            };
            // remove the eq sign
            let _ = sl.pop();
            (sl.trim().to_string(), r.trim_start().to_string())
        })
        .collect()
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ParseCommandErr {
    EmptyBody,
    CommandIsNotIdentifier,
}

// note: vvv idk why but the IDE doesn't detect the very CLEAR AND UNAMBIGUOUS `use tlib::iter_tools::{IterSplit, SplitNotEscapedString};` at the top
// noinspection RsUnresolvedReference
pub fn parse_command<
    F: FnMut(Vec<Containerized<String>>) -> Result<Vec<ASTNode>, ParseCommandErr>,
>(
    mut v: Vec<Containerized<String>>,
) -> Result<impl FnOnce(F) -> Result<Command, ParseCommandErr>, ParseCommandErr> {
    // section: parse initial command
    // dbgs!(v);

    if v.is_empty() {
        return Err(ParseCommandErr::EmptyBody);
    }

    let first = match v.remove(0) {
        Containerized::Free(s) => s,
        Containerized::Contained(_) => return Err(ParseCommandErr::CommandIsNotIdentifier),
    };

    // let mut spl = split_unescaped_string(&first, '{', Some(2), false, true);
    let mut spl = first
        .splitn_not_escaped::<Vec<_>>(2, '{', '\\', false)
        .into_iter();
    let cmd_raw_1 = spl.next().unwrap();

    if let Some(rest) = spl.next() {
        // `rest` is the start of an attribute: 'cmd{attr'
        v.insert(0, Containerized::Free(rest));
        v.insert(0, Containerized::Free("{".to_string()));
    }

    // split on the first whitespace because `cmd_raw_1` could contain more than the command`
    let mut spl = cmd_raw_1.splitn(2, char::is_whitespace);
    let cmd_raw_2 = spl.next().unwrap();

    if let Some(rest) = spl.next() {
        v.insert(0, Containerized::Free(rest.to_string()));
    }

    let mut spl = cmd_raw_2.splitn(2, '@').collect::<Vec<_>>();
    let cmd = spl.pop().unwrap().to_string();
    let backend = spl.pop().map(|s| s.to_string());

    // section: parse attributes
    // dbgs!(backend, cmd, v);

    let mut iter = v.into_iter().peekable();
    let mut put_before = None;

    let mut attrs = Vec::new();
    match iter.peek() {
        Some(Containerized::Free(s)) if s == "{" => {
            let mut attr_string = String::new();
            let mut lvl = 0;
            'outer: while let Some(el) = iter.next() {
                match el {
                    Containerized::Free(mut s) => {
                        for (i, c) in s.char_indices() {
                            match c {
                                '{' => lvl += 1,
                                '}' if lvl == 1 => {
                                    let mut right = s.split_off(i);
                                    attr_string.push_str(&s);
                                    right.remove(0);
                                    put_before = Some(right);
                                    break 'outer;
                                }
                                '}' => lvl -= 1,
                                _ => (),
                            }
                        }
                        attr_string.push_str(&s);
                    }
                    c @ Containerized::Contained(_) => {
                        let s = c.join("[", "]");
                        attr_string.extend(s);
                    }
                }
            }

            attrs = parse_attrs(&attr_string[1..]);
        }
        _ => (),
    }

    let iter = put_before
        .into_iter()
        .map(Containerized::Free)
        .chain(iter)
        .collect::<Vec<_>>()
        .into_iter()
        .peekable();

    // section: parse arguments
    // dbgs!(attrs);

    let spl = iter
        .flat_map(|c| match c {
            Containerized::Free(s) => str_split_keep_sep(&s, |&c| c == ';')
                .map(Containerized::Free)
                .collect::<Vec<_>>(),
            c => vec![c],
        })
        .split(
            |c| match c {
                Containerized::Free(s) => s == ";",
                _ => false,
            },
            false,
        )
        .collect::<Vec<_>>();

    Ok(move |f: F| {
        let args = spl
            .into_iter()
            .map(|mut v: Vec<Containerized<String>>| {
                if let Some(Containerized::Free(s)) = v.first_mut() {
                    // note: this is to allow users to opt into having whitespace at the start of args
                    *s = s.trim_start().replace("\\ ", "")
                }
                v
            })
            .map(f)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        Ok(Command {
            backend,
            cmd,
            attributes: attrs,
            arguments: args,
        })
    })
}

#[inline]
pub fn parse(s: String) -> Result<AST, ParseCommandErr> {
    use self::pipeline::*;
    let (meta, cont) = preprocess(s);
    let desugared = desugar_slfcalls(cont);
    Ok(AST {
        metadata: meta,
        nodes: parse_desugared(desugared)?,
    })
}

#[inline]
pub fn get_metadata(s: String) -> HashMap<String, String> {
    self::pipeline::preprocess(s).0
}
