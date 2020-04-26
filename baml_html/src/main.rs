use self::backend_html::BackendHtml;
use baml_core::{parse, Backend};

mod backend_html;

fn main() {
    let s = include_str!("../../test.baml").to_string();
    let ast = parse(s);
    let s = BackendHtml::from_template_file("~/blog/index_template.html")
        .expect("IO error")
        .compile_ast(ast.expect("Err"));
    println!("=== done\n{}", s);
}
