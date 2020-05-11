use self::backend_html::BackendHtml;
use baml_core::{parse, Backend, AST};
use clap::{App, Arg};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod backend_html;

fn resolve_path<P: AsRef<Path>>(p: P, mut cwd: PathBuf) -> PathBuf {
    if p.as_ref().is_relative() {
        cwd.push(p);
        cwd
    } else {
        p.as_ref().to_path_buf()
    }
}

fn main() {
    let app = App::new("baml_html")
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::with_name("template")
                .short("t")
                .long("template")
                .takes_value(true)
                .default_value("template.html")
                .help("The template used for formatting the output"),
        )
        .arg(
            Arg::with_name("output-dir")
                .short("o")
                .long("output-dir")
                .takes_value(true)
                .default_value("out")
                .help("The directory to place the final files in"),
        )
        .arg(
            Arg::with_name("dry-run")
                .long("dry-run")
                .takes_value(true)
                .help("process the template once and with an empty string as input, the given value is the output filename"),
        )
        .arg(
            Arg::with_name("FILES")
                .takes_value(true)
                .multiple(true)
                .required_unless("dry-run")
                .help("The files to process, one at a time"),
        );

    let matches = app.get_matches();

    let cwd = match std::env::current_dir() {
        Ok(x) => x,
        Err(e) => {
            eprintln!("error: failed to get current working directory ({:?})", e);
            return;
        }
    };

    let template = resolve_path(matches.value_of_os("template").unwrap(), cwd.clone());

    let mut backend = BackendHtml::new(
        std::fs::read_to_string(template).unwrap_or_else(|_| "(%content%)".to_string()),
        HashMap::new(),
    );

    let output_dir = resolve_path(matches.value_of_os("output-dir").unwrap(), cwd.clone());
    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        eprintln!("error: couldn't create output directory ({:?})", e);
        return;
    }

    if let Some(s) = matches.value_of_os("dry-run") {
        let path = resolve_path(s, cwd);

        let mut out_path = output_dir;
        out_path.push(path.with_extension("html").file_name().unwrap());

        // we skip the parsing process and directly use an empty AST
        let ast = AST {
            metadata: HashMap::new(),
            nodes: Vec::new(),
        };

        let compiled = backend.compile_ast(ast);

        match std::fs::write(&out_path, compiled) {
            Ok(()) => println!("created {}", out_path.to_string_lossy()),
            Err(e) => {
                eprintln!(
                    "error: can't write to file {} ({:?})",
                    out_path.to_string_lossy(),
                    e
                );
            }
        }

        return;
    }

    for file in matches.values_of_os("FILES").unwrap() {
        let path = resolve_path(file, cwd.clone());
        if path.file_name().is_none() {
            eprintln!(
                "error: skipping {} as it has no file name",
                path.to_string_lossy()
            );
            continue;
        }

        let mut out_path = output_dir.clone();
        out_path.push(path.with_extension("html").file_name().unwrap());

        let cont = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "error: skipping {} because of error reading it ({:?})",
                    path.to_string_lossy(),
                    e
                );
                continue;
            }
        };

        let ast = match parse(cont) {
            Ok(x) => x,
            Err(e) => {
                eprintln!(
                    "error: skipping {} because of error parsing it ({:?})",
                    path.to_string_lossy(),
                    e
                );
                continue;
            }
        };

        let compiled = backend.compile_ast(ast);

        match std::fs::write(&out_path, compiled) {
            Ok(()) => println!("created {}", out_path.to_string_lossy()),
            Err(e) => {
                eprintln!(
                    "error: can't write to file {} ({:?})",
                    out_path.to_string_lossy(),
                    e
                );
                continue;
            }
        }
    }
}
