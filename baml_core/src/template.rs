//! template syntax:
//! - `%content` is replaced by the content - the meat of the file
//! - `%{x}` is replaced by the value of variable 'x'
//! - `%perc` is replaced by `%`
//! - `%run(cmd)` runs a command
//! - `%forfiles(args) %( ... %)` is a for loop that runs on files in a directory
//! - `%setext(ext:path)` invokes `Path::with_extension`
//! - `%alt(args)` (args separated by `:`) takes the first non-empty arg

use crate::parser::split_unescaped_string;
use crate::parser::util::{
    char_is_backslash, reverse_auto_escape, CreateAutoEscape, CreateTakeWhileLevelGe0,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TemplateEngine {
    pub vars: HashMap<String, String>,
    template: String,
    template_dir: PathBuf,
}

impl TemplateEngine {
    pub fn from_string_and_dir(template: String, template_dir: PathBuf) -> Self {
        Self {
            vars: HashMap::new(),
            template,
            template_dir,
        }
    }

    pub fn new<P: AsRef<Path>>(
        template_path: P,
        vars: HashMap<String, String>,
    ) -> std::io::Result<Self> {
        let template = std::fs::read_to_string(template_path.as_ref())?;
        // if template is a file, `parent()` never fails
        let template_dir = template_path.as_ref().parent().unwrap().to_path_buf();

        Ok(Self {
            vars,
            template,
            template_dir,
        })
    }

    fn process(
        &self,
        s: String,
        get_vars_from_file: &mut impl FnMut(PathBuf) -> HashMap<String, String>,
    ) -> String {
        let mut res = s;

        block_command::replace_all(&mut res, "%forfiles", |args, body| {
            let mut var = None;
            let mut path = None;
            let mut exclude_by_name = Vec::new();
            let mut include_by_name = Vec::new();
            let mut sort_key = None;
            let mut sort_descending = false;
            for arg in args.split(':') {
                let mut spl = arg.splitn(2, ' ');
                let verb = spl.next().unwrap();
                let object = match spl.next() {
                    Some(x) => x,
                    None => continue,
                };
                match verb {
                    "with" => var = Some(object),
                    "in" => path = Some(object),
                    "sort_key" => sort_key = Some(object),
                    "sort_order" => {
                        if ["-", "desc", "descending", "decreasing", "dec"].contains(&object) {
                            sort_descending = true;
                        } else if !["+", "asc", "ascemdomg", "increasing", "inc"].contains(&object)
                        {
                            eprintln!("warning: unknown sort_order: `{}`. Try `+` or `-`", object)
                        }
                    }
                    "exclude_name" => {
                        exclude_by_name.append(
                            &mut split_unescaped_string(object, ' ', None, false, false)
                                .collect::<Vec<_>>(),
                        );
                    }
                    "include_name" => {
                        include_by_name.append(
                            &mut split_unescaped_string(object, ' ', None, false, false)
                                .collect::<Vec<_>>(),
                        );
                    }
                    verb => eprintln!("warning: ignoring unrecognised verb `{}`", verb),
                }
            }
            let path = path.ok_or("No path given".to_string())?;

            let mut res = vec![];
            let mut dir = self.template_dir.clone();
            dir.push(path);
            for entry in std::fs::read_dir(dir).map_err(|e| format!("IOError: {:?}", e))? {
                match entry {
                    Ok(e) => {
                        if e.path().file_name().map_or(false, |filename| {
                            let filename_owner = filename.to_string_lossy();
                            let filename = filename_owner.as_ref();
                            exclude_by_name
                                .iter()
                                .any(|pattern| shell::matches_pattern(filename, pattern))
                                || include_by_name
                                    .iter()
                                    .all(|pattern| !shell::matches_pattern(filename, pattern))
                        }) {
                            // file is excluded or not included
                            continue;
                        }
                        let mut body = body.to_string();
                        let hm = {
                            let mut hm = self.vars.clone();
                            let file_vars = get_vars_from_file(e.path());
                            hm.extend(file_vars);
                            if let Some(var) = var {
                                if let Some(filename) = e.path().file_name() {
                                    hm.insert(
                                        var.to_string(),
                                        filename.to_string_lossy().to_string(),
                                    );
                                }
                            }
                            hm
                        };
                        var::replace_all(&mut body, &hm);
                        res.push((hm, body));
                    }
                    Err(e) => eprintln!(
                        "warning: encountered IOError while skimming directory: {:?}",
                        e
                    ),
                }
            }
            if let Some(key) = sort_key {
                res.sort_by(|(hml, _), (hmr, _)| {
                    let mut sl = key.to_string();
                    let mut sr = key.to_string();
                    var::replace_all(&mut sl, hml);
                    var::replace_all(&mut sr, hmr);
                    let ord = sl.cmp(&sr);
                    if sort_descending {
                        ord.reverse()
                    } else {
                        ord
                    }
                });
            }
            Ok(res.into_iter().map(|(_, s)| s).collect::<Vec<_>>().join(""))
        });

        var::replace_all(&mut res, &self.vars);

        basic_command::replace_all(&mut res, "%run", |args| {
            let (cmd, args) = shell::parse_cmd_and_args(args)?;
            let output = std::process::Command::new(cmd)
                .args(
                    args.into_iter()
                        .map(|s| self.process(s, get_vars_from_file)),
                )
                .current_dir(&self.template_dir)
                .output()
                .map_err(|e| format!("{:?}", e))?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });

        basic_command::replace_all(&mut res, "%setext", |args| {
            let mut spl = args.splitn(2, ':');
            let ext = spl.next().unwrap();
            let file = spl.next().ok_or("incomplete args for setext".to_string())?;
            Ok(Path::new(file)
                .with_extension(ext)
                .to_string_lossy()
                .to_string())
        });

        basic_command::replace_all(&mut res, "%alt", |args| {
            let spl = split_unescaped_string(args, ':', None, false, false);
            Ok(spl.fold(String::new(), |acc, x| {
                if acc.is_empty() && !x.is_empty() {
                    x
                } else {
                    acc
                }
            }))
        });

        res.replace("%perc", "%")
    }

    pub fn run(
        &self,
        mut get_vars_from_file: impl FnMut(PathBuf) -> HashMap<String, String>,
    ) -> String {
        self.process(self.template.clone(), &mut get_vars_from_file)
    }
}

mod basic_command {
    use super::*;

    pub fn parse_args(s: &str) -> String {
        let mut iter = s[1..].chars().auto_escape(char_is_backslash).peekable();
        iter.take_while_lvl_ge0(
            |&(esc, c)| !esc && c == '(',
            |&(esc, c)| !esc && c == ')',
            false,
        )
        .flat_map(reverse_auto_escape)
        .collect()
    }

    pub fn replace_all(
        s: &mut String,
        perc_cmd: &str,
        mut process: impl FnMut(&str) -> Result<String, String>,
    ) {
        while let Some(i) = s.find(perc_cmd) {
            let args = parse_args(&s[i + perc_cmd.len()..]);
            //   %abc(def)
            // i-^    [-] ^- end
            //   [--]  ^- args.len()
            //    ^- perc_cmd.len()
            let end = i + perc_cmd.len() + args.len() + 2;
            let res = process(&args);
            if let Err(ref e) = res {
                let end = if e.is_empty() {
                    ".".to_string()
                } else {
                    format!(": {}", e)
                };
                eprintln!(
                    "warning: `{}({})` failed to evaluate{}",
                    perc_cmd, args, end
                );
            }
            s.replace_range(i..end, &res.unwrap_or(String::new()))
        }
    }
}

mod block_command {
    use super::*;

    pub fn replace_all(
        s: &mut String,
        perc_cmd: &str,
        mut process: impl FnMut(&str, &str) -> Result<String, String>,
    ) {
        while let Some(i) = s.find(perc_cmd) {
            let tmp = match s.get(i + perc_cmd.len()..) {
                Some(s) => s,
                None => {
                    eprintln!(
                        "warning: incomplete block command ignored (command starts with `{}`)",
                        perc_cmd
                    );
                    continue;
                }
            };
            let args = basic_command::parse_args(tmp);
            //   %abc(def)
            // i-^    [-]^- last
            //   [--]  ^- args.len()
            //    ^- perc_cmd.len()
            let last = i + perc_cmd.len() + 1 + args.len();
            match s.get(last + 1..last + 3) {
                Some("%(") => (),
                Some(x) => {
                    eprintln!(
                        "warning: invalid block command ignored (starting command should end with `%(`, not `{}`)",
                        x
                    );
                    continue;
                }
                None => {
                    eprintln!(
                        "warning: incomplete block command ignored (starting command should end with `%(`, not EOF)",
                    );
                    continue;
                }
            }
            if s.len() == last + 3 {
                eprintln!(
                    "warning: incomplete block command ignored (expected ending command, found EOF)",
                );
                continue;
            }
            let tmp = s[last + 3..].chars().collect::<Vec<_>>();
            let mut iter = tmp.windows(2).peekable();
            // let body = eat_while_lvl_geq0(
            //     &mut iter,
            //     |_| false,
            //     // vvv this compares the two for equality - their length is always equal anyway
            //     |&sl| sl.iter().zip(end_perc_cmd.chars()).all(|(a, ref b)| a == b),
            // )
            let body = iter
                .take_while_lvl_ge0(|&sl| sl == &['%', '('], |&sl| sl == &['%', ')'], false)
                .map(|sl| sl[0])
                .collect::<String>();
            // %abc(...)%(defgh%)
            //         ^  [---]  ^- end
            //      last   ^-body.len()
            let end = last + body.len() + 5;
            let r = process(&args, &body);
            if let Err(ref e) = r {
                let end = if e.is_empty() {
                    ".".to_string()
                } else {
                    format!(": {}", e)
                };
                eprintln!(
                    "warning: `{}({})%( ... %)` failed to evaluate{}",
                    perc_cmd, args, end
                );
            }
            s.replace_range(i..end, &r.unwrap_or(String::new()))
        }
    }
}

mod var {
    use super::*;

    pub fn replace_all(s: &mut String, vars: &HashMap<String, String>) {
        while let Some(i) = s.find("%{") {
            if i + 2 == s.len() {
                eprintln!("warning: incomplete variable insertion ignored (at EOF)",);
                continue;
            }
            let mut iter = s[i + 2..].chars().auto_escape(char_is_backslash).peekable();

            // let arg = eat_while_lvl_geq0(
            //     &mut iter,
            //     |&(esc, c)| !esc && c == '{',
            //     |&(esc, c)| !esc && c == '}',
            // )
            let arg = iter
                .take_while_lvl_ge0(
                    |&(esc, c)| !esc && c == '{',
                    |&(esc, c)| !esc && c == '}',
                    false,
                )
                .flat_map(reverse_auto_escape)
                .collect::<String>();
            //   %{abc}
            // i-^ [-] ^- end
            //      ^- args.len()
            let end = i + arg.len() + 3;
            let res = match vars.get(&arg) {
                Some(s) => s,
                None => {
                    eprintln!(
                        "warning: `%{{{}}}` failed to evaluate: unknown variable",
                        arg
                    );
                    ""
                }
            };
            s.replace_range(i..end, res)
        }
    }
}

mod shell {
    use super::*;

    pub fn split_whitespace(s: &str) -> Vec<String> {
        let mut iter = s.chars().auto_escape(char_is_backslash).peekable();
        let mut res = vec![String::new()];
        while let Some((esc, c)) = iter.next() {
            let last = res.last_mut().unwrap();
            match c {
                '"' if !esc => {
                    last.extend(
                        // eat_while_lvl_geq0(&mut iter, |_| false, |&(esc, c)| !esc && c == '"')
                        iter.take_while_lvl_ge0(|_| false, |&(esc, c)| !esc && c == '"', false)
                            .flat_map(reverse_auto_escape),
                    );
                }
                ' ' => {
                    if !last.is_empty() {
                        res.push(String::new());
                    }
                }
                c => last.push(c),
            }
        }
        res
    }

    pub fn parse_cmd_and_args(s: &str) -> Result<(String, Vec<String>), &str> {
        let mut v = split_whitespace(s);
        if v.is_empty() {
            Err("empty command")
        } else {
            let first = v.remove(0);
            Ok((first, v))
        }
    }

    pub fn matches_pattern(mut s: &str, pat: &str) -> bool {
        let mut pat = pat.chars().enumerate().map(|(i, c)| (i == 0, c));
        let mut last_star = false;
        while let Some((first, c)) = pat.next() {
            if c == '*' {
                last_star = true;
            } else {
                last_star = false;
                match s.find(c) {
                    Some(i) => {
                        if first && i != 0 {
                            return false;
                        }
                        s = &s[i + 1..];
                    }
                    None => {
                        return false;
                    }
                }
            }
        }
        last_star || s.is_empty()
    }

    // pub fn parse_process_args(s: &str) -> Result<(&str, Vec<&str>), String> {
    //     let mut in_str = false;
    //     let mut split_at = vec![];
    //     for (esc, (i, c)) in s.char_indices().auto_escape(|&(_, c)| c == '\\').peekable() {
    //         match c {
    //             '"' if !esc => in_str = !in_str,
    //             ' ' if !in_str => split_at.push(i),
    //             _ => (),
    //         }
    //     }
    //     if split_at.is_empty() {
    //         return Ok((s, vec![]));
    //     }
    //     let iter = std::iter::once((0, *split_at.first().unwrap()))
    //         .chain(split_at.windows(2).map(|sl| match sl {
    //             &[i, j] => (i + 1, j),
    //             _ => unreachable!(),
    //         }))
    //         .chain(std::iter::once((*split_at.last().unwrap() + 1, s.len())));
    //     let mut res = vec![];
    //     for (i, j) in iter {
    //         let s = s[i..j].trim_matches('"');
    //         if !s.is_empty() {
    //             res.push(s);
    //         }
    //     }
    //     if res.is_empty() {
    //         Err("empty command".to_string())
    //     } else {
    //         let cmd = res.remove(0);
    //         Ok((cmd, res))
    //     }
    // }
}
//
// mod for_loop {
//     use super::*;
//
//     pub fn replace_all<S: AsRef<str>>(
//         template_dir: &Path,
//         args: Vec<S>,
//         body: &str,
//     ) -> Result<String, String> {
//         let mut iter = args.into_iter();
//         let method = iter.next().unwrap();
//         let domain = iter.next().unwrap();
//         let other = iter.collect::<Vec<_>>();
//
//         match method.as_ref() {
//             "dir" => {
//                 let mut path = Path::new(domain.as_ref()).to_path_buf();
//                 if path.is_relative() {
//                     let mut tmp = template_dir.to_path_buf();
//                     tmp.push(path);
//                     path = tmp;
//                 }
//                 let mut excluded_files = vec![];
//                 for s in other {
//                     let s = s.as_ref();
//                     let spl = s.splitn(3, ' ').collect::<Vec<_>>();
//                     match spl[0] {
//                         "exclude" => {
//                             let mut path = path.clone();
//                             match spl[1] {
//                                 "=" => {
//                                     path.push(spl[2]);
//                                     excluded_files.push(path);
//                                 }
//                                 "^=" => {
//                                     for dir in std::fs::read_dir(path)
//                                         .map_err(|e| format!("IOError: {:?}", e))?
//                                     {
//                                         if let Ok(dir) = dir {
//                                             let path = dir.path();
//                                             if let Some(name) = path.file_name() {
//                                                 if name.to_string_lossy().starts_with(spl[1]) {
//                                                     excluded_files.push(path);
//                                                 }
//                                             }
//                                         }
//                                     }
//                                 }
//                                 _ => (),
//                             }
//                         }
//                         "sort-by" => {
//                             let mut path = path.clone();
//                             match spl[1] {
//                                 "meta" => {
//                                     //
//                                     todo!()
//                                 }
//                                 _ => (),
//                             }
//                         }
//                         _ => (),
//                     }
//                 }
//                 let out = std::fs::read_dir(path).map_err(|e| format!("IOError: {:?}", e))?;
//                 let mut v = out
//                     .filter_map(|x| x.ok())
//                     .map(|e| e.path())
//                     .filter(|path| !excluded_files.contains(path))
//                     .map(|p| {
//                         let s = body.replace("%forvar", p.to_string_lossy().as_ref());
//                         // todo: compile the file and insert its metadata - maybe with %metaof(file:meta)
//                         // todo: sort by
//                         s
//                     })
//                     .collect::<Vec<_>>();
//
//                 Ok(v.join("\n"))
//             }
//             m => Err(format!("unknown for-loop method: {}", m)),
//         }
//     }
// }
